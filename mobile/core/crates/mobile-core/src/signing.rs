//! Anti-replay request signing.
//!
//! Byte-for-byte contract with the backend
//! (`services/api-server/crates/api/src/middleware/anti_replay.rs`):
//!
//! ```text
//! bodyHash  = hex(SHA-256(body)); GET/HEAD always use the empty-body constant
//!             (the backend rejects GET/HEAD requests carrying a body)
//! uri       = path + ("?" + query when non-empty)
//! payload   = "{METHOD}:{uri}:{timestamp}:{nonce}:{bodyHash}"   // METHOD uppercase
//! signature = hex(HMAC-SHA256(key = session_secret UTF-8 bytes, msg = payload UTF-8))
//! ```

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// hex(SHA-256("")); the fixed body hash for GET/HEAD requests.
pub const EMPTY_BODY_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The four anti-replay header values for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHeaders {
    /// `X-Request-Timestamp` — epoch seconds, as a string.
    pub timestamp: String,
    /// `X-Request-Nonce` — 32 hex chars (UUID without dashes).
    pub nonce: String,
    /// `X-Request-Body-SHA256`
    pub body_sha256: String,
    /// `X-Request-Signature`
    pub signature: String,
}

/// Streaming hex SHA-256 of a body chunk sequence. Use incremental `update`
/// for large bodies (e.g. multipart uploads) — never buffer the whole body.
pub fn body_hash_hex(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    hex::encode(hasher.finalize())
}

/// Sign one request.
///
/// `method` is case-insensitive and is uppercased into the payload, mirroring
/// the backend's `req.method().as_str()`. For GET/HEAD the body hash is
/// forced to [`EMPTY_BODY_SHA256`] regardless of `body` (the backend rejects
/// GET/HEAD with a body; the client pipeline must never send one).
///
/// `session_secret` is the hex string returned by login/refresh; its UTF-8
/// bytes are the HMAC key, matching the backend verbatim.
pub fn sign_request(
    method: &str,
    path_and_query: &str,
    body: &[u8],
    session_secret: &str,
    timestamp: i64,
    nonce: &str,
) -> SignatureHeaders {
    let body_hash = if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
        EMPTY_BODY_SHA256.to_string()
    } else {
        body_hash_hex(body)
    };
    let payload = format!(
        "{}:{}:{}:{}:{}",
        method.to_uppercase(),
        path_and_query,
        timestamp,
        nonce,
        body_hash
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(session_secret.as_bytes())
        .expect("HMAC-SHA256 accepts keys of any length");
    mac.update(payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    SignatureHeaders {
        timestamp: timestamp.to_string(),
        nonce: nonce.to_string(),
        body_sha256: body_hash,
        signature,
    }
}

/// Generate a fresh nonce: 32 hex chars (UUID v4 without dashes).
pub fn fresh_nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locked test vector. Expected values computed independently
    /// and cross-checked against the backend `anti_replay.rs` payload format
    /// (`format!("{}:{}:{}:{}:{}", method_str, uri, timestamp_str, nonce, body_hash)`).
    #[test]
    fn locked_test_vector() {
        let headers = sign_request(
            "POST",
            "/api/v2/dispatch-orders/abc/accept?t=1",
            b"{\"a\":1}",
            "deadbeef",
            1_700_000_000,
            "0123456789abcdef0123456789abcdef",
        );
        assert_eq!(headers.timestamp, "1700000000");
        assert_eq!(headers.nonce, "0123456789abcdef0123456789abcdef");
        assert_eq!(
            headers.body_sha256,
            "015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862"
        );
        assert_eq!(
            headers.signature,
            "21957da175858f68061185c8e678b32d6e603f16bffc1a4ea7b53f06727438de"
        );
    }

    #[test]
    fn empty_body_sha256_constant_is_correct() {
        assert_eq!(body_hash_hex(b""), EMPTY_BODY_SHA256);
    }

    #[test]
    fn get_and_head_force_empty_body_hash() {
        for method in ["GET", "get", "HEAD", "head"] {
            let headers = sign_request(method, "/api/v2/ping", b"ignored", "s", 1, "n");
            assert_eq!(headers.body_sha256, EMPTY_BODY_SHA256, "method {method}");
        }
    }

    #[test]
    fn method_is_uppercased_in_payload() {
        let upper = sign_request("POST", "/p", b"x", "s", 1, "n");
        let lower = sign_request("post", "/p", b"x", "s", 1, "n");
        assert_eq!(upper.signature, lower.signature);
    }

    #[test]
    fn nonce_format_is_32_hex_chars() {
        let nonce = fresh_nonce();
        assert_eq!(nonce.len(), 32);
        assert!(nonce.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn different_bodies_produce_different_hashes() {
        assert_ne!(body_hash_hex(b"a"), body_hash_hex(b"b"));
    }
}
