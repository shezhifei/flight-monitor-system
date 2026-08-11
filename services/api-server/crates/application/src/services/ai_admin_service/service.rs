use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tokio::sync::RwLock;

use fms_domain::error::DomainError;
#[cfg(test)]
use fms_domain::models::ai_entity_config::AiEntityConfigRecord;
use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;

use crate::schemas::ai_schemas::{
    ConnectionProbeRequest, EntityConfigUpdate, EntityToolsUpdateRequest, SystemPromptUpdate,
};

use super::batch::{
    build_batch_request_payload, extract_batch_response_text, extract_batch_usage, normalize_api_format,
    BatchExecutionConfig, BatchUsage,
};
use super::catalog::{
    available_models, dedupe, infer_provider, normalize_string_list, registry_executor_type_name, tool_catalog,
    tool_categories_map, validate_tool_names,
};
use super::config::{
    default_entity_config, mask_config, merge_objects, merged_entity_config, metrics_to_value, remove_api_key,
    EntityRuntimeMetrics,
};
use super::schemas::{AiBatchRequestItem, AiBatchResultItem};

static JSON_NULL: serde_json::Value = serde_json::Value::Null;

pub struct AiAdminService {
    repo: Arc<dyn AiEntityConfigRepository + Send + Sync>,
    http_client: reqwest::Client,
    runtime_metrics: RwLock<HashMap<String, EntityRuntimeMetrics>>,
    #[cfg(test)]
    pub mock_completions: Arc<std::sync::Mutex<Vec<String>>>,
}

impl AiAdminService {
    fn build_pooled_reqwest_client() -> reqwest::Client {
        crate::http_client::shared_http_client()
    }

