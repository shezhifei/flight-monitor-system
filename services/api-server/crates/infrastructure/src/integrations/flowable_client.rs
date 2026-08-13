use async_trait::async_trait;
use fms_domain::ports::flowable_gateway::{FlowableGateway, FlowableGatewayError};
use reqwest::{multipart, StatusCode};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlowableClientError {
    #[error("resource not found")]
    NotFound,
    #[error("{0}")]
    Upstream(String),
}

#[derive(Debug)]
pub struct FlowableClient {
    base_url: String,
    username: String,
    password: String,
    api_prefix: String,
    http_client: reqwest::Client,
}

impl FlowableClient {
    /// Construct a `FlowableClient`, fail-fast on invalid `base_url`.
    ///
    /// Returns `Err` when `base_url` is empty, whitespace-only, or cannot be
    /// parsed as an absolute URL with a host.  This replaces the old `new`
    /// which silently swallowed parse errors via `.ok().unwrap_or_default()`,
    /// degrading to an empty `api_prefix` and a potentially broken origin.
    pub fn try_new(base_url: String, username: String, password: String) -> Result<Self, FlowableClientError> {
        let trimmed = base_url.trim().trim_end_matches('/').to_string();
        if trimmed.is_empty() {
            return Err(FlowableClientError::Upstream(
                "flowable base_url is empty or whitespace-only".to_string(),
            ));
        }
        let parsed = reqwest::Url::parse(&trimmed)
            .map_err(|error| FlowableClientError::Upstream(format!("flowable base_url parse failed: {error}")))?;
        if parsed.host_str().map(str::is_empty).unwrap_or(true) {
            return Err(FlowableClientError::Upstream(format!(
                "flowable base_url has no host: {trimmed}"
            )));
        }
        let api_prefix = parsed.path().trim_end_matches('/').to_string();
        let api_prefix = if api_prefix.is_empty() {
            String::new()
        } else {
            api_prefix
        };
        let origin = flowable_origin_from_url(&parsed).unwrap_or_else(|| trimmed.clone());

        Ok(Self {
            base_url: origin.trim_end_matches('/').to_string(),
            username,
            password,
            api_prefix,
            http_client: crate::http_client::shared_http_client(),
        })
    }

    pub async fn get_process_definitions(
        &self,
        key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let mut params = Vec::new();
        if let Some(key) = key.filter(|value| !value.trim().is_empty()) {
            params.push(("key", key.trim().to_string()));
        }
        if let Some(tenant_id) = tenant_id.filter(|value| !value.trim().is_empty()) {
            params.push(("tenantId", tenant_id.trim().to_string()));
        }
        let response = self
            .request(reqwest::Method::GET, "/repository/process-definitions")
            .query(&params)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_list_response(response).await
    }

