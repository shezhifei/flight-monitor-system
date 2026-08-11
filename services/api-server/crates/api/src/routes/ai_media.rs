//! AI 媒体路由 — ASR（语音转写）与 TTS（文本合成语音）
//!
//! 提供语音转写和声音合成的 REST API 端点。

use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::services::ai_media_service::{AiMediaError, AiMediaService};
use fms_domain::models::ai_media::{AsrParams, TtsParams};

// ---------------------------------------------------------------------------
// 请求体
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TtsSynthesizeRequest {
    /// 要合成的文本内容
    text: String,

    /// AI 实体 ID（可选，为空时使用默认实体）
    entity_id: Option<String>,

    /// TTS 模型名称
    model: Option<String>,

    /// 声音标识
    voice: Option<String>,

    /// 输出音频格式
    response_format: Option<String>,

    /// 语速倍率 (0.25 ~ 4.0)
    speed: Option<f64>,
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}

fn media_error_to_response(err: AiMediaError) -> HttpResponse {
    match &err {
        AiMediaError::Disabled(msg) => HttpResponse::ServiceUnavailable().json(json!({
            "success": false,
            "message": msg,
            "code": "AI_MEDIA_DISABLED",
        })),
        AiMediaError::Validation(msg) => HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": msg,
            "code": "VALIDATION_ERROR",
        })),
        AiMediaError::Configuration(msg) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "message": msg,
            "code": "CONFIGURATION_ERROR",
        })),
        AiMediaError::FileTooLarge { max_mb, actual_mb } => HttpResponse::PayloadTooLarge().json(json!({
            "success": false,
            "message": format!("文件大小 {actual_mb}MB 超过限制 {max_mb}MB"),
            "code": "FILE_TOO_LARGE",
            "max_mb": max_mb,
            "actual_mb": actual_mb,
        })),
        AiMediaError::UnsupportedFormat(msg) => HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": msg,
            "code": "UNSUPPORTED_FORMAT",
        })),
        AiMediaError::Upstream(msg) => HttpResponse::BadGateway().json(json!({
            "success": false,
            "message": msg,
            "code": "UPSTREAM_ERROR",
        })),
        AiMediaError::Internal(msg) => HttpResponse::InternalServerError().json(json!({
            "success": false,
            "message": msg,
            "code": "INTERNAL_ERROR",
        })),
    }
}

// ---------------------------------------------------------------------------
// 端点处理函数
// ---------------------------------------------------------------------------

/// POST /api/v2/ai/media/transcribe
///
/// 上传音频文件进行 ASR 转写。
/// Content-Type: multipart/form-data
///
/// Fields:
///   - file: 音频文件（必填）
///   - entity_id: AI 实体 ID（可选）
///   - model: ASR 模型名称（可选，默认 whisper-1）
///   - language: 语言代码（可选，自动检测）
///   - prompt: 引导提示（可选）
///   - response_format: 响应格式（可选，默认 json）
///   - temperature: 采样温度（可选）
async fn transcribe(
    svc: web::Data<Arc<AiMediaService>>,
    claims: JwtAuth,
    mut payload: Multipart,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:media")?;

    let mut audio_data: Option<Vec<u8>> = None;
    let mut filename = "audio.bin".to_string();
    let mut entity_id: Option<String> = None;
    let mut params = AsrParams {
        model: String::new(),
        ..AsrParams::default()
    };

    // 解析 multipart 表单
    while let Some(item) = payload.next().await {
        let mut field = item.map_err(|e| ApiError::BadRequest(format!("multipart 解析失败: {e}")))?;

        let field_name = field
            .content_disposition()
            .and_then(|cd| cd.get_name())
            .unwrap_or("")
            .to_string();

        match field_name.as_str() {
            "file" => {
                if let Some(name) = field.content_disposition().and_then(|cd| cd.get_filename()) {
                    filename = name.to_string();
                }
                let mut data = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(|e| ApiError::BadRequest(format!("读取文件数据失败: {e}")))?;
                    data.extend_from_slice(&chunk);
                }
                audio_data = Some(data);
            }
            "entity_id" => {
                entity_id = read_text_field(&mut field).await;
            }
            "model" => {
                if let Some(v) = read_text_field(&mut field).await {
                    params.model = v;
                }
            }
            "language" => {
                params.language = read_text_field(&mut field).await;
            }
            "prompt" => {
                params.prompt = read_text_field(&mut field).await;
            }
            "response_format" => {
                if let Some(v) = read_text_field(&mut field).await {
                    params.response_format = v;
                }
            }
            "temperature" => {
                if let Some(v) = read_text_field(&mut field).await {
                    params.temperature = v.parse().ok();
                }
            }
            _ => {
                // 忽略未知字段
            }
        }
    }

    let audio_data = audio_data.ok_or_else(|| ApiError::BadRequest("缺少必填的 file 字段".into()))?;

    if audio_data.is_empty() {
        return Err(ApiError::BadRequest("上传的音频文件为空".into()));
    }

    match svc
        .transcribe(entity_id.as_deref(), audio_data, &filename, params)
        .await
    {
        Ok(result) => Ok(ok_resp(result)),
        Err(err) => Ok(media_error_to_response(err)),
    }
}

