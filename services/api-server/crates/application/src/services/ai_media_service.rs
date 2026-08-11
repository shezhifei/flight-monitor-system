//! AI 媒体服务 (AiMediaService)
//!
//! 提供 ASR（语音转写）和 TTS（文本合成语音）能力。
//! 通过 reqwest 直接调用 OpenAI-compatible API，不经过 Python Sidecar。

use metrics::counter;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::multipart;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

use fms_domain::models::ai_media::{
    builtin_voices, AsrParams, AsrResult, AsrSegment, SupportedFormats, TtsParams, TtsResult, VoiceInfo,
};

use crate::services::ai_admin_service::AiAdminService;

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum AiMediaError {
    /// 功能未启用
    Disabled(String),
    /// 参数验证失败
    Validation(String),
    /// 配置缺失或无效
    Configuration(String),
    /// 上游 API 调用失败
    Upstream(String),
    /// 文件大小超限
    FileTooLarge { max_mb: u64, actual_mb: u64 },
    /// 不支持的格式
    UnsupportedFormat(String),
    /// 内部错误
    Internal(String),
}

impl std::fmt::Display for AiMediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled(msg) => write!(f, "功能未启用: {msg}"),
            Self::Validation(msg) => write!(f, "参数验证失败: {msg}"),
            Self::Configuration(msg) => write!(f, "配置错误: {msg}"),
            Self::Upstream(msg) => write!(f, "上游 API 错误: {msg}"),
            Self::FileTooLarge { max_mb, actual_mb } => {
                write!(f, "文件大小超限: {actual_mb}MB (最大 {max_mb}MB)")
            }
            Self::UnsupportedFormat(fmt) => write!(f, "不支持的格式: {fmt}"),
            Self::Internal(msg) => write!(f, "内部错误: {msg}"),
        }
    }
}

impl std::error::Error for AiMediaError {}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

const AI_MEDIA_ASR_TOTAL: &str = "ai_media_asr_total";
const AI_MEDIA_ASR_ERROR_TOTAL: &str = "ai_media_asr_error_total";
const AI_MEDIA_TTS_TOTAL: &str = "ai_media_tts_total";
const AI_MEDIA_TTS_ERROR_TOTAL: &str = "ai_media_tts_error_total";

