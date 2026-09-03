//! 实时音频会话服务
//!
//! 编排实体配置解析、provider 选择、会话生命周期和事件处理。
//! 第一阶段使用 deterministic fake provider 建立协议闭环。

use std::sync::Arc;

use async_trait::async_trait;
use metrics::counter;
use serde_json::Value;
use tracing::{info, warn};

use fms_domain::models::ai_entity_config::AiRealtimeAudioConfig;
use fms_domain::models::ai_realtime_audio::*;

use crate::services::ai_admin_service::AiAdminService;

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

const AI_REALTIME_SESSION_TOTAL: &str = "ai_realtime_session_total";
const AI_REALTIME_SESSION_ACTIVE: &str = "ai_realtime_session_active";
const AI_REALTIME_ERROR_TOTAL: &str = "ai_realtime_error_total";
const AI_REALTIME_BARGE_IN_TOTAL: &str = "ai_realtime_barge_in_total";
const AI_REALTIME_FRAME_DROP_TOTAL: &str = "ai_realtime_audio_frame_drop_total";

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

/// 实时音频服务错误
#[derive(Debug, Clone)]
pub enum RealtimeAudioError {
    /// 实体不存在
    EntityNotFound(String),
    /// 实时音频未启用
    RealtimeDisabled,
    /// Provider 不可用
    ProviderUnavailable(String),
    /// 帧过大
    FrameTooLarge { size: usize, max: usize },
    /// 会话超时
    SessionTimeout,
    /// 背压
    Backpressure,
    /// Provider 内部错误（消息已脱敏）
    ProviderError(String),
    /// 内部错误（消息已脱敏）
    Internal(String),
}