    pub async fn get_process_definition(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::GET,
                &format!("/repository/process-definitions/{}", process_definition_id.trim()),
            )
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_optional_value_response(response).await
    }

    pub async fn get_process_definition_xml(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<String>, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::GET,
                &format!(
                    "/repository/process-definitions/{}/resourcedata",
                    process_definition_id.trim()
                ),
            )
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(self.response_error(response).await);
        }
        response
            .text()
            .await
            .map(Some)
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))
    }

    pub async fn get_deployments(
        &self,
        name: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let mut params = Vec::new();
        if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
            params.push(("nameLike", format!("%{}%", name.trim())));
        }
        if let Some(tenant_id) = tenant_id.filter(|value| !value.trim().is_empty()) {
            params.push(("tenantId", tenant_id.trim().to_string()));
        }
        let response = self
            .request(reqwest::Method::GET, "/repository/deployments")
            .query(&params)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_list_response(response).await
    }

    pub async fn deploy_process(
        &self,
        bpmn_xml: &str,
        deployment_name: Option<&str>,
        category: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<serde_json::Value, FlowableClientError> {
        let file_part = multipart::Part::text(bpmn_xml.to_string())
            .file_name("process.bpmn20.xml")
            .mime_str("application/xml")
            .map_err(|error: reqwest::Error| FlowableClientError::Upstream(error.to_string()))?;
        let mut form = multipart::Form::new().part("file", file_part);
        if let Some(name) = deployment_name.filter(|value| !value.trim().is_empty()) {
            form = form.text("deploymentName", name.trim().to_string());
        }
        if let Some(category) = category.filter(|value| !value.trim().is_empty()) {
            form = form.text("category", category.trim().to_string());
        }
        if let Some(tenant_id) = tenant_id.filter(|value| !value.trim().is_empty()) {
            form = form.text("tenantId", tenant_id.trim().to_string());
        }

        let response = self
            .request(reqwest::Method::POST, "/repository/deployments")
            .multipart(form)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_value_response(response).await
    }

    pub async fn delete_deployment(&self, deployment_id: &str, cascade: bool) -> Result<bool, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::DELETE,
                &format!("/repository/deployments/{}", deployment_id.trim()),
            )
            .query(&[("cascade", cascade.to_string())])
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_bool_response(response).await
    }

    pub async fn start_process_instance(
        &self,
        process_key: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
        business_key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Option<String>, FlowableClientError> {
        let mut payload = serde_json::Map::new();
        payload.insert(
            "processDefinitionKey".to_string(),
            serde_json::Value::String(process_key.trim().to_string()),
        );
        payload.insert(
            "variables".to_string(),
            serde_json::Value::Array(convert_variables(variables.cloned().unwrap_or_default())),
        );
        if let Some(business_key) = business_key.filter(|value| !value.trim().is_empty()) {
            payload.insert(
                "businessKey".to_string(),
                serde_json::Value::String(business_key.trim().to_string()),
            );
        }
        if let Some(tenant_id) = tenant_id.filter(|value| !value.trim().is_empty()) {
            payload.insert(
                "tenantId".to_string(),
                serde_json::Value::String(tenant_id.trim().to_string()),
            );
        }
        let response = self
            .request(reqwest::Method::POST, "/runtime/process-instances")
            .json(&serde_json::Value::Object(payload))
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        let payload = self.parse_value_response(response).await?;
        Ok(payload
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned))
    }

    pub async fn get_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(reqwest::Method::GET, "/runtime/process-instances")
            .query(filters)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_list_response(response).await
    }

    pub async fn get_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::GET,
                &format!("/runtime/process-instances/{}", process_instance_id.trim()),
            )
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_optional_value_response(response).await
    }

    pub async fn delete_process_instance(
        &self,
        process_instance_id: &str,
        delete_reason: Option<&str>,
    ) -> Result<bool, FlowableClientError> {
        let mut request = self.request(
            reqwest::Method::DELETE,
            &format!("/runtime/process-instances/{}", process_instance_id.trim()),
        );
        if let Some(reason) = delete_reason.filter(|value| !value.trim().is_empty()) {
            request = request.query(&[("deleteReason", reason.trim().to_string())]);
        }
        let response = request
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_bool_response(response).await
    }

    pub async fn get_tasks(&self, filters: &[(&str, String)]) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(reqwest::Method::GET, "/runtime/tasks")
            .query(filters)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_list_response(response).await
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(reqwest::Method::GET, &format!("/runtime/tasks/{}", task_id.trim()))
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_optional_value_response(response).await
    }

    pub async fn claim_task(&self, task_id: &str, user_id: &str) -> Result<bool, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/runtime/tasks/{}/claim", task_id.trim()),
            )
            .json(&serde_json::json!({ "userId": user_id.trim() }))
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_bool_response(response).await
    }

    pub async fn unclaim_task(&self, task_id: &str) -> Result<bool, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/runtime/tasks/{}/unclaim", task_id.trim()),
            )
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_bool_response(response).await
    }

    pub async fn complete_task(
        &self,
        task_id: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<bool, FlowableClientError> {
        let response = self
            .request(reqwest::Method::POST, &format!("/runtime/tasks/{}", task_id.trim()))
            .json(&serde_json::json!({
                "action": "complete",
                "variables": convert_variables(variables.cloned().unwrap_or_default()),
            }))
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_bool_response(response).await
    }

    pub async fn get_executions(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(reqwest::Method::GET, "/runtime/executions")
            .query(filters)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_list_response(response).await
    }

    pub async fn get_process_instance_variables(
        &self,
        process_instance_id: &str,
    ) -> Result<serde_json::Value, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::GET,
                &format!("/runtime/process-instances/{}/variables", process_instance_id.trim()),
            )
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_value_response(response).await
    }

    pub async fn set_process_instance_variables(
        &self,
        process_instance_id: &str,
        variables: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<bool, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/runtime/process-instances/{}/variables", process_instance_id.trim()),
            )
            .json(&serde_json::Value::Array(convert_variables(variables.clone())))
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_bool_response(response).await
    }

    pub async fn get_historic_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(reqwest::Method::GET, "/history/historic-process-instances")
            .query(filters)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_list_response(response).await
    }

    pub async fn get_historic_tasks(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(reqwest::Method::GET, "/history/historic-task-instances")
            .query(filters)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_list_response(response).await
    }

    pub async fn get_historic_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(
                reqwest::Method::GET,
                &format!("/history/historic-process-instances/{}", process_instance_id.trim()),
            )
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_optional_value_response(response).await
    }

    pub async fn get_historic_variable_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let response = self
            .request(reqwest::Method::GET, "/history/historic-variable-instances")
            .query(filters)
            .send()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))?;
        self.parse_list_response(response).await
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let normalized_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let url = format!("{}{}{}", self.base_url, self.api_prefix, normalized_path);
        self.http_client
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password))
    }

    async fn parse_list_response(
        &self,
        response: reqwest::Response,
    ) -> Result<Vec<serde_json::Value>, FlowableClientError> {
        let payload = self.parse_value_response(response).await?;
        if let Some(items) = payload.as_array() {
            return Ok(items.clone());
        }
        if let Some(items) = payload.get("data").and_then(serde_json::Value::as_array) {
            return Ok(items.clone());
        }
        Ok(Vec::new())
    }

    async fn parse_optional_value_response(
        &self,
        response: reqwest::Response,
    ) -> Result<Option<serde_json::Value>, FlowableClientError> {
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        self.parse_value_response(response).await.map(Some)
    }

    async fn parse_bool_response(&self, response: reqwest::Response) -> Result<bool, FlowableClientError> {
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if response.status().is_success() {
            Ok(true)
        } else {
            Err(self.response_error(response).await)
        }
    }

    async fn parse_value_response(
        &self,
        response: reqwest::Response,
    ) -> Result<serde_json::Value, FlowableClientError> {
        if response.status() == StatusCode::NOT_FOUND {
            return Err(FlowableClientError::NotFound);
        }
        if !response.status().is_success() {
            return Err(self.response_error(response).await);
        }
        response
            .json::<serde_json::Value>()
            .await
            .map_err(|error| FlowableClientError::Upstream(error.to_string()))
    }

    async fn response_error(&self, response: reqwest::Response) -> FlowableClientError {
        let status = response.status().as_u16();
        let body_result = response.text().await;
        let body = match body_result {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!(
                    status,
                    error = %error,
                    "failed to read flowable upstream error response body"
                );
                return FlowableClientError::Upstream(format!(
                    "flowable returned HTTP {status} (body read failed: {error})"
                ));
            }
        };
        let detail = body.trim();
        if detail.is_empty() {
            FlowableClientError::Upstream(format!("flowable returned HTTP {status}"))
        } else {
            let compact = detail.split_whitespace().collect::<Vec<_>>().join(" ");
            let truncated = if compact.chars().count() > 500 {
                format!("{}...", compact.chars().take(500).collect::<String>())
            } else {
                compact
            };
            FlowableClientError::Upstream(format!("flowable returned HTTP {status}: {truncated}"))
        }
    }
}

