//! 实时音频会话领域模型
//!
//! 定义 realtime audio WebSocket 协议的客户端/服务端事件、会话配置和验证逻辑。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 客户端事件
// ---------------------------------------------------------------------------

/// 客户端发送的实时音频事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RealtimeAudioClientEvent {
    #[serde(rename = "session.start")]
    SessionStart(RealtimeSessionStart),
    #[serde(rename = "audio.chunk")]
    AudioChunk(RealtimeAudioChunk),
    #[serde(rename = "audio.end")]
    AudioEnd,
    #[serde(rename = "session.cancel")]
    SessionCancel {
        #[serde(default)]
        reason: Option<String>,
    },
    #[serde(rename = "playback.interrupted")]
    PlaybackInterrupted {
        #[serde(default)]
        reason: Option<String>,
    },
}

/// 会话启动参数
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeSessionStart {
    #[serde(default)]
    pub session_id: Option<String>,
    pub entity_id: String,
    #[serde(default)]
    pub input_audio: Option<RealtimeAudioFormat>,
    #[serde(default)]
    pub output_audio: Option<RealtimeAudioFormat>,
}

/// 音频格式描述
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeAudioFormat {
    #[serde(default = "default_pcm_format")]
    pub format: String,
    #[serde(default = "default_input_sample_rate")]
    pub sample_rate_hz: u32,
    #[serde(default = "default_channels")]
    pub channels: u32,
    #[serde(default)]
    pub voice: Option<String>,
}

/// 音频数据块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeAudioChunk {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub audio_base64: String,
}

// ---------------------------------------------------------------------------
// 服务端事件
// ---------------------------------------------------------------------------

/// 服务端发送的实时音频事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RealtimeAudioServerEvent {
    #[serde(rename = "session.ready")]
    SessionReady(RealtimeSessionReady),
    #[serde(rename = "asr.partial")]
    AsrPartial(RealtimeAsrPartial),
    #[serde(rename = "asr.final")]
    AsrFinal(RealtimeAsrFinal),
    #[serde(rename = "intent.partial")]
    IntentPartial(RealtimeIntentPartial),
    #[serde(rename = "agent.delta")]
    AgentDelta(RealtimeAgentDelta),
    #[serde(rename = "tts.chunk")]
    TtsChunk(RealtimeTtsChunk),
    #[serde(rename = "session.interrupted")]
    SessionInterrupted(RealtimeSessionInterrupted),
    #[serde(rename = "error")]
    Error(RealtimeErrorEvent),
    #[serde(rename = "session.closed")]
    SessionClosed(RealtimeSessionClosed),
}

/// 会话就绪事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeSessionReady {
    pub session_id: String,
    pub protocol_version: u32,
    pub resolved_config: RealtimeResolvedConfig,
}

/// 解析后的实时配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeResolvedConfig {
    pub entity_id: String,
    pub asr_model: String,
    pub tts_model: String,
    pub sample_rate_hz: u32,
    pub chunk_ms: u32,
}

/// ASR 部分结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeAsrPartial {
    pub sequence: u64,
    pub text: String,
    pub confidence: f64,
}

/// ASR 最终结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeAsrFinal {
    pub sequence: u64,
    pub text: String,
    pub confidence: f64,
}

/// 意图识别部分结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeIntentPartial {
    pub intent: String,
    #[serde(default)]
    pub slots: std::collections::HashMap<String, String>,
}

/// Agent 文本增量
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeAgentDelta {
    pub sequence: u64,
    pub text: String,
}

/// TTS 音频数据块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeTtsChunk {
    pub sequence: u64,
    pub audio_base64: String,
    pub format: String,
    pub sample_rate_hz: u32,
}

/// 会话中断事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeSessionInterrupted {
    pub reason: String,
    pub cancelled_output_sequence: u64,
}

/// 错误事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeErrorEvent {
    pub code: RealtimeErrorCode,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

/// 会话关闭事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeSessionClosed {
    pub reason: RealtimeSessionCloseReason,
}

// ---------------------------------------------------------------------------
// 枚举类型
// ---------------------------------------------------------------------------