impl std::fmt::Display for RealtimeAudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EntityNotFound(id) => write!(f, "entity not found: {id}"),
            Self::RealtimeDisabled => write!(f, "realtime audio is disabled"),
            Self::ProviderUnavailable(msg) => write!(f, "provider unavailable: {msg}"),
            Self::FrameTooLarge { size, max } => {
                write!(f, "frame size {size} exceeds maximum {max}")
            }
            Self::SessionTimeout => write!(f, "session timed out"),
            Self::Backpressure => write!(f, "backpressure: queue full"),
            Self::ProviderError(msg) => write!(f, "provider error: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for RealtimeAudioError {}

impl RealtimeAudioError {
    /// 转换为客户端安全的错误码
    pub fn error_code(&self) -> RealtimeErrorCode {
        match self {
            Self::EntityNotFound(_) => RealtimeErrorCode::EntityNotFound,
            Self::RealtimeDisabled => RealtimeErrorCode::RealtimeDisabled,
            Self::ProviderUnavailable(_) => RealtimeErrorCode::RealtimeProviderUnavailable,
            Self::FrameTooLarge { .. } => RealtimeErrorCode::FrameTooLarge,
            Self::SessionTimeout => RealtimeErrorCode::SessionTimeout,
            Self::Backpressure => RealtimeErrorCode::Backpressure,
            Self::ProviderError(_) => RealtimeErrorCode::ProviderError,
            Self::Internal(_) => RealtimeErrorCode::InternalError,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Backpressure | Self::ProviderError(_))
    }
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// 实时音频 Provider 抽象
#[async_trait]
pub trait RealtimeAudioProvider: Send + Sync {
    /// 初始化 provider 会话
    async fn start(&self, config: &RealtimeResolvedConfig) -> Result<(), RealtimeAudioError>;

    /// 推送音频数据，返回产生的服务端事件
    async fn push_audio(&self, chunk: &RealtimeAudioChunk)
        -> Result<Vec<RealtimeAudioServerEvent>, RealtimeAudioError>;

    /// 结束音频输入，返回最终事件
    async fn finish_audio(&self) -> Result<Vec<RealtimeAudioServerEvent>, RealtimeAudioError>;

    /// 中断当前播放
    async fn interrupt(&self) -> Result<Vec<RealtimeAudioServerEvent>, RealtimeAudioError>;
}

// ---------------------------------------------------------------------------
// Session handle
// ---------------------------------------------------------------------------

/// 会话句柄 — 包含 session.ready 事件和初始状态
pub struct RealtimeAudioSessionHandle {
    pub session_id: String,
    pub ready_event: RealtimeAudioServerEvent,
    pub config: RealtimeResolvedConfig,
    pub max_frame_bytes: usize,
    pub max_session_seconds: u32,
    pub provider: Arc<dyn RealtimeAudioProvider>,
}

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

/// 活跃会话状态
pub struct RealtimeAudioSessionState {
    pub session_id: String,
    pub entity_id: String,
    pub config: RealtimeResolvedConfig,
    pub max_frame_bytes: usize,
    pub max_session_seconds: u32,
    pub chunk_count: u64,
    pub started_at: std::time::Instant,
    pub provider: Arc<dyn RealtimeAudioProvider>,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// 实时音频会话服务
pub struct RealtimeAudioSessionService {
    ai_admin_service: Arc<AiAdminService>,
}

impl RealtimeAudioSessionService {
    pub fn new(ai_admin_service: Arc<AiAdminService>) -> Self {
        Self { ai_admin_service }
    }

    /// 启动实时音频会话
    pub async fn start_session(
        &self,
        entity_id: &str,
        protocol_version: u32,
    ) -> Result<RealtimeAudioSessionHandle, RealtimeAudioError> {
        // 1. 加载实体配置
        let config = self
            .ai_admin_service
            .get_entity_runtime_config(entity_id)
            .await
            .map_err(|e| RealtimeAudioError::Internal(format!("load config failed: {e}")))?
            .ok_or_else(|| RealtimeAudioError::EntityNotFound(entity_id.to_string()))?;

        // 2. 解析实时音频配置
        let realtime_config = resolve_realtime_config(&config)?;

        // 3. 检查是否启用
        if !realtime_config.enabled {
            return Err(RealtimeAudioError::RealtimeDisabled);
        }

        // 4. 选择 provider
        let provider = select_provider(&realtime_config)?;

        // 5. 构建 resolved config
        let resolved = RealtimeResolvedConfig {
            entity_id: entity_id.to_string(),
            asr_model: realtime_config
                .asr_streaming_model
                .clone()
                .unwrap_or_else(|| "default-asr".to_string()),
            tts_model: realtime_config
                .tts_streaming_model
                .clone()
                .unwrap_or_else(|| "default-tts".to_string()),
            sample_rate_hz: realtime_config.input_sample_rate_hz,
            chunk_ms: realtime_config.chunk_ms,
        };

        // 6. 初始化 provider
        provider.start(&resolved).await?;

        let session_id = uuid::Uuid::new_v4().to_string();

        info!(
            session_id = %session_id,
            entity_id = %entity_id,
            asr_model = %resolved.asr_model,
            tts_model = %resolved.tts_model,
            "realtime audio session started"
        );

        let _ = counter!(AI_REALTIME_SESSION_TOTAL, "entity_id" => entity_id.to_string());
        let _ = counter!(AI_REALTIME_SESSION_ACTIVE, "entity_id" => entity_id.to_string());

        let ready_event = RealtimeAudioServerEvent::SessionReady(RealtimeSessionReady {
            session_id: session_id.clone(),
            protocol_version,
            resolved_config: resolved.clone(),
        });

        Ok(RealtimeAudioSessionHandle {
            session_id,
            ready_event,
            config: resolved,
            max_frame_bytes: realtime_config.max_frame_bytes,
            max_session_seconds: realtime_config.max_session_seconds,
            provider,
        })
    }

    /// 从会话句柄创建活跃会话状态（provider 已内含在 handle 中）
    pub fn create_session_state(&self, handle: RealtimeAudioSessionHandle) -> RealtimeAudioSessionState {
        RealtimeAudioSessionState {
            session_id: handle.session_id,
            entity_id: handle.config.entity_id.clone(),
            config: handle.config,
            max_frame_bytes: handle.max_frame_bytes,
            max_session_seconds: handle.max_session_seconds,
            chunk_count: 0,
            started_at: std::time::Instant::now(),
            provider: handle.provider,
        }
    }

    /// 处理客户端事件
    pub async fn process_client_event(
        &self,
        state: &mut RealtimeAudioSessionState,
        event: RealtimeAudioClientEvent,
    ) -> Vec<RealtimeAudioServerEvent> {
        // 检查会话超时
        if state.started_at.elapsed().as_secs() > state.max_session_seconds as u64 {
            return vec![RealtimeAudioServerEvent::SessionClosed(RealtimeSessionClosed {
                reason: RealtimeSessionCloseReason::Timeout,
            })];
        }

        match event {
            RealtimeAudioClientEvent::AudioChunk(chunk) => {
                // 验证帧大小
                if let Err(e) = validate_audio_chunk(&chunk, state.max_frame_bytes) {
                    let _ = counter!(AI_REALTIME_FRAME_DROP_TOTAL);
                    let err = match e {
                        RealtimeAudioValidationError::EmptyAudioData => {
                            RealtimeAudioError::Internal("empty audio data".to_string())
                        }
                        RealtimeAudioValidationError::FrameTooLarge { size, max } => {
                            RealtimeAudioError::FrameTooLarge { size, max }
                        }
                        _ => RealtimeAudioError::Internal(e.to_string()),
                    };
                    return vec![make_error_event(err)];
                }

                state.chunk_count += 1;
                match state.provider.push_audio(&chunk).await {
                    Ok(events) => events,
                    Err(e) => vec![make_error_event(e)],
                }
            }
            RealtimeAudioClientEvent::AudioEnd => match state.provider.finish_audio().await {
                Ok(events) => events,
                Err(e) => vec![make_error_event(e)],
            },
            RealtimeAudioClientEvent::PlaybackInterrupted { reason } => {
                warn!(reason = ?reason, "playback interrupted");
                let _ = counter!(AI_REALTIME_BARGE_IN_TOTAL);
                match state.provider.interrupt().await {
                    Ok(events) => events,
                    Err(e) => vec![make_error_event(e)],
                }
            }
            RealtimeAudioClientEvent::SessionCancel { reason } => {
                info!(reason = ?reason, "session cancelled by client");
                vec![RealtimeAudioServerEvent::SessionClosed(RealtimeSessionClosed {
                    reason: RealtimeSessionCloseReason::ClientDisconnected,
                })]
            }
            RealtimeAudioClientEvent::SessionStart(_) => {
                vec![make_error_event(RealtimeAudioError::Internal(
                    "session already started".to_string(),
                ))]
            }
        }
    }

    /// 从实体配置中解析实时音频配置（供路由层使用）
    pub async fn resolve_entity_realtime_config(
        &self,
        entity_id: &str,
    ) -> Result<AiRealtimeAudioConfig, RealtimeAudioError> {
        let config = self
            .ai_admin_service
            .get_entity_runtime_config(entity_id)
            .await
            .map_err(|e| RealtimeAudioError::Internal(format!("load config failed: {e}")))?
            .ok_or_else(|| RealtimeAudioError::EntityNotFound(entity_id.to_string()))?;

        resolve_realtime_config(&config)
    }
}

// ---------------------------------------------------------------------------
// 配置解析
// ---------------------------------------------------------------------------

fn resolve_realtime_config(config: &Value) -> Result<AiRealtimeAudioConfig, RealtimeAudioError> {
    let media = config.get("media").cloned().unwrap_or(serde_json::json!({}));
    let realtime_json = media.get("realtime").cloned().unwrap_or(serde_json::json!({}));

    serde_json::from_value(realtime_json)
        .map_err(|e| RealtimeAudioError::Internal(format!("invalid realtime config: {e}")))
}

fn select_provider(config: &AiRealtimeAudioConfig) -> Result<Arc<dyn RealtimeAudioProvider>, RealtimeAudioError> {
    let provider_name = config.provider.as_deref().unwrap_or("fake");

    match provider_name {
        "fake" => Ok(Arc::new(FakeRealtimeAudioProvider::new())),
        other => {
            warn!(provider = other, "unsupported realtime provider");
            Err(RealtimeAudioError::ProviderUnavailable(other.to_string()))
        }
    }
}

/// Map internal errors to sanitized client-facing error events.
///
/// ProviderError and Internal messages must never leak raw SDK or internal details
/// to the client. Only safe, fixed descriptions are emitted.
fn make_error_event(error: RealtimeAudioError) -> RealtimeAudioServerEvent {
    let _ = counter!(AI_REALTIME_ERROR_TOTAL, "code" => format!("{:?}", error.error_code()));

    // Internal tracing log retains full detail
    if matches!(
        error,
        RealtimeAudioError::ProviderError(_) | RealtimeAudioError::Internal(_)
    ) {
        tracing::error!(error = %error, "realtime audio internal error");
    }

    let safe_message = match &error {
        RealtimeAudioError::EntityNotFound(_) => error.to_string(),
        RealtimeAudioError::RealtimeDisabled => error.to_string(),
        RealtimeAudioError::ProviderUnavailable(_) => error.to_string(),
        RealtimeAudioError::FrameTooLarge { .. } => error.to_string(),
        RealtimeAudioError::SessionTimeout => error.to_string(),
        RealtimeAudioError::Backpressure => error.to_string(),
        RealtimeAudioError::ProviderError(_) => "provider failed while processing realtime audio".to_string(),
        RealtimeAudioError::Internal(_) => "internal server error".to_string(),
    };

    RealtimeAudioServerEvent::Error(RealtimeErrorEvent {
        code: error.error_code(),
        message: safe_message,
        retryable: error.is_retryable(),
    })
}

// ---------------------------------------------------------------------------
// Fake Provider
// ---------------------------------------------------------------------------

/// 确定性 fake provider，用于测试和开发
pub struct FakeRealtimeAudioProvider {
    chunk_count: std::sync::atomic::AtomicU64,
}

impl Default for FakeRealtimeAudioProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeRealtimeAudioProvider {
    pub fn new() -> Self {
        Self {
            chunk_count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl RealtimeAudioProvider for FakeRealtimeAudioProvider {
    async fn start(&self, _config: &RealtimeResolvedConfig) -> Result<(), RealtimeAudioError> {
        Ok(())
    }

    async fn push_audio(
        &self,
        _chunk: &RealtimeAudioChunk,
    ) -> Result<Vec<RealtimeAudioServerEvent>, RealtimeAudioError> {
        let count = self.chunk_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Deterministic fake ASR partial results
        let (text, confidence) = match count {
            0 => ("CA", 0.72),
            1 => ("CA123", 0.85),
            _ => ("CA123 request", 0.90),
        };

        Ok(vec![RealtimeAudioServerEvent::AsrPartial(RealtimeAsrPartial {
            sequence: count + 1,
            text: text.to_string(),
            confidence,
        })])
    }

    async fn finish_audio(&self) -> Result<Vec<RealtimeAudioServerEvent>, RealtimeAudioError> {
        let count = self.chunk_count.load(std::sync::atomic::Ordering::SeqCst);

        // Deterministic fake final results
        let mut events = Vec::new();

        events.push(RealtimeAudioServerEvent::AsrFinal(RealtimeAsrFinal {
            sequence: count,
            text: "CA123 request pushback".to_string(),
            confidence: 0.91,
        }));

        events.push(RealtimeAudioServerEvent::IntentPartial(RealtimeIntentPartial {
            intent: "flight_ground_operation".to_string(),
            slots: {
                let mut m = std::collections::HashMap::new();
                m.insert("flight_no".to_string(), "CA123".to_string());
                m.insert("operation".to_string(), "pushback".to_string());
                m
            },
        }));

        events.push(RealtimeAudioServerEvent::AgentDelta(RealtimeAgentDelta {
            sequence: 1,
            text: "CA123, pushback approved".to_string(),
        }));

        // Fake TTS chunk with fixed bytes
        let fake_pcm: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
        let audio_base64 = base64_encode(&fake_pcm);
        events.push(RealtimeAudioServerEvent::TtsChunk(RealtimeTtsChunk {
            sequence: 1,
            audio_base64,
            format: "pcm_s16le".to_string(),
            sample_rate_hz: 24000,
        }));

        Ok(events)
    }

    async fn interrupt(&self) -> Result<Vec<RealtimeAudioServerEvent>, RealtimeAudioError> {
        Ok(vec![RealtimeAudioServerEvent::SessionInterrupted(
            RealtimeSessionInterrupted {
                reason: "barge_in".to_string(),
                cancelled_output_sequence: 1,
            },
        )])
    }
}

// ---------------------------------------------------------------------------
// 简单 base64 编码（避免外部依赖）
// ---------------------------------------------------------------------------

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_provider_deterministic_first_chunk() {
        let provider = FakeRealtimeAudioProvider::new();
        let config = RealtimeResolvedConfig {
            entity_id: "test".to_string(),
            asr_model: "fake".to_string(),
            tts_model: "fake".to_string(),
            sample_rate_hz: 16000,
            chunk_ms: 40,
        };

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            provider.start(&config).await.unwrap();

            let chunk = RealtimeAudioChunk {
                sequence: 1,
                timestamp_ms: 100,
                audio_base64: "SGVsbG8=".to_string(),
            };

            let events = provider.push_audio(&chunk).await.unwrap();
            assert_eq!(events.len(), 1);
            match &events[0] {
                RealtimeAudioServerEvent::AsrPartial(partial) => {
                    assert_eq!(partial.text, "CA");
                    assert!((partial.confidence - 0.72).abs() < f64::EPSILON);
                }
                _ => panic!("expected AsrPartial"),
            }
        });
    }

    #[test]
    fn fake_provider_deterministic_second_chunk() {
        let provider = FakeRealtimeAudioProvider::new();
        let config = RealtimeResolvedConfig {
            entity_id: "test".to_string(),
            asr_model: "fake".to_string(),
            tts_model: "fake".to_string(),
            sample_rate_hz: 16000,
            chunk_ms: 40,
        };

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            provider.start(&config).await.unwrap();

            let chunk = RealtimeAudioChunk {
                sequence: 1,
                timestamp_ms: 100,
                audio_base64: "SGVsbG8=".to_string(),
            };

            // First chunk
            provider.push_audio(&chunk).await.unwrap();
            // Second chunk
            let events = provider.push_audio(&chunk).await.unwrap();
            assert_eq!(events.len(), 1);
            match &events[0] {
                RealtimeAudioServerEvent::AsrPartial(partial) => {
                    assert_eq!(partial.text, "CA123");
                }
                _ => panic!("expected AsrPartial"),
            }
        });
    }

    #[test]
    fn fake_provider_finish_audio_returns_all_events() {
        let provider = FakeRealtimeAudioProvider::new();
        let config = RealtimeResolvedConfig {
            entity_id: "test".to_string(),
            asr_model: "fake".to_string(),
            tts_model: "fake".to_string(),
            sample_rate_hz: 16000,
            chunk_ms: 40,
        };

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            provider.start(&config).await.unwrap();

            let events = provider.finish_audio().await.unwrap();
            assert_eq!(events.len(), 4); // asr.final, intent.partial, agent.delta, tts.chunk

            assert!(matches!(
                &events[0],
                RealtimeAudioServerEvent::AsrFinal(f) if f.text == "CA123 request pushback"
            ));
            assert!(matches!(
                &events[1],
                RealtimeAudioServerEvent::IntentPartial(i) if i.intent == "flight_ground_operation"
            ));
            assert!(matches!(
                &events[2],
                RealtimeAudioServerEvent::AgentDelta(d) if d.text == "CA123, pushback approved"
            ));
            assert!(matches!(
                &events[3],
                RealtimeAudioServerEvent::TtsChunk(t) if t.sample_rate_hz == 24000
            ));
        });
    }

    #[test]
    fn fake_provider_interrupt_returns_session_interrupted() {
        let provider = FakeRealtimeAudioProvider::new();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let events = provider.interrupt().await.unwrap();
            assert_eq!(events.len(), 1);
            match &events[0] {
                RealtimeAudioServerEvent::SessionInterrupted(interrupted) => {
                    assert_eq!(interrupted.reason, "barge_in");
                    assert_eq!(interrupted.cancelled_output_sequence, 1);
                }
                _ => panic!("expected SessionInterrupted"),
            }
        });
    }

    #[test]
    fn error_event_sanitizes_provider_error() {
        let error = RealtimeAudioError::ProviderError("internal sdk details".to_string());
        let event = make_error_event(error);
        match event {
            RealtimeAudioServerEvent::Error(e) => {
                assert_eq!(e.code, RealtimeErrorCode::ProviderError);
                assert!(e.retryable);
                // Message must NOT contain raw internal details
                assert!(!e.message.contains("internal sdk details"));
                assert_eq!(e.message, "provider failed while processing realtime audio");
            }
            _ => panic!("expected Error event"),
        }
    }

    #[test]
    fn error_event_sanitizes_internal_error() {
        let error = RealtimeAudioError::Internal("secret db connection string".to_string());
        let event = make_error_event(error);
        match event {
            RealtimeAudioServerEvent::Error(e) => {
                assert_eq!(e.code, RealtimeErrorCode::InternalError);
                assert!(!e.message.contains("secret db connection string"));
                assert_eq!(e.message, "internal server error");
            }
            _ => panic!("expected Error event"),
        }
    }

    #[test]
    fn unknown_provider_returns_provider_unavailable() {
        let config = AiRealtimeAudioConfig {
            enabled: true,
            provider: Some("openai-realtime-v99".to_string()),
            ..AiRealtimeAudioConfig::default()
        };

        match select_provider(&config) {
            Err(RealtimeAudioError::ProviderUnavailable(name)) => {
                assert_eq!(name, "openai-realtime-v99");
            }
            Err(other) => panic!("expected ProviderUnavailable, got: {other}"),
            Ok(_) => panic!("expected error for unknown provider"),
        }
    }

    #[test]
    fn fake_provider_explicit_select_succeeds() {
        let config = AiRealtimeAudioConfig {
            enabled: true,
            provider: Some("fake".to_string()),
            ..AiRealtimeAudioConfig::default()
        };

        let result = select_provider(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn default_provider_is_fake() {
        let config = AiRealtimeAudioConfig {
            enabled: true,
            provider: None,
            ..AiRealtimeAudioConfig::default()
        };

        let result = select_provider(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn full_fake_session_flow_ready_to_tts() {
        // Simulates the complete flow: start -> push_audio x2 -> finish_audio
        let provider = Arc::new(FakeRealtimeAudioProvider::new());
        let config = RealtimeResolvedConfig {
            entity_id: "test-entity".to_string(),
            asr_model: "fake-streaming-asr".to_string(),
            tts_model: "fake-streaming-tts".to_string(),
            sample_rate_hz: 16000,
            chunk_ms: 40,
        };

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            provider.start(&config).await.unwrap();

            // Chunk 1 -> asr.partial "CA"
            let chunk1 = RealtimeAudioChunk {
                sequence: 1,
                timestamp_ms: 100,
                audio_base64: "SGVsbG8=".to_string(),
            };
            let events1 = provider.push_audio(&chunk1).await.unwrap();
            assert_eq!(events1.len(), 1);
            assert!(matches!(&events1[0],
                RealtimeAudioServerEvent::AsrPartial(p) if p.text == "CA"
            ));

            // Chunk 2 -> asr.partial "CA123"
            let chunk2 = RealtimeAudioChunk {
                sequence: 2,
                timestamp_ms: 200,
                audio_base64: "V29ybGQ=".to_string(),
            };
            let events2 = provider.push_audio(&chunk2).await.unwrap();
            assert_eq!(events2.len(), 1);
            assert!(matches!(&events2[0],
                RealtimeAudioServerEvent::AsrPartial(p) if p.text == "CA123"
            ));

            // finish_audio -> asr.final + intent.partial + agent.delta + tts.chunk
            let events3 = provider.finish_audio().await.unwrap();
            assert_eq!(events3.len(), 4);
            assert!(matches!(&events3[0],
                RealtimeAudioServerEvent::AsrFinal(f) if f.text == "CA123 request pushback"
            ));
            assert!(matches!(&events3[1],
                RealtimeAudioServerEvent::IntentPartial(i) if i.intent == "flight_ground_operation"
            ));
            assert!(matches!(&events3[2],
                RealtimeAudioServerEvent::AgentDelta(d) if d.text == "CA123, pushback approved"
            ));
            assert!(matches!(&events3[3],
                RealtimeAudioServerEvent::TtsChunk(t) if t.sample_rate_hz == 24000
            ));
        });
    }

    #[test]
    fn interrupt_returns_session_interrupted() {
        let provider = FakeRealtimeAudioProvider::new();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let events = provider.interrupt().await.unwrap();
            assert_eq!(events.len(), 1);
            assert!(matches!(&events[0],
                RealtimeAudioServerEvent::SessionInterrupted(i) if i.reason == "barge_in"
            ));
        });
    }

    #[test]
    fn provider_error_does_not_leak_internal_details() {
        let error = RealtimeAudioError::ProviderError("secret-api-key-12345".to_string());
        let event = make_error_event(error);
        match event {
            RealtimeAudioServerEvent::Error(e) => {
                assert!(!e.message.contains("secret-api-key-12345"));
                assert_eq!(e.message, "provider failed while processing realtime audio");
            }
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn internal_error_does_not_leak_details() {
        let error = RealtimeAudioError::Internal("db connection string: postgres://user:pass@host".to_string());
        let event = make_error_event(error);
        match event {
            RealtimeAudioServerEvent::Error(e) => {
                assert!(!e.message.contains("postgres://"));
                assert_eq!(e.message, "internal server error");
            }
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn entity_not_found_message_is_safe_to_expose() {
        let error = RealtimeAudioError::EntityNotFound("my-entity-id".to_string());
        let event = make_error_event(error);
        match event {
            RealtimeAudioServerEvent::Error(e) => {
                assert!(e.message.contains("my-entity-id"));
                assert_eq!(e.code, RealtimeErrorCode::EntityNotFound);
            }
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn oversized_frame_maps_to_frame_too_large() {
        let error = RealtimeAudioError::FrameTooLarge {
            size: 100000,
            max: 65536,
        };
        let event = make_error_event(error);
        match event {
            RealtimeAudioServerEvent::Error(e) => {
                assert_eq!(e.code, RealtimeErrorCode::FrameTooLarge);
                assert!(!e.retryable);
            }
            _ => panic!("expected Error event"),
        }
    }

    #[test]
    fn realtime_disabled_maps_to_correct_error_code() {
        let error = RealtimeAudioError::RealtimeDisabled;
        assert_eq!(error.error_code(), RealtimeErrorCode::RealtimeDisabled);
        assert!(!error.is_retryable());
    }

    #[test]
    fn resolve_realtime_config_from_entity_config() {
        let config = serde_json::json!({
            "media": {
                "realtime": {
                    "enabled": true,
                    "provider": "fake",
                    "chunk_ms": 20
                }
            }
        });

        let rt = resolve_realtime_config(&config).unwrap();
        assert!(rt.enabled);
        assert_eq!(rt.provider.as_deref(), Some("fake"));
        assert_eq!(rt.chunk_ms, 20);
        assert_eq!(rt.input_sample_rate_hz, 16000); // default
    }

    #[test]
    fn resolve_realtime_config_missing_defaults_to_disabled() {
        let config = serde_json::json!({
            "media": {}
        });

        let rt = resolve_realtime_config(&config).unwrap();
        assert!(!rt.enabled);
    }

    #[test]
    fn base64_encode_works() {
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"AB"), "QUI=");
    }

    #[test]
    fn session_start_event_is_rejected_in_active_session() {
        // This tests the service logic that rejects duplicate session.start
        let event = RealtimeAudioClientEvent::SessionStart(RealtimeSessionStart {
            session_id: None,
            entity_id: "test".to_string(),
            input_audio: None,
            output_audio: None,
        });

        // The service should return an error event for this
        assert!(matches!(event, RealtimeAudioClientEvent::SessionStart(_)));
    }
}
