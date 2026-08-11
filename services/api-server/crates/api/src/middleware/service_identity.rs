//! Service Identity Authentication for Rust internal API
//!
//! Validates X-Service-Identity JWT tokens from Rust API gateway.
//! This is different from user JwtAuth - it's for service-to-service authentication.

use actix_web::dev::Payload;
use actix_web::{web, FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::ApiError;
use crate::middleware::jwt::JwtSecret;

const SERVICE_IDENTITY_HEADER: &str = "X-Service-Identity";
const SERVICE_ISSUER: &str = "fms-rust-api";
const SERVICE_SUBJECT: &str = "rust-api-gateway";
const SERVICE_IDENTITY_AUDIENCE: &str = "python-ai-runtime";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceIdentityClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub iat: usize,
    pub exp: usize,
    pub path: String,
}

#[derive(Debug)]
pub enum ServiceIdentityError {
    Missing,
    Invalid(String),
    Expired,
    PathMismatch { expected: String, actual: String },
}

impl std::fmt::Display for ServiceIdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceIdentityError::Missing => write!(f, "Service identity token is missing"),
            ServiceIdentityError::Invalid(msg) => write!(f, "Invalid service identity: {}", msg),
            ServiceIdentityError::Expired => write!(f, "Service identity token has expired"),
            ServiceIdentityError::PathMismatch { expected, actual } => {
                write!(f, "Path mismatch: expected '{}', got '{}'", expected, actual)
            }
        }
    }
}

impl std::error::Error for ServiceIdentityError {}

pub struct ServiceIdentity(pub ServiceIdentityClaims);

impl FromRequest for ServiceIdentity {
    type Error = ApiError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move { extract_service_identity(&req).await })
    }
}

async fn extract_service_identity(req: &HttpRequest) -> Result<ServiceIdentity, ApiError> {
    let token = req
        .headers()
        .get(SERVICE_IDENTITY_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            warn!("Missing X-Service-Identity header on internal endpoint");
            ApiError::Unauthorized("Missing service identity token".into())
        })?;

    let secret = req
        .app_data::<web::Data<JwtSecret>>()
        .ok_or_else(|| ApiError::Internal("JWT secret not configured".into()))?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.leeway = 10;
    validation.set_audience(&[SERVICE_IDENTITY_AUDIENCE]);
    validation.set_issuer(&[SERVICE_ISSUER]);

    let decoded = decode::<ServiceIdentityClaims>(token, &DecodingKey::from_secret(secret.0.as_bytes()), &validation)
        .map_err(|e| {
        warn!(error = %e, "Service identity validation failed");
        if e.to_string().contains("ExpiredSignature") {
            ApiError::Unauthorized("Service identity token expired".into())
        } else {
            ApiError::Unauthorized("Invalid service identity token".into())
        }
    })?;

    let claims = decoded.claims;

    if claims.sub != SERVICE_SUBJECT {
        warn!(subject = %claims.sub, "Invalid service identity subject");
        return Err(ApiError::Unauthorized("Invalid service identity subject".into()));
    }

    let request_path = req.path();
    if claims.path != request_path {
        warn!(
            token_path = %claims.path,
            request_path = %request_path,
            "Service identity path mismatch"
        );
        return Err(ApiError::Forbidden("Service identity path mismatch".into()));
    }

    Ok(ServiceIdentity(claims))
}

pub struct OptionalServiceIdentity(pub Option<ServiceIdentityClaims>);

impl FromRequest for OptionalServiceIdentity {
    type Error = ApiError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move {
            match extract_service_identity(&req).await {
                Ok(ServiceIdentity(claims)) => Ok(OptionalServiceIdentity(Some(claims))),
                Err(_) => Ok(OptionalServiceIdentity(None)),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn create_test_token(secret: &str, path: &str, expired: bool) -> String {
        use jsonwebtoken::{encode, EncodingKey, Header};
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize;

        let exp = if expired { now - 100 } else { now + 60 };

        let claims = ServiceIdentityClaims {
            iss: SERVICE_ISSUER.to_string(),
            sub: SERVICE_SUBJECT.to_string(),
            aud: SERVICE_IDENTITY_AUDIENCE.to_string(),
            iat: now,
            exp,
            path: path.to_string(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn service_identity_claims_serialization() {
        let claims = ServiceIdentityClaims {
            iss: SERVICE_ISSUER.to_string(),
            sub: SERVICE_SUBJECT.to_string(),
            aud: SERVICE_IDENTITY_AUDIENCE.to_string(),
            iat: 1000,
            exp: 1060,
            path: "/internal/ai/v1/runs".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        assert!(json.contains("\"iss\":\"fms-rust-api\""));
        assert!(json.contains("\"sub\":\"rust-api-gateway\""));
        assert!(json.contains("\"aud\":\"python-ai-runtime\""));
        assert!(json.contains("\"/internal/ai/v1/runs\""));
    }

    #[test]
    fn service_identity_error_display() {
        let err = ServiceIdentityError::Missing;
        assert_eq!(err.to_string(), "Service identity token is missing");

        let err = ServiceIdentityError::Invalid("test".to_string());
        assert_eq!(err.to_string(), "Invalid service identity: test");

        let err = ServiceIdentityError::Expired;
        assert_eq!(err.to_string(), "Service identity token has expired");

        let err = ServiceIdentityError::PathMismatch {
            expected: "/expected".to_string(),
            actual: "/actual".to_string(),
        };
        assert_eq!(err.to_string(), "Path mismatch: expected '/expected', got '/actual'");
    }

    #[actix_web::test]
    async fn path_mismatch_error_is_generic_for_clients() {
        let token = create_test_token("test-secret", "/internal/ai/v1/runs/expected", false);
        let request = actix_web::test::TestRequest::post()
            .uri("/internal/ai/v1/runs/actual")
            .insert_header((SERVICE_IDENTITY_HEADER, token))
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .to_http_request();

        let error = match extract_service_identity(&request).await {
            Ok(_) => panic!("path mismatch should fail"),
            Err(error) => error,
        };

        assert_eq!(error.to_string(), "Forbidden: Service identity path mismatch");
        assert!(!error.to_string().contains("/internal/ai/v1/runs/expected"));
        assert!(!error.to_string().contains("/internal/ai/v1/runs/actual"));
    }
}