/// 实时音频错误码
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RealtimeErrorCode {
    BadRequest,
    Unauthorized,
    Forbidden,
    EntityNotFound,
    RealtimeDisabled,
    RealtimeProviderUnavailable,
    UnsupportedAudioFormat,
    UnsupportedFrameType,
    FrameTooLarge,
    SessionTimeout,
    Backpressure,
    ProviderError,
    InternalError,
}

/// 会话关闭原因
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeSessionCloseReason {
    Normal,
    Error,
    Timeout,
    ClientDisconnected,
    ServerShutdown,
}

// ---------------------------------------------------------------------------
// 验证错误
// ---------------------------------------------------------------------------

/// 验证错误类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeAudioValidationError {
    EmptyAudioData,
    FrameTooLarge { size: usize, max: usize },
    InvalidSessionId,
    MissingEntityId,
    InvalidSampleRate,
}

impl std::fmt::Display for RealtimeAudioValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAudioData => write!(f, "audio data is empty"),
            Self::FrameTooLarge { size, max } => {
                write!(f, "frame size {size} exceeds maximum {max}")
            }
            Self::InvalidSessionId => write!(f, "invalid session id"),
            Self::MissingEntityId => write!(f, "missing entity_id"),
            Self::InvalidSampleRate => write!(f, "invalid sample rate"),
        }
    }
}

impl std::error::Error for RealtimeAudioValidationError {}

// ---------------------------------------------------------------------------
// 验证辅助函数
// ---------------------------------------------------------------------------

/// 验证音频数据块
pub fn validate_audio_chunk(
    chunk: &RealtimeAudioChunk,
    max_frame_bytes: usize,
) -> Result<(), RealtimeAudioValidationError> {
    if chunk.audio_base64.is_empty() {
        return Err(RealtimeAudioValidationError::EmptyAudioData);
    }

    // base64 解码后大小约为 3/4 * len
    let estimated_bytes = (chunk.audio_base64.len() * 3) / 4;
    if estimated_bytes > max_frame_bytes {
        return Err(RealtimeAudioValidationError::FrameTooLarge {
            size: estimated_bytes,
            max: max_frame_bytes,
        });
    }

    Ok(())
}

