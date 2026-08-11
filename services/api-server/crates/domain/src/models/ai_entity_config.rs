//! AI 实体配置模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiEntityConfigRecord {
    pub id: String,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 实时音频配置（嵌套在 entity config 的 `media.realtime` 下）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiRealtimeAudioConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub asr_streaming_model: Option<String>,
    #[serde(default)]
    pub tts_streaming_model: Option<String>,
    #[serde(default = "default_input_sample_rate_hz")]
    pub input_sample_rate_hz: u32,
    #[serde(default = "default_output_sample_rate_hz")]
    pub output_sample_rate_hz: u32,
    #[serde(default = "default_chunk_ms")]
    pub chunk_ms: u32,
    #[serde(default = "default_latency_budget_ms")]
    pub latency_budget_ms: u32,
    #[serde(default = "default_true")]
    pub vad_enabled: bool,
    #[serde(default = "default_true")]
    pub barge_in_enabled: bool,
    #[serde(default = "default_max_session_seconds")]
    pub max_session_seconds: u32,
    #[serde(default = "default_max_frame_bytes")]
    pub max_frame_bytes: usize,
}

impl Default for AiRealtimeAudioConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: None,
            asr_streaming_model: None,
            tts_streaming_model: None,
            input_sample_rate_hz: default_input_sample_rate_hz(),
            output_sample_rate_hz: default_output_sample_rate_hz(),
            chunk_ms: default_chunk_ms(),
            latency_budget_ms: default_latency_budget_ms(),
            vad_enabled: true,
            barge_in_enabled: true,
            max_session_seconds: default_max_session_seconds(),
            max_frame_bytes: default_max_frame_bytes(),
        }
    }
}

fn default_input_sample_rate_hz() -> u32 {
    16000
}
fn default_output_sample_rate_hz() -> u32 {
    24000
}
fn default_chunk_ms() -> u32 {
    40
}
fn default_latency_budget_ms() -> u32 {
    800
}
fn default_true() -> bool {
    true
}
fn default_max_session_seconds() -> u32 {
    300
}
fn default_max_frame_bytes() -> usize {
    65536
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_config_without_realtime_still_deserializes() {
        let json = r#"{
            "api_key": "sk-test",
            "base_url": "https://api.openai.com/v1",
            "media": {
                "asr": {"model": "whisper-1"},
                "tts": {"model": "tts-1"}
            }
        }"#;

        let config: serde_json::Value = serde_json::from_str(json).unwrap();
        let media = config.get("media").unwrap();
        let realtime: AiRealtimeAudioConfig =
            serde_json::from_value(media.get("realtime").cloned().unwrap_or(serde_json::json!({}))).unwrap();

        assert!(!realtime.enabled);
    }

    #[test]
    fn missing_realtime_enabled_defaults_to_false() {
        let config = AiRealtimeAudioConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn realtime_config_with_defaults_resolves_correctly() {
        let json = r#"{
            "enabled": true,
            "provider": "fake",
            "asr_streaming_model": "fake-streaming-asr",
            "tts_streaming_model": "fake-streaming-tts"
        }"#;

        let config: AiRealtimeAudioConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.provider.as_deref(), Some("fake"));
        assert_eq!(config.input_sample_rate_hz, 16000);
        assert_eq!(config.output_sample_rate_hz, 24000);
        assert_eq!(config.chunk_ms, 40);
        assert_eq!(config.max_frame_bytes, 65536);
        assert!(config.barge_in_enabled);
    }

    #[test]
    fn realtime_config_serializes_and_deserializes() {
        let config = AiRealtimeAudioConfig {
            enabled: true,
            provider: Some("fake".to_string()),
            asr_streaming_model: Some("fake-asr".to_string()),
            tts_streaming_model: Some("fake-tts".to_string()),
            input_sample_rate_hz: 16000,
            output_sample_rate_hz: 24000,
            chunk_ms: 20,
            latency_budget_ms: 500,
            vad_enabled: true,
            barge_in_enabled: false,
            max_session_seconds: 600,
            max_frame_bytes: 32768,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: AiRealtimeAudioConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }
}