#[async_trait]
impl FlowableGateway for FlowableClient {
    async fn get_process_definitions(
        &self,
        key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_process_definitions(self, key, tenant_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_process_definition(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_process_definition(self, process_definition_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_process_definition_xml(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<String>, FlowableGatewayError> {
        FlowableClient::get_process_definition_xml(self, process_definition_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_deployments(
        &self,
        name: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_deployments(self, name, tenant_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn deploy_process(
        &self,
        bpmn_xml: &str,
        deployment_name: Option<&str>,
        category: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<serde_json::Value, FlowableGatewayError> {
        FlowableClient::deploy_process(self, bpmn_xml, deployment_name, category, tenant_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn delete_deployment(&self, deployment_id: &str, cascade: bool) -> Result<bool, FlowableGatewayError> {
        FlowableClient::delete_deployment(self, deployment_id, cascade)
            .await
            .map_err(map_gateway_error)
    }

    async fn start_process_instance(
        &self,
        process_key: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
        business_key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Option<String>, FlowableGatewayError> {
        FlowableClient::start_process_instance(self, process_key, variables, business_key, tenant_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_process_instances(self, filters)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_process_instance(self, process_instance_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn delete_process_instance(
        &self,
        process_instance_id: &str,
        delete_reason: Option<&str>,
    ) -> Result<bool, FlowableGatewayError> {
        FlowableClient::delete_process_instance(self, process_instance_id, delete_reason)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_tasks(&self, filters: &[(&str, String)]) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_tasks(self, filters)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_task(&self, task_id: &str) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_task(self, task_id).await.map_err(map_gateway_error)
    }

    async fn claim_task(&self, task_id: &str, user_id: &str) -> Result<bool, FlowableGatewayError> {
        FlowableClient::claim_task(self, task_id, user_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn unclaim_task(&self, task_id: &str) -> Result<bool, FlowableGatewayError> {
        FlowableClient::unclaim_task(self, task_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn complete_task(
        &self,
        task_id: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<bool, FlowableGatewayError> {
        FlowableClient::complete_task(self, task_id, variables)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_executions(&self, filters: &[(&str, String)]) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_executions(self, filters)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_process_instance_variables(
        &self,
        process_instance_id: &str,
    ) -> Result<serde_json::Value, FlowableGatewayError> {
        FlowableClient::get_process_instance_variables(self, process_instance_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn set_process_instance_variables(
        &self,
        process_instance_id: &str,
        variables: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<bool, FlowableGatewayError> {
        FlowableClient::set_process_instance_variables(self, process_instance_id, variables)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_historic_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_historic_process_instances(self, filters)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_historic_tasks(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_historic_tasks(self, filters)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_historic_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_historic_process_instance(self, process_instance_id)
            .await
            .map_err(map_gateway_error)
    }

    async fn get_historic_variable_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        FlowableClient::get_historic_variable_instances(self, filters)
            .await
            .map_err(map_gateway_error)
    }
}

fn map_gateway_error(error: FlowableClientError) -> FlowableGatewayError {
    match error {
        FlowableClientError::NotFound => FlowableGatewayError::NotFound,
        FlowableClientError::Upstream(message) => FlowableGatewayError::Upstream(message),
    }
}

fn convert_variables(variables: serde_json::Map<String, serde_json::Value>) -> Vec<serde_json::Value> {
    variables
        .into_iter()
        .map(|(name, value)| {
            let (var_type, normalized) = normalize_variable_value(value);
            serde_json::json!({
                "name": name,
                "value": normalized,
                "type": var_type,
            })
        })
        .collect()
}

fn normalize_variable_value(value: serde_json::Value) -> (&'static str, serde_json::Value) {
    match value {
        serde_json::Value::Bool(value) => ("boolean", serde_json::Value::Bool(value)),
        serde_json::Value::Number(value) => {
            if value.is_i64() || value.is_u64() {
                ("integer", serde_json::Value::Number(value))
            } else {
                ("double", serde_json::Value::Number(value))
            }
        }
        serde_json::Value::String(value) => ("string", serde_json::Value::String(value)),
        serde_json::Value::Null => ("string", serde_json::Value::Null),
        other => (
            "string",
            serde_json::Value::String(serde_json::to_string(&other).unwrap_or_else(|_| "{}".to_string())),
        ),
    }
}

fn flowable_origin_from_url(url: &reqwest::Url) -> Option<String> {
    let host = url.host_str()?.trim();
    if host.is_empty() {
        return None;
    }
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let mut origin = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        origin.push(':');
        origin.push_str(&port.to_string());
    }
    Some(origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowable_client_preserves_explicit_base_url_port() {
        let client = FlowableClient::try_new(
            "http://127.0.0.1:8082/flowable-rest/service".to_string(),
            "rest-admin".to_string(),
            "secret".to_string(),
        )
        .expect("valid flowable url");

        assert_eq!(client.base_url, "http://127.0.0.1:8082");
        assert_eq!(client.api_prefix, "/flowable-rest/service");
    }

    #[test]
    fn flowable_client_preserves_https_port_and_api_prefix() {
        let client = FlowableClient::try_new(
            "https://flowable.example.test:8443/flowable-rest/service/".to_string(),
            "rest-admin".to_string(),
            "secret".to_string(),
        )
        .expect("valid flowable url");

        assert_eq!(client.base_url, "https://flowable.example.test:8443");
        assert_eq!(client.api_prefix, "/flowable-rest/service");
    }

    // === Task 15: fail-fast configuration ===

    #[test]
    fn flowable_client_new_returns_ok_for_valid_url() {
        let result = FlowableClient::try_new(
            "http://127.0.0.1:8082/flowable-rest/service".to_string(),
            "rest-admin".to_string(),
            "secret".to_string(),
        );

        assert!(result.is_ok(), "valid URL should construct successfully");
        let client = result.unwrap();
        assert_eq!(client.base_url, "http://127.0.0.1:8082");
        assert_eq!(client.api_prefix, "/flowable-rest/service");
    }

    #[test]
    fn flowable_client_new_returns_err_for_empty_base_url() {
        let result = FlowableClient::try_new("".to_string(), "rest-admin".to_string(), "secret".to_string());

        assert!(result.is_err(), "empty base URL should fail-fast");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("base_url") || err_msg.contains("base URL") || err_msg.contains("empty"),
            "error should mention base_url/empty, got: {err_msg}"
        );
    }

    #[test]
    fn flowable_client_new_returns_err_for_whitespace_only_base_url() {
        let result = FlowableClient::try_new("   ".to_string(), "rest-admin".to_string(), "secret".to_string());

        assert!(result.is_err(), "whitespace-only base URL should fail-fast");
    }

    #[test]
    fn flowable_client_new_returns_err_for_invalid_url_scheme() {
        let result = FlowableClient::try_new(
            "not-a-url-at-all".to_string(),
            "rest-admin".to_string(),
            "secret".to_string(),
        );

        assert!(result.is_err(), "invalid URL should fail-fast");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("base_url") || err_msg.contains("URL") || err_msg.contains("parse"),
            "error should mention URL parse failure, got: {err_msg}"
        );
    }

    #[test]
    fn flowable_client_new_returns_err_for_relative_url() {
        let result = FlowableClient::try_new(
            "/flowable-rest/service".to_string(),
            "rest-admin".to_string(),
            "secret".to_string(),
        );

        assert!(result.is_err(), "relative URL without host should fail-fast");
    }
}