// ---------------------------------------------------------------------------
// 配置
// ---------------------------------------------------------------------------

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes"))
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_string(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

// ---------------------------------------------------------------------------
// AiMediaService
// ---------------------------------------------------------------------------

pub struct AiMediaService {
    ai_admin_service: Arc<AiAdminService>,
    http_client: reqwest::Client,
}

struct AiMediaApiConfig {
    base_url: String,
    api_key: String,
    asr_endpoint: Option<String>,
    tts_endpoint: Option<String>,
    default_asr_model: String,
    default_tts_model: String,
    default_tts_voice: String,
}

impl AiMediaService {
    pub fn new(ai_admin_service: Arc<AiAdminService>) -> Self {
        let http_client = crate::http_client::shared_http_client();

        Self {
            ai_admin_service,
            http_client,
        }
    }

    // -----------------------------------------------------------------------
    // 公共 API
    // -----------------------------------------------------------------------

    /// ASR 语音转写
    pub async fn transcribe(
        &self,
        entity_id: Option<&str>,
        audio_data: Vec<u8>,
        filename: &str,
        params: AsrParams,
    ) -> Result<AsrResult, AiMediaError> {
        if !self.is_asr_enabled() {
            return Err(AiMediaError::Disabled(
                "ASR 功能未启用 (AI_MEDIA_ASR_ENABLED=false)".into(),
            ));
        }

        // 验证文件大小
        let max_bytes = self.max_audio_size_bytes();
        if audio_data.len() as u64 > max_bytes {
            return Err(AiMediaError::FileTooLarge {
                max_mb: max_bytes / (1024 * 1024),
                actual_mb: audio_data.len() as u64 / (1024 * 1024),
            });
        }

        // 验证格式
        let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
        let supported = SupportedFormats::default();
        if !ext.is_empty() && !supported.asr_input.contains(&ext) {
            return Err(AiMediaError::UnsupportedFormat(format!(
                "不支持的音频格式: .{ext}（支持: {}）",
                supported.asr_input.join(", ")
            )));
        }

        // 加载 API 配置
        let api_config = self.load_api_config(entity_id).await?;
        let mut params = params;
        params.model = resolve_requested_or_default(&params.model, &api_config.default_asr_model);

        // 构建 multipart 请求
        let mime_type = guess_audio_mime(&ext);
        let file_part = multipart::Part::bytes(audio_data)
            .file_name(filename.to_string())
            .mime_str(mime_type)
            .map_err(|e| AiMediaError::Internal(e.to_string()))?;

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", params.model.clone());

        if let Some(language) = &params.language {
            form = form.text("language", language.clone());
        }
        if let Some(prompt) = &params.prompt {
            form = form.text("prompt", prompt.clone());
        }
        if params.response_format != "json" {
            form = form.text("response_format", params.response_format.clone());
        }
        if let Some(temp) = params.temperature {
            form = form.text("temperature", temp.to_string());
        }

        let url = api_config
            .asr_endpoint
            .clone()
            .unwrap_or_else(|| format!("{}/audio/transcriptions", api_config.base_url.trim_end_matches('/')));

        let timeout_secs = env_u64("AI_MEDIA_UPLOAD_TIMEOUT_SECS", 120);

        let _ = counter!(AI_MEDIA_ASR_TOTAL, "model" => params.model.clone());

        let response = self
            .http_client
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", api_config.api_key))
            .multipart(form)
            .timeout(Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| {
                let _ = counter!(AI_MEDIA_ASR_ERROR_TOTAL, "reason" => "request_failed");
                AiMediaError::Upstream(format!("ASR 请求失败: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            let _ = counter!(AI_MEDIA_ASR_ERROR_TOTAL, "reason" => format!("http_{status}"));
            return Err(AiMediaError::Upstream(format!("ASR 上游返回 HTTP {status}: {body}")));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| AiMediaError::Upstream(format!("ASR 响应解析失败: {e}")))?;

        Ok(AsrResult {
            text: body.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
            language: body.get("language").and_then(Value::as_str).map(str::to_string),
            duration_ms: body
                .get("duration")
                .and_then(Value::as_f64)
                .map(|d| (d * 1000.0) as u64),
            segments: body.get("segments").and_then(|s| {
                s.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|seg| {
                            Some(AsrSegment {
                                id: seg.get("id")?.as_u64()? as u32,
                                start: seg.get("start")?.as_f64()?,
                                end: seg.get("end")?.as_f64()?,
                                text: seg.get("text")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
            }),
            model: params.model,
        })
    }

    /// TTS 文本合成语音
    pub async fn synthesize(
        &self,
        entity_id: Option<&str>,
        text: &str,
        params: TtsParams,
    ) -> Result<TtsResult, AiMediaError> {
        if !self.is_tts_enabled() {
            return Err(AiMediaError::Disabled(
                "TTS 功能未启用 (AI_MEDIA_TTS_ENABLED=false)".into(),
            ));
        }

        // 验证输入文本
        let text = text.trim();
        if text.is_empty() {
            return Err(AiMediaError::Validation("合成文本不能为空".into()));
        }
        if text.len() > 4096 {
            return Err(AiMediaError::Validation(format!(
                "合成文本过长: {} 字符 (最大 4096)",
                text.len()
            )));
        }

        // 验证语速
        if !(0.25..=4.0).contains(&params.speed) {
            return Err(AiMediaError::Validation(format!(
                "语速必须在 0.25 ~ 4.0 之间，当前: {}",
                params.speed
            )));
        }

        // 验证输出格式
        let supported = SupportedFormats::default();
        if !supported.tts_output.contains(&params.response_format) {
            return Err(AiMediaError::UnsupportedFormat(format!(
                "不支持的 TTS 输出格式: {}（支持: {}）",
                params.response_format,
                supported.tts_output.join(", ")
            )));
        }

        // 加载 API 配置
        let api_config = self.load_api_config(entity_id).await?;
        let mut params = params;
        params.model = resolve_requested_or_default(&params.model, &api_config.default_tts_model);
        params.voice = resolve_requested_or_default(&params.voice, &api_config.default_tts_voice);

        let url = api_config
            .tts_endpoint
            .clone()
            .unwrap_or_else(|| format!("{}/audio/speech", api_config.base_url.trim_end_matches('/')));
        let payload = json!({
            "model": params.model,
            "input": text,
            "voice": params.voice,
            "response_format": params.response_format,
            "speed": params.speed,
        });

        let timeout_secs = env_u64("AI_MEDIA_UPLOAD_TIMEOUT_SECS", 120);

        let _ = counter!(AI_MEDIA_TTS_TOTAL,
            "model" => params.model.clone(),
            "voice" => params.voice.clone()
        );

        let response = self
            .http_client
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", api_config.api_key))
            .header(CONTENT_TYPE, "application/json")
            .json(&payload)
            .timeout(Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| {
                let _ = counter!(AI_MEDIA_TTS_ERROR_TOTAL, "reason" => "request_failed");
                AiMediaError::Upstream(format!("TTS 请求失败: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            let _ = counter!(AI_MEDIA_TTS_ERROR_TOTAL, "reason" => format!("http_{status}"));
            return Err(AiMediaError::Upstream(format!("TTS 上游返回 HTTP {status}: {body}")));
        }

        let audio_data = response
            .bytes()
            .await
            .map_err(|e| AiMediaError::Upstream(format!("TTS 音频数据读取失败: {e}")))?
            .to_vec();

        let content_type = TtsResult::mime_type_for_format(&params.response_format).to_string();

        Ok(TtsResult {
            audio_data,
            format: params.response_format,
            content_type,
        })
    }

    /// 查询 ASR/TTS 能力状态
    pub fn capabilities(&self) -> Value {
        let asr_enabled = self.is_asr_enabled();
        let tts_enabled = self.is_tts_enabled();
        let formats = SupportedFormats::default();

        json!({
            "asr": {
                "enabled": asr_enabled,
                "default_model": env_string("AI_MEDIA_ASR_DEFAULT_MODEL", "whisper-1"),
                "supported_input_formats": formats.asr_input,
                "supported_output_formats": formats.asr_output,
                "max_file_size_mb": env_u64("AI_MEDIA_MAX_AUDIO_SIZE_MB", 25),
            },
            "tts": {
                "enabled": tts_enabled,
                "default_model": env_string("AI_MEDIA_TTS_DEFAULT_MODEL", "tts-1"),
                "default_voice": env_string("AI_MEDIA_TTS_DEFAULT_VOICE", "alloy"),
                "supported_output_formats": formats.tts_output,
                "voices": self.list_voices(),
            },
        })
    }

    /// 列出可用的 TTS 声音
    pub fn list_voices(&self) -> Vec<VoiceInfo> {
        builtin_voices()
    }

    /// 列出支持的音频格式
    pub fn supported_formats(&self) -> SupportedFormats {
        SupportedFormats::default()
    }

    // -----------------------------------------------------------------------
    // 内部辅助方法
    // -----------------------------------------------------------------------

    fn is_asr_enabled(&self) -> bool {
        env_bool("AI_MEDIA_ASR_ENABLED", true)
    }

    fn is_tts_enabled(&self) -> bool {
        env_bool("AI_MEDIA_TTS_ENABLED", true)
    }

    fn max_audio_size_bytes(&self) -> u64 {
        env_u64("AI_MEDIA_MAX_AUDIO_SIZE_MB", 25) * 1024 * 1024
    }

    /// 从 AiAdminService 加载 entity 的 API 配置（base_url + api_key）
    async fn load_api_config(&self, entity_id: Option<&str>) -> Result<AiMediaApiConfig, AiMediaError> {
        // 尝试从 AiAdminService 获取首个可用的 entity 配置
        let entities_payload = self
            .ai_admin_service
            .list_entities_payload()
            .await
            .map_err(|e| AiMediaError::Configuration(format!("加载 AI 实体配置失败: {e}")))?;

        let entities = entities_payload
            .get("entities")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        // 如果指定了 entity_id，查找对应实体；否则取第一个有 api_key 的实体
        let target = if let Some(eid) = entity_id {
            entities
                .iter()
                .find(|e| e.get("id").and_then(Value::as_str) == Some(eid))
                .ok_or_else(|| AiMediaError::Configuration(format!("AI 实体 '{eid}' 不存在")))?
                .clone()
        } else {
            entities
                .into_iter()
                .find(|e| e.get("has_api_key").and_then(Value::as_bool).unwrap_or(false))
                .ok_or_else(|| AiMediaError::Configuration("未找到可用的 AI 实体配置，请先配置 API Key".into()))?
        };

        let entity_id_str = target.get("id").and_then(Value::as_str).unwrap_or("default");

        // 获取完整运行时配置（含 api_key），仅服务层内部使用。
        let config = self
            .ai_admin_service
            .get_entity_runtime_config(entity_id_str)
            .await
            .map_err(|e| AiMediaError::Configuration(format!("加载实体配置失败: {e}")))?
            .ok_or_else(|| AiMediaError::Configuration(format!("AI 实体 '{entity_id_str}' 配置为空")))?;

        let base_url = config
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("https://api.openai.com/v1")
            .to_string();

        let api_key = config
            .get("api_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .or_else(|| self.resolve_env_api_key(entity_id_str))
            .ok_or_else(|| {
                AiMediaError::Configuration(
                    "未找到可用的 API Key，请在 AI 实体中配置 api_key，或设置 AI_MEDIA_API_KEY / OPENAI_API_KEY 环境变量".into(),
                )
            })?;

        let endpoints = config.get("endpoints").and_then(Value::as_object);
        let asr_endpoint = endpoint_override(endpoints, "asr", &base_url);
        let tts_endpoint = endpoint_override(endpoints, "tts", &base_url);
        let default_asr_model = resolve_asr_model(&config, "");
        let default_tts_model = resolve_tts_model(&config, "");
        let default_tts_voice = resolve_tts_voice(&config, "");

        Ok(AiMediaApiConfig {
            base_url,
            api_key,
            asr_endpoint,
            tts_endpoint,
            default_asr_model,
            default_tts_model,
            default_tts_voice,
        })
    }

    fn resolve_env_api_key(&self, entity_id: &str) -> Option<String> {
        let env_key_specific = format!("AI_API_KEY_{}", entity_id.to_uppercase().replace('-', "_"));

        if let Ok(key) = std::env::var("AI_MEDIA_API_KEY") {
            let key = key.trim().to_string();
            if !key.is_empty() {
                return Some(key);
            }
        }

        if let Ok(key) = std::env::var(&env_key_specific) {
            let key = key.trim().to_string();
            if !key.is_empty() {
                return Some(key);
            }
        }

        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            let key = key.trim().to_string();
            if !key.is_empty() {
                return Some(key);
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

fn guess_audio_mime(ext: &str) -> &'static str {
    match ext {
        "mp3" | "mpga" => "audio/mpeg",
        "mp4" | "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "webm" => "audio/webm",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "mpeg" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

fn endpoint_override(endpoints: Option<&serde_json::Map<String, Value>>, key: &str, base_url: &str) -> Option<String> {
    let value = endpoints?
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())?;

    if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_string())
    } else {
        Some(format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            value.trim_start_matches('/')
        ))
    }
}

fn resolve_requested_or_default(requested: &str, fallback: &str) -> String {
    let requested = requested.trim();
    if requested.is_empty() {
        fallback.to_string()
    } else {
        requested.to_string()
    }
}

fn resolve_asr_model(config: &Value, requested: &str) -> String {
    resolve_media_string(
        requested,
        config,
        &["asr_model"],
        &[&["media", "asr", "model"], &["media", "asr_model"]],
        &env_string("AI_MEDIA_ASR_DEFAULT_MODEL", "whisper-1"),
    )
}

fn resolve_tts_model(config: &Value, requested: &str) -> String {
    resolve_media_string(
        requested,
        config,
        &["tts_model"],
        &[&["media", "tts", "model"], &["media", "tts_model"]],
        &env_string("AI_MEDIA_TTS_DEFAULT_MODEL", "tts-1"),
    )
}

fn resolve_tts_voice(config: &Value, requested: &str) -> String {
    resolve_media_string(
        requested,
        config,
        &["tts_voice"],
        &[&["media", "tts", "voice"], &["media", "tts_voice"]],
        &env_string("AI_MEDIA_TTS_DEFAULT_VOICE", "alloy"),
    )
}

fn resolve_media_string(
    requested: &str,
    config: &Value,
    flat_keys: &[&str],
    nested_paths: &[&[&str]],
    fallback: &str,
) -> String {
    let requested = requested.trim();
    if !requested.is_empty() {
        return requested.to_string();
    }

    for key in flat_keys {
        if let Some(value) = config_text(config.get(*key)) {
            return value;
        }
    }

    for path in nested_paths {
        if let Some(value) = config_path_text(config, path) {
            return value;
        }
    }

    fallback.to_string()
}

fn config_path_text(config: &Value, path: &[&str]) -> Option<String> {
    let mut current = config;
    for key in path {
        current = current.get(*key)?;
    }
    config_text(Some(current))
}

fn config_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_asr_params_are_valid() {
        let params = AsrParams::default();
        assert_eq!(params.model, "whisper-1");
        assert_eq!(params.response_format, "json");
        assert!(params.language.is_none());
    }

    #[test]
    fn default_tts_params_are_valid() {
        let params = TtsParams::default();
        assert_eq!(params.model, "tts-1");
        assert_eq!(params.voice, "alloy");
        assert_eq!(params.response_format, "mp3");
        assert!((params.speed - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn guess_audio_mime_works() {
        assert_eq!(guess_audio_mime("mp3"), "audio/mpeg");
        assert_eq!(guess_audio_mime("wav"), "audio/wav");
        assert_eq!(guess_audio_mime("ogg"), "audio/ogg");
        assert_eq!(guess_audio_mime("xyz"), "application/octet-stream");
    }

    #[test]
    fn tts_mime_type_for_format_works() {
        assert_eq!(TtsResult::mime_type_for_format("mp3"), "audio/mpeg");
        assert_eq!(TtsResult::mime_type_for_format("opus"), "audio/opus");
        assert_eq!(TtsResult::mime_type_for_format("wav"), "audio/wav");
        assert_eq!(TtsResult::mime_type_for_format("unknown"), "application/octet-stream");
    }

    #[test]
    fn supported_formats_default_has_entries() {
        let formats = SupportedFormats::default();
        assert!(!formats.asr_input.is_empty());
        assert!(!formats.asr_output.is_empty());
        assert!(!formats.tts_output.is_empty());
        assert!(formats.asr_input.contains(&"mp3".to_string()));
        assert!(formats.tts_output.contains(&"mp3".to_string()));
    }

    #[test]
    fn builtin_voices_are_populated() {
        let voices = builtin_voices();
        assert!(!voices.is_empty());
        assert!(voices.iter().any(|v| v.id == "alloy"));
        assert!(voices.iter().any(|v| v.id == "nova"));
    }

    #[test]
    fn entity_media_defaults_resolve_from_flat_config() {
        let config = json!({
            "asr_model": "whisper-large-v3",
            "tts_model": "gpt-4o-mini-tts",
            "tts_voice": "verse"
        });

        assert_eq!(resolve_asr_model(&config, ""), "whisper-large-v3");
        assert_eq!(resolve_tts_model(&config, ""), "gpt-4o-mini-tts");
        assert_eq!(resolve_tts_voice(&config, ""), "verse");
    }

    #[test]
    fn entity_media_defaults_resolve_from_nested_config() {
        let config = json!({
            "media": {
                "asr": {"model": "paraformer-realtime-v2"},
                "tts": {"model": "cosyvoice-v1", "voice": "longxiaochun"}
            }
        });

        assert_eq!(resolve_asr_model(&config, ""), "paraformer-realtime-v2");
        assert_eq!(resolve_tts_model(&config, ""), "cosyvoice-v1");
        assert_eq!(resolve_tts_voice(&config, ""), "longxiaochun");
    }

    #[test]
    fn flat_media_fields_override_nested_defaults() {
        let config = json!({
            "asr_model": "entity-asr",
            "tts_model": "entity-tts",
            "tts_voice": "entity-voice",
            "media": {
                "asr": {"model": "default-asr"},
                "tts": {"model": "default-tts", "voice": "default-voice"}
            }
        });

        assert_eq!(resolve_asr_model(&config, ""), "entity-asr");
        assert_eq!(resolve_tts_model(&config, ""), "entity-tts");
        assert_eq!(resolve_tts_voice(&config, ""), "entity-voice");
    }

    #[test]
    fn request_media_params_override_entity_defaults() {
        let config = json!({
            "asr_model": "entity-asr",
            "tts_model": "entity-tts",
            "tts_voice": "entity-voice"
        });

        assert_eq!(resolve_asr_model(&config, "request-asr"), "request-asr");
        assert_eq!(resolve_tts_model(&config, "request-tts"), "request-tts");
        assert_eq!(resolve_tts_voice(&config, "request-voice"), "request-voice");
    }
}