/// POST /api/v2/ai/media/synthesize
///
/// 提交文本进行 TTS 合成，返回音频二进制流。
async fn synthesize(
    svc: web::Data<Arc<AiMediaService>>,
    claims: JwtAuth,
    body: web::Json<TtsSynthesizeRequest>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:media")?;

    let params = TtsParams {
        model: body.model.as_deref().unwrap_or_default().to_owned(),
        voice: body.voice.as_deref().unwrap_or_default().to_owned(),
        response_format: body.response_format.clone().unwrap_or_else(|| "mp3".to_string()),
        speed: body.speed.unwrap_or(1.0),
    };

    match svc.synthesize(body.entity_id.as_deref(), &body.text, params).await {
        Ok(result) => Ok(HttpResponse::Ok()
            .content_type(result.content_type)
            .insert_header((
                "Content-Disposition",
                format!("attachment; filename=\"speech.{}\"", result.format),
            ))
            .body(result.audio_data)),
        Err(err) => Ok(media_error_to_response(err)),
    }
}

/// GET /api/v2/ai/media/capabilities
///
/// 查询 ASR/TTS 能力状态和配置信息。
async fn capabilities(svc: web::Data<Arc<AiMediaService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:media")?;
    Ok(ok_resp(svc.capabilities()))
}

/// GET /api/v2/ai/media/voices
///
/// 列出可用的 TTS 声音。
async fn voices(svc: web::Data<Arc<AiMediaService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:media")?;
    let voices = svc.list_voices();
    Ok(ok_resp(json!({
        "voices": voices,
        "total": voices.len(),
    })))
}

/// GET /api/v2/ai/media/formats
///
/// 列出支持的音频格式。
async fn formats(svc: web::Data<Arc<AiMediaService>>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:media")?;
    Ok(ok_resp(svc.supported_formats()))
}

// ---------------------------------------------------------------------------
// multipart 辅助
// ---------------------------------------------------------------------------

async fn read_text_field(field: &mut actix_multipart::Field) -> Option<String> {
    let mut buf = Vec::new();
    while let Some(chunk) = field.next().await {
        if let Ok(data) = chunk {
            buf.extend_from_slice(&data);
        }
    }
    String::from_utf8(buf)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// 路由注册
// ---------------------------------------------------------------------------

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/media")
            .route("/transcribe", web::post().to(transcribe))
            .route("/synthesize", web::post().to(synthesize))
            .route("/capabilities", web::get().to(capabilities))
            .route("/voices", web::get().to(voices))
            .route("/formats", web::get().to(formats)),
    );
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test as actix_test;

    #[test]
    fn tts_request_deserializes() {
        let json = r#"{
            "text": "Hello world",
            "voice": "nova",
            "speed": 1.5
        }"#;
        let req: TtsSynthesizeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.text, "Hello world");
        assert_eq!(req.voice.unwrap(), "nova");
        assert!((req.speed.unwrap() - 1.5).abs() < f64::EPSILON);
        assert!(req.entity_id.is_none());
        assert!(req.model.is_none());
    }

    #[actix_web::test]
    async fn media_routes_are_mounted() {
        let app = actix_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(crate::middleware::jwt::JwtSecret(
                    "test-secret".to_string(),
                )))
                .configure(configure),
        )
        .await;

        let paths = [
            "/api/v2/ai/media/capabilities",
            "/api/v2/ai/media/voices",
            "/api/v2/ai/media/formats",
        ];

        for path in paths {
            let req = actix_test::TestRequest::get().uri(path).to_request();
            let resp = actix_test::call_service(&app, req).await;
            assert_ne!(
                resp.status(),
                actix_web::http::StatusCode::NOT_FOUND,
                "route not mounted: {path}"
            );
        }

        // POST routes
        let post_paths = ["/api/v2/ai/media/transcribe", "/api/v2/ai/media/synthesize"];

        for path in post_paths {
            let req = actix_test::TestRequest::post().uri(path).to_request();
            let resp = actix_test::call_service(&app, req).await;
            assert_ne!(
                resp.status(),
                actix_web::http::StatusCode::NOT_FOUND,
                "route not mounted: {path}"
            );
        }
    }
}