/// 验证会话启动参数
pub fn validate_session_start(start: &RealtimeSessionStart) -> Result<(), RealtimeAudioValidationError> {
    if start.entity_id.is_empty() {
        return Err(RealtimeAudioValidationError::MissingEntityId);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 默认值函数
// ---------------------------------------------------------------------------

fn default_pcm_format() -> String {
    "pcm_s16le".to_string()
}

fn default_input_sample_rate() -> u32 {
    16000
}

fn default_channels() -> u32 {
    1
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_start_event_deserializes() {
        let json = r#"{
            "type": "session.start",
            "session_id": "test-session-1",
            "entity_id": "tower-audio-agent",
            "input_audio": {
                "format": "pcm_s16le",
                "sample_rate_hz": 16000,
                "channels": 1
            },
            "output_audio": {
                "format": "pcm_s16le",
                "sample_rate_hz": 24000,
                "voice": "default"
            }
        }"#;

        let event: RealtimeAudioClientEvent = serde_json::from_str(json).unwrap();
        match event {
            RealtimeAudioClientEvent::SessionStart(start) => {
                assert_eq!(start.entity_id, "tower-audio-agent");
                assert_eq!(start.session_id, Some("test-session-1".to_string()));
                let input = start.input_audio.unwrap();
                assert_eq!(input.format, "pcm_s16le");
                assert_eq!(input.sample_rate_hz, 16000);
                assert_eq!(input.channels, 1);
            }
            _ => panic!("expected SessionStart"),
        }
    }

    #[test]
    fn audio_chunk_rejects_empty_base64() {
        let json = r#"{
            "type": "audio.chunk",
            "sequence": 1,
            "timestamp_ms": 100,
            "audio_base64": ""
        }"#;

        let event: RealtimeAudioClientEvent = serde_json::from_str(json).unwrap();
        match event {
            RealtimeAudioClientEvent::AudioChunk(chunk) => {
                let result = validate_audio_chunk(&chunk, 65536);
                assert_eq!(result, Err(RealtimeAudioValidationError::EmptyAudioData));
            }
            _ => panic!("expected AudioChunk"),
        }
    }

    #[test]
    fn error_server_event_serializes_with_retryable() {
        let event = RealtimeAudioServerEvent::Error(RealtimeErrorEvent {
            code: RealtimeErrorCode::FrameTooLarge,
            message: "audio frame exceeds max frame size".to_string(),
            retryable: false,
        });

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "error");
        assert_eq!(parsed["code"], "FRAME_TOO_LARGE");
        assert_eq!(parsed["message"], "audio frame exceeds max frame size");
        assert_eq!(parsed["retryable"], false);
    }

    #[test]
    fn unknown_event_type_returns_serde_error() {
        let json = r#"{"type": "unknown.event"}"#;
        let result: Result<RealtimeAudioClientEvent, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn audio_chunk_event_round_trips() {
        let chunk = RealtimeAudioClientEvent::AudioChunk(RealtimeAudioChunk {
            sequence: 42,
            timestamp_ms: 1234,
            audio_base64: "SGVsbG8=".to_string(),
        });

        let json = serde_json::to_string(&chunk).unwrap();
        let parsed: RealtimeAudioClientEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(chunk, parsed);
    }

    #[test]
    fn session_ready_event_serializes_correctly() {
        let event = RealtimeAudioServerEvent::SessionReady(RealtimeSessionReady {
            session_id: "sess-123".to_string(),
            protocol_version: 1,
            resolved_config: RealtimeResolvedConfig {
                entity_id: "tower-audio-agent".to_string(),
                asr_model: "fake-streaming-asr".to_string(),
                tts_model: "fake-streaming-tts".to_string(),
                sample_rate_hz: 16000,
                chunk_ms: 40,
            },
        });

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "session.ready");
        assert_eq!(parsed["session_id"], "sess-123");
        assert_eq!(parsed["protocol_version"], 1);
        assert_eq!(parsed["resolved_config"]["entity_id"], "tower-audio-agent");
    }

    #[test]
    fn oversized_frame_validation_fails() {
        // A base64 string of 1000 chars encodes ~750 bytes
        let large_base64 = "A".repeat(1000);
        let chunk = RealtimeAudioChunk {
            sequence: 1,
            timestamp_ms: 100,
            audio_base64: large_base64,
        };

        let result = validate_audio_chunk(&chunk, 500);
        assert!(matches!(
            result,
            Err(RealtimeAudioValidationError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn missing_entity_id_validation_fails() {
        let start = RealtimeSessionStart {
            session_id: None,
            entity_id: "".to_string(),
            input_audio: None,
            output_audio: None,
        };

        let result = validate_session_start(&start);
        assert_eq!(result, Err(RealtimeAudioValidationError::MissingEntityId));
    }

    #[test]
    fn audio_end_event_deserializes() {
        let json = r#"{"type": "audio.end"}"#;
        let event: RealtimeAudioClientEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event, RealtimeAudioClientEvent::AudioEnd);
    }

    #[test]
    fn session_cancel_with_reason_deserializes() {
        let json = r#"{"type": "session.cancel", "reason": "user_cancelled"}"#;
        let event: RealtimeAudioClientEvent = serde_json::from_str(json).unwrap();
        match event {
            RealtimeAudioClientEvent::SessionCancel { reason } => {
                assert_eq!(reason, Some("user_cancelled".to_string()));
            }
            _ => panic!("expected SessionCancel"),
        }
    }

    #[test]
    fn playback_interrupted_deserializes() {
        let json = r#"{"type": "playback.interrupted", "reason": "barge_in"}"#;
        let event: RealtimeAudioClientEvent = serde_json::from_str(json).unwrap();
        match event {
            RealtimeAudioClientEvent::PlaybackInterrupted { reason } => {
                assert_eq!(reason, Some("barge_in".to_string()));
            }
            _ => panic!("expected PlaybackInterrupted"),
        }
    }
}
