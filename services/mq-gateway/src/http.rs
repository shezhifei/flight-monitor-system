use std::sync::Arc;
use std::sync::Once;

use actix_web::body::BoxBody;
use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, Responder};

use crate::api::{AckRequest, PublishRequest, PublishResponse, ReceiveRequest, ReceiveResponse};
use crate::transport::{MessageTransport, TransportError};

type TransportData = web::Data<Arc<dyn MessageTransport>>;
const TOKEN_HEADER: &str = "x-mq-gateway-token";
static MISSING_TOKEN_WARNING: Once = Once::new();

#[derive(Clone)]
pub struct AuthToken {
    token: Option<String>,
    require_auth: bool,
}

pub fn app<T>(
    transport: T,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<BoxBody>,
        Error = Error,
        InitError = (),
    >,
>
where
    T: MessageTransport + 'static,
{
    let env = runtime_environment();
    let is_prod = is_production_environment(env.as_deref());
    app_with_token_and_env(transport, auth_token_from_env(), is_prod)
}

pub fn app_with_token<T>(
    transport: T,
    token: Option<String>,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<BoxBody>,
        Error = Error,
        InitError = (),
    >,
>
where
    T: MessageTransport + 'static,
{
    app_with_token_and_env(transport, token, false)
}

pub fn app_with_token_and_env<T>(
    transport: T,
    token: Option<String>,
    is_production: bool,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<BoxBody>,
        Error = Error,
        InitError = (),
    >,
>
where
    T: MessageTransport + 'static,
{
    App::new()
        .app_data(web::Data::new(
            Arc::new(transport) as Arc<dyn MessageTransport>
        ))
        .app_data(web::Data::new(AuthToken::new(token, is_production)))
        .configure(configure_routes)
}

pub fn auth_token_from_env() -> Option<String> {
    std::env::var("MQ_GATEWAY_TOKEN")
        .ok()
        .and_then(|value| normalize_token(&value))
}

pub fn runtime_environment() -> Option<String> {
    std::env::var("APP_ENVIRONMENT")
        .or_else(|_| std::env::var("APP_ENV"))
        .or_else(|_| std::env::var("ENVIRONMENT"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn is_production_environment(environment: Option<&str>) -> bool {
    match environment.map(str::trim) {
        None | Some("") => true,
        Some(value)
            if value.eq_ignore_ascii_case("development")
                || value.eq_ignore_ascii_case("dev")
                || value.eq_ignore_ascii_case("test")
                || value.eq_ignore_ascii_case("testing")
                || value.eq_ignore_ascii_case("local")
                || value.eq_ignore_ascii_case("localhost") =>
        {
            false
        }
        Some(_) => true,
    }
}

pub fn auth_token_data_from_env() -> web::Data<AuthToken> {
    let env = runtime_environment();
    let is_prod = is_production_environment(env.as_deref());
    web::Data::new(AuthToken::new(auth_token_from_env(), is_prod))
}

impl AuthToken {
    fn new(token: Option<String>, is_production: bool) -> Self {
        let token = token.and_then(|value| normalize_token(&value));
        let require_auth = is_production;
        if token.is_none() {
            if require_auth {
                log::error!(
                    "MQ_GATEWAY_TOKEN is not configured; running in production mode - write endpoints will reject all requests"
                );
            } else {
                MISSING_TOKEN_WARNING.call_once(|| {
                    log::warn!(
                        "MQ_GATEWAY_TOKEN is not configured; write endpoints are unauthenticated (dev/test mode)"
                    );
                });
            }
        }
        Self {
            token,
            require_auth,
        }
    }
}

pub fn configure_routes(config: &mut web::ServiceConfig) {
    config
        .service(web::resource("/health").route(web::get().to(health)))
        .service(web::resource("/messages/publish").route(web::post().to(publish)))
        .service(web::resource("/messages/receive").route(web::post().to(receive)))
        .service(web::resource("/messages/ack").route(web::post().to(ack)));
}

async fn health(transport: TransportData) -> impl Responder {
    match transport.health().await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "healthy"})),
        Err(error) => transport_error_response(error),
    }
}

async fn publish(
    request: HttpRequest,
    auth_token: Option<web::Data<AuthToken>>,
    transport: TransportData,
    payload: web::Json<PublishRequest>,
) -> impl Responder {
    if !is_authorized(&request, auth_token.as_ref().map(|token| token.get_ref())) {
        return unauthorized_response();
    }

    let request = match payload.into_inner().validate() {
        Ok(request) => request,
        Err(error) => return HttpResponse::BadRequest().json(error_payload(error.to_string())),
    };

    match transport.publish(request).await {
        Ok(message_id) => HttpResponse::Ok().json(PublishResponse { message_id }),
        Err(error) => transport_error_response(error),
    }
}

async fn receive(
    request: HttpRequest,
    auth_token: Option<web::Data<AuthToken>>,
    transport: TransportData,
    payload: web::Json<ReceiveRequest>,
) -> impl Responder {
    if !is_authorized(&request, auth_token.as_ref().map(|token| token.get_ref())) {
        return unauthorized_response();
    }

    let request = match payload.into_inner().validate() {
        Ok(request) => request,
        Err(error) => return HttpResponse::BadRequest().json(error_payload(error.to_string())),
    };

    match transport.receive(request).await {
        Ok(messages) => HttpResponse::Ok().json(ReceiveResponse { messages }),
        Err(error) => transport_error_response(error),
    }
}

async fn ack(
    request: HttpRequest,
    auth_token: Option<web::Data<AuthToken>>,
    transport: TransportData,
    payload: web::Json<AckRequest>,
) -> impl Responder {
    if !is_authorized(&request, auth_token.as_ref().map(|token| token.get_ref())) {
        return unauthorized_response();
    }

    let request = match payload.into_inner().validate() {
        Ok(request) => request,
        Err(error) => return HttpResponse::BadRequest().json(error_payload(error.to_string())),
    };

    match transport.ack(request).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(TransportError::UnknownReceipt(error)) => {
            HttpResponse::NotFound().json(error_payload(error))
        }
        Err(error) => transport_error_response(error),
    }
}

fn transport_error_response(error: TransportError) -> HttpResponse {
    match error {
        TransportError::Unavailable(message) => {
            HttpResponse::ServiceUnavailable().json(error_payload(message))
        }
        TransportError::UnknownReceipt(message) => {
            HttpResponse::NotFound().json(error_payload(message))
        }
        TransportError::Backend(message) => HttpResponse::BadGateway().json(error_payload(message)),
    }
}

fn is_authorized(request: &HttpRequest, auth_token: Option<&AuthToken>) -> bool {
    let Some(auth) = auth_token else {
        return true;
    };

    let Some(expected) = auth.token.as_deref() else {
        return !auth.require_auth;
    };

    bearer_token(request) == Some(expected) || header_token(request, TOKEN_HEADER) == Some(expected)
}

fn bearer_token(request: &HttpRequest) -> Option<&str> {
    let header = request
        .headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .trim();
    header.strip_prefix("Bearer ").map(str::trim)
}

fn header_token<'a>(request: &'a HttpRequest, header_name: &str) -> Option<&'a str> {
    request
        .headers()
        .get(header_name)?
        .to_str()
        .ok()
        .map(str::trim)
}

fn unauthorized_response() -> HttpResponse {
    HttpResponse::Unauthorized().json(error_payload("missing or invalid mq gateway token"))
}

fn normalize_token(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn error_payload(message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({ "error": message.into() })
}