    pub fn new(repo: Arc<dyn AiEntityConfigRepository + Send + Sync>) -> Self {
        Self {
            repo,
            http_client: Self::build_pooled_reqwest_client(),
            runtime_metrics: RwLock::new(HashMap::new()),
            #[cfg(test)]
            mock_completions: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub async fn has_usable_ai_config(&self) -> Result<bool, DomainError> {
        let items = self.repo.find_all().await?;
        Ok(items.iter().any(|item| {
            item.config
                .get("api_key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
        }))
    }

    pub async fn list_entities_payload(&self) -> Result<serde_json::Value, DomainError> {
        let items = self.repo.find_all().await?;
        let entities = items
            .into_iter()
            .map(|item| {
                serde_json::json!({
                    "id": item.id,
                    "model": item.config.get("default_model").unwrap_or(&JSON_NULL),
                    "base_url": item.config.get("base_url").unwrap_or(&JSON_NULL),
                    "has_api_key": item.config.get("api_key").and_then(serde_json::Value::as_str).map(str::trim).is_some_and(|value| !value.is_empty()),
                    "asr_model": item.config.get("asr_model").unwrap_or(&JSON_NULL),
                    "tts_model": item.config.get("tts_model").unwrap_or(&JSON_NULL),
                    "tts_voice": item.config.get("tts_voice").unwrap_or(&JSON_NULL),
                    "media": item.config.get("media").unwrap_or(&JSON_NULL),
                    "realtime_audio_enabled": item.config
                        .get("media")
                        .and_then(|m| m.get("realtime"))
                        .and_then(|r| r.get("enabled"))
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    "tool_categories": item.config.get("allowed_tool_categories").cloned().unwrap_or_else(|| serde_json::json!([])),
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::json!({
            "entities": entities,
            "total": entities.len(),
        }))
    }

    pub async fn list_entities(&self) -> Result<Vec<serde_json::Value>, DomainError> {
        let items = self.repo.find_all().await?;
        Ok(items
            .into_iter()
            .map(|item| {
                let config = merged_entity_config(&item.config);
                serde_json::json!({
                    "id": item.id,
                    "model": config
                        .get("default_model")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!("gpt-3.5-turbo")),
                    "status": "active",
                })
            })
            .collect())
    }

    pub async fn get_entity_masked_config(&self, entity_id: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let Some(record) = self.repo.find_by_id(entity_id).await? else {
            return Ok(None);
        };
        Ok(Some(mask_config(record.config)))
    }

    pub async fn get_entity_runtime_config(&self, entity_id: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let Some(record) = self.repo.find_by_id(entity_id).await? else {
            return Ok(None);
        };
        Ok(Some(serde_json::Value::Object(merged_entity_config(&record.config))))
    }

    pub async fn update_entity(
        &self,
        entity_id: &str,
        update: EntityConfigUpdate,
    ) -> Result<serde_json::Value, DomainError> {
        let mut patch = serde_json::to_value(update).map_err(|error| DomainError::Internal(error.to_string()))?;

        if let Some(enabled) = patch.get("realtime_audio_enabled").and_then(serde_json::Value::as_bool) {
            if let Some(patch_obj) = patch.as_object_mut() {
                let media = patch_obj.entry("media").or_insert_with(|| serde_json::json!({}));
                if let Some(media_obj) = media.as_object_mut() {
                    let realtime = media_obj.entry("realtime").or_insert_with(|| serde_json::json!({}));
                    if let Some(rt_obj) = realtime.as_object_mut() {
                        rt_obj.insert("enabled".to_string(), serde_json::json!(enabled));
                    }
                }
                patch_obj.remove("realtime_audio_enabled");
            }
        }

        let merged = self.merge_entity_config(entity_id, patch).await?;
        Ok(remove_api_key(merged))
    }

    pub async fn get_entity_prompt(&self, entity_id: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let Some(record) = self.repo.find_by_id(entity_id).await? else {
            return Ok(None);
        };
        Ok(Some(serde_json::json!({
            "prompt": record.config.get("system_prompt").cloned().unwrap_or_else(|| serde_json::json!(""))
        })))
    }

    pub async fn get_entity_status(&self, entity_id: &str) -> Result<serde_json::Value, DomainError> {
        let record = self
            .repo
            .find_by_id(entity_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "AiEntityConfig",
                id: entity_id.to_string(),
            })?;
        let metrics = self
            .runtime_metrics
            .read()
            .await
            .get(entity_id)
            .cloned()
            .unwrap_or_default();

        Ok(serde_json::json!({
            "id": record.id,
            "metrics": metrics_to_value(&metrics),
            "config": remove_api_key(serde_json::Value::Object(merged_entity_config(&record.config))),
        }))
    }

    pub async fn process_batch(
        &self,
        entity_id: &str,
        requests: Vec<AiBatchRequestItem>,
    ) -> Result<Vec<AiBatchResultItem>, DomainError> {
        let config = self.load_batch_execution_config(entity_id).await?;
        let mut results = Vec::with_capacity(requests.len());

        for request in requests {
            let (result, usage) = self.process_single_batch_item(&config, request).await;
            self.record_batch_metrics(entity_id, &config, &result, usage).await;
            results.push(result);
        }

        Ok(results)
    }

    #[cfg(test)]
    pub fn set_next_chat_completion(&self, completion: &str) {
        self.mock_completions.lock().unwrap().push(completion.to_string());
    }

    pub async fn complete_text(&self, entity_id: &str, prompt: &str) -> Result<String, DomainError> {
        #[cfg(test)]
        {
            let mut completions = self.mock_completions.lock().unwrap();
            if !completions.is_empty() {
                return Ok(completions.remove(0));
            }
        }
        let config = self.load_batch_execution_config(entity_id).await?;
        let request = AiBatchRequestItem {
            request_id: ulid::Ulid::new().to_string(),
            content: prompt.to_string(),
            metadata: None,
        };
        let (result, usage) = self.process_single_batch_item(&config, request).await;
        self.record_batch_metrics(entity_id, &config, &result, usage).await;
        if result.success {
            Ok(result.response.unwrap_or_default())
        } else {
            Err(DomainError::Internal(
                result.error.unwrap_or_else(|| "AI request failed".to_string()),
            ))
        }
    }

    pub async fn update_entity_prompt(&self, entity_id: &str, data: SystemPromptUpdate) -> Result<(), DomainError> {
        self.merge_entity_config(
            entity_id,
            serde_json::json!({
                "system_prompt": data.prompt,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn get_entity_tools(&self, entity_id: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let Some(record) = self.repo.find_by_id(entity_id).await? else {
            return Ok(None);
        };
        Ok(Some(serde_json::json!({
            "allowed_tool_categories": record.config.get("allowed_tool_categories").cloned().unwrap_or_else(|| serde_json::json!([])),
            "allowed_tools": record.config.get("allowed_tools").unwrap_or(&JSON_NULL),
            "denied_tools": record.config.get("denied_tools").cloned().unwrap_or_else(|| serde_json::json!([])),
        })))
    }

    pub async fn update_entity_tools(
        &self,
        entity_id: &str,
        data: EntityToolsUpdateRequest,
    ) -> Result<serde_json::Value, DomainError> {
        let catalog = tool_catalog();
        let known_tools = catalog
            .iter()
            .map(|item| item.name.to_string())
            .collect::<BTreeSet<_>>();
        let known_categories = tool_categories_map();

        let payload = serde_json::to_value(data).map_err(|error| DomainError::Internal(error.to_string()))?;
        let mut normalized = serde_json::Map::new();

        if let Some(items) = payload
            .get("allowed_tool_categories")
            .and_then(serde_json::Value::as_array)
        {
            let categories = normalize_string_list(items)
                .into_iter()
                .map(|value| value.to_lowercase())
                .collect::<Vec<_>>();
            let invalid = categories
                .iter()
                .filter(|value| !known_categories.contains_key(value.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if !invalid.is_empty() {
                return Err(DomainError::ValidationError(format!(
                    "无效工具类别: {}",
                    invalid.join(", ")
                )));
            }
            normalized.insert(
                "allowed_tool_categories".to_string(),
                serde_json::json!(dedupe(categories)),
            );
        }

        if let Some(items) = payload.get("allowed_tools").and_then(serde_json::Value::as_array) {
            let tools = normalize_string_list(items);
            validate_tool_names(&tools, &known_tools)?;
            normalized.insert("allowed_tools".to_string(), serde_json::json!(dedupe(tools)));
        }

        if let Some(items) = payload.get("denied_tools").and_then(serde_json::Value::as_array) {
            let tools = normalize_string_list(items);
            validate_tool_names(&tools, &known_tools)?;
            normalized.insert("denied_tools".to_string(), serde_json::json!(dedupe(tools)));
        }

        let merged = self
            .merge_entity_config(entity_id, serde_json::Value::Object(normalized))
            .await?;
        Ok(serde_json::json!({
            "allowed_tool_categories": merged.get("allowed_tool_categories").cloned().unwrap_or_else(|| serde_json::json!([])),
            "allowed_tools": merged.get("allowed_tools").unwrap_or(&JSON_NULL),
            "denied_tools": merged.get("denied_tools").cloned().unwrap_or_else(|| serde_json::json!([])),
        }))
    }

    pub fn list_available_models_payload(&self) -> serde_json::Value {
        let models = available_models()
            .into_iter()
            .map(|item| {
                serde_json::json!({
                    "id": item.id,
                    "name": item.name,
                    "provider": item.provider,
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "models": models,
            "total": models.len(),
        })
    }

    pub fn list_tool_categories_payload(&self) -> serde_json::Value {
        let catalog = tool_catalog();
        let categories = tool_categories_map()
            .iter()
            .map(|(&name, &display_name)| {
                let tools = catalog
                    .iter()
                    .filter(|item| item.category == name)
                    .map(|item| item.name.to_string())
                    .collect::<Vec<_>>();
                serde_json::json!({
                    "name": name,
                    "display_name": display_name,
                    "tool_count": tools.len(),
                    "tools": tools,
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "categories": categories,
            "total_categories": categories.len(),
        })
    }

    pub fn list_tools_payload(&self, category: Option<&str>) -> Result<Vec<serde_json::Value>, DomainError> {
        let normalized_category = category
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_lowercase());
        if let Some(category) = normalized_category.as_ref() {
            if !tool_categories_map().contains_key(category.as_str()) {
                return Err(DomainError::ValidationError(format!("无效的工具类别: {category}")));
            }
        }

        Ok(tool_catalog()
            .into_iter()
            .filter(|item| {
                normalized_category
                    .as_ref()
                    .map(|category| item.category == category)
                    .unwrap_or(true)
            })
            .map(|item| {
                serde_json::json!({
                    "name": item.name,
                    "description": item.description,
                    "category": item.category,
                    "operation_level": item.operation_level,
                    "side_effect": item.side_effect,
                    "parameters": item.parameters,
                    "required_params": item.required_params,
                })
            })
            .collect())
    }

    pub fn registry_status_payload(&self) -> serde_json::Value {
        let categories = tool_categories_map();
        let tools = tool_catalog()
            .into_iter()
            .map(|item| item.name.to_string())
            .collect::<Vec<_>>();
        let executors = categories
            .keys()
            .copied()
            .map(|category| {
                serde_json::json!({
                    "category": category,
                    "type": registry_executor_type_name(category),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "total_tools": tools.len(),
            "tools": tools,
            "executors": executors,
            "executors_count": executors.len(),
            "is_initialized": true,
        })
    }

    pub fn registry_initialize_payload(&self) -> serde_json::Value {
        let tools = tool_catalog()
            .into_iter()
            .map(|item| item.name.to_string())
            .collect::<Vec<_>>();
        serde_json::json!({
            "total_tools": tools.len(),
            "tools": tools,
        })
    }

    pub async fn test_connection(&self, request: ConnectionProbeRequest) -> Result<serde_json::Value, DomainError> {
        let mut config = default_entity_config();
        if let Some(entity_id) = request
            .entity_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let record = self
                .repo
                .find_by_id(entity_id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "AiEntityConfig",
                    id: entity_id.to_string(),
                })?;
            if let Some(current) = record.config.as_object() {
                merge_objects(&mut config, current.clone());
            }
        }

        if let Some(base_url) = request.base_url.as_deref() {
            config.insert(
                "base_url".to_string(),
                serde_json::Value::String(base_url.trim().to_string()),
            );
        }
        if let Some(api_key) = request.api_key.as_deref() {
            config.insert(
                "api_key".to_string(),
                serde_json::Value::String(api_key.trim().to_string()),
            );
        }

        let base_url = config
            .get("base_url")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("https://api.openai.com/v1");
        let api_key = config
            .get("api_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| DomainError::ValidationError("缺少 API Key，请先输入或保存有效密钥".to_string()))?;

        let models_url = format!("{}/models", base_url.trim_end_matches('/'));
        let response = self
            .http_client
            .get(&models_url)
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .header(CONTENT_TYPE, "application/json")
            .timeout(std::time::Duration::from_secs_f64(request.timeout.clamp(3.0, 60.0)))
            .send()
            .await
            .map_err(|_| DomainError::Internal("连接失败：无法访问指定地址或请求超时".to_string()))?;

        if !response.status().is_success() {
            return Err(DomainError::Internal(format!(
                "连接失败：上游返回 HTTP {}",
                response.status().as_u16()
            )));
        }

        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|_| DomainError::Internal("连接测试成功，请检查地址与凭证".to_string()))?;
        let models = payload
            .get("data")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                let id = item.get("id").and_then(serde_json::Value::as_str)?.trim();
                if id.is_empty() {
                    return None;
                }
                let provider = infer_provider(base_url);
                Some(serde_json::json!({
                    "id": id,
                    "name": id,
                    "provider": provider,
                }))
            })
            .collect::<Vec<_>>();

        Ok(serde_json::json!({
            "reachable": true,
            "base_url": base_url,
            "models": if request.include_models { serde_json::json!(models) } else { serde_json::json!([]) },
            "model_count": models.len(),
        }))
    }

    async fn merge_entity_config(
        &self,
        entity_id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value, DomainError> {
        let mut config = default_entity_config();
        if let Some(record) = self.repo.find_by_id(entity_id).await? {
            if let Some(current) = record.config.as_object() {
                merge_objects(&mut config, current.clone());
            }
        }
        let patch_object = patch
            .as_object()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, value)| !value.is_null())
            .collect::<serde_json::Map<String, serde_json::Value>>();
        merge_objects(&mut config, patch_object);
        let saved = self
            .repo
            .save(entity_id, &serde_json::Value::Object(config.clone()))
            .await?;
        Ok(saved.config)
    }

    async fn load_batch_execution_config(&self, entity_id: &str) -> Result<BatchExecutionConfig, DomainError> {
        let record = self
            .repo
            .find_by_id(entity_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "AiEntityConfig",
                id: entity_id.to_string(),
            })?;

        let config = merged_entity_config(&record.config);
        Ok(BatchExecutionConfig {
            base_url: config
                .get("base_url")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("https://api.openai.com/v1")
                .trim_end_matches('/')
                .to_string(),
            api_key: config
                .get("api_key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
            default_model: config
                .get("default_model")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("gpt-3.5-turbo")
                .to_string(),
            api_format: normalize_api_format(
                config
                    .get("api_format")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("chat_completions"),
            )
            .to_string(),
            temperature: config.get("temperature").and_then(serde_json::Value::as_f64),
            max_tokens: config.get("max_tokens").and_then(serde_json::Value::as_u64),
            timeout_seconds: config
                .get("timeout")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(30.0)
                .clamp(1.0, 120.0) as u64,
            max_retries: config
                .get("max_retries")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(3)
                .min(8) as usize,
            retry_delay_seconds: config
                .get("retry_delay")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.5)
                .clamp(0.0, 30.0),
            cost_per_1k_input: config
                .get("cost_per_1k_input")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0)
                .max(0.0),
            cost_per_1k_output: config
                .get("cost_per_1k_output")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0)
                .max(0.0),
        })
    }

    async fn process_single_batch_item(
        &self,
        config: &BatchExecutionConfig,
        request: AiBatchRequestItem,
    ) -> (AiBatchResultItem, Option<BatchUsage>) {
        match self.request_batch_response(config, &request.content).await {
            Ok(response) => (
                AiBatchResultItem {
                    request_id: request.request_id,
                    success: true,
                    response: Some(extract_batch_response_text(&response)),
                    error: None,
                    metadata: request.metadata,
                },
                Some(extract_batch_usage(&response)),
            ),
            Err(error) => (
                AiBatchResultItem {
                    request_id: request.request_id,
                    success: false,
                    response: None,
                    error: Some(error),
                    metadata: request.metadata,
                },
                None,
            ),
        }
    }

    async fn record_batch_metrics(
        &self,
        entity_id: &str,
        config: &BatchExecutionConfig,
        result: &AiBatchResultItem,
        usage: Option<BatchUsage>,
    ) {
        let mut metrics = self.runtime_metrics.write().await;
        let entry = metrics.entry(entity_id.to_string()).or_default();
        entry.requests += 1;

        if !result.success {
            entry.errors += 1;
            return;
        }

        let Some(usage) = usage else {
            return;
        };

        entry.total_tokens += usage.total_tokens;
        entry.total_cost += ((usage.input_tokens as f64 * config.cost_per_1k_input)
            + (usage.output_tokens as f64 * config.cost_per_1k_output))
            / 1000.0;
    }

    async fn request_batch_response(
        &self,
        config: &BatchExecutionConfig,
        user_content: &str,
    ) -> Result<serde_json::Value, String> {
        if config.api_key.trim().is_empty() {
            return Err("API key required".to_string());
        }

        let endpoint = if config.api_format == "responses" {
            format!("{}/responses", config.base_url)
        } else {
            format!("{}/chat/completions", config.base_url)
        };
        let payload = build_batch_request_payload(config, user_content);

        let mut last_error = None;
        for attempt in 0..=config.max_retries {
            let response = self
                .http_client
                .post(&endpoint)
                .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
                .header(CONTENT_TYPE, "application/json")
                .timeout(std::time::Duration::from_secs(config.timeout_seconds.max(1)))
                .json(&payload)
                .send()
                .await;

            match response {
                Ok(response) => {
                    let status = response.status();
                    let body = response
                        .json::<serde_json::Value>()
                        .await
                        .map_err(|error| error.to_string());
                    match body {
                        Ok(body) if status.is_success() => return Ok(body),
                        Ok(body) => {
                            last_error = Some(format!("AI request failed with status {}: {}", status, body));
                        }
                        Err(error) => {
                            last_error = Some(format!("AI response parse failed with status {}: {}", status, error));
                        }
                    }
                }
                Err(error) => last_error = Some(error.to_string()),
            }

            if attempt < config.max_retries {
                tokio::time::sleep(std::time::Duration::from_secs_f64(config.retry_delay_seconds)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| "AI request failed".to_string()))
    }
}
