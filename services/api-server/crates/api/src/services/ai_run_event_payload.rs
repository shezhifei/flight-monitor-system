use serde_json::{json, Value};

const MAX_STRING_LEN: usize = 512;
const MAX_PAYLOAD_SIZE: usize = 4096;

pub fn sanitize_event_payload_opt(value: Option<Value>) -> Option<Value> {
    value.map(sanitize_event_payload)
}

pub fn sanitize_event_payload(mut value: Value) -> Value {
    redact_value(&mut value);
    let s = value.to_string();
    if s.len() > MAX_PAYLOAD_SIZE {
        json!({"_redacted": "payload_exceeded_4kb", "original_size": s.len()})
    } else {
        value
    }
}

fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let k_lower = k.to_lowercase();
                if is_sensitive_key(&k_lower) {
                    *v = Value::String("[REDACTED]".to_string());
                } else {
                    redact_value(v);
                }
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_value(v);
            }
        }
        Value::String(s) => {
            if s.contains("sk-")
                || s.contains("Bearer ")
                || s.contains("bearer ")
                || s.starts_with("eyJ")
                || s.contains("Basic ")
                || s.contains("basic ")
            {
                *s = "[REDACTED]".to_string();
            } else if s.len() > MAX_STRING_LEN {
                *s = format!("{}...[TRUNCATED]", &s[..MAX_STRING_LEN]);
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let sensitive_keys = [
        "authorization",
        "token",
        "jwt",
        "api_key",
        "apikey",
        "password",
        "secret",
        "bearer",
        "prompt",
        "user_message",
        "arguments",
        "result",
        "error_stack",
        "access_token",
        "refresh_token",
        "private_key",
        "connection_string",
        "database_url",
        "redis_url",
        "old_password",
        "new_password",
        "confirm_password",
        "encryption_key",
        "signing_key",
        "client_secret",
    ];
    sensitive_keys.contains(&key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sanitize_event_payload() {
        let payload = json!({
            "authorization": "Bearer xxx",
            "metadata": {
                "tool_name": "test_tool",
                "api_key": "sk-12345",
                "normal": "value",
                "long_string": "a".repeat(1000)
            },
            "array": [
                {
                    "password": "pwd"
                }
            ]
        });

        let sanitized = sanitize_event_payload(payload);
        assert_eq!(sanitized["authorization"], "[REDACTED]");
        assert_eq!(sanitized["metadata"]["api_key"], "[REDACTED]");
        assert_eq!(sanitized["metadata"]["tool_name"], "test_tool");
        assert_eq!(sanitized["metadata"]["normal"], "value");
        assert!(sanitized["metadata"]["long_string"]
            .as_str()
            .unwrap()
            .ends_with("[TRUNCATED]"));
        let truncated_str = sanitized["metadata"]["long_string"].as_str().unwrap();
        assert!(
            truncated_str.len() > 512,
            "truncated string should be longer than MAX_STRING_LEN"
        );
        assert!(
            truncated_str.len() < 540,
            "truncated string should not be excessively long"
        );
        assert!(
            truncated_str.starts_with("aaa"),
            "truncated string should start with original content"
        );
        assert_eq!(sanitized["array"][0]["password"], "[REDACTED]");
    }

    #[test]
    fn test_payload_too_large() {
        let mut large_map = serde_json::Map::new();
        for i in 0..100 {
            large_map.insert(format!("key_{}", i), Value::String("a".repeat(100)));
        }
        let payload = Value::Object(large_map);
        let sanitized = sanitize_event_payload(payload);
        assert_eq!(sanitized["_redacted"], "payload_exceeded_4kb");
    }

    #[test]
    fn test_expanded_sensitive_keys_redacted() {
        let payload = json!({
            "access_token": "should-be-hidden",
            "refresh_token": "should-be-hidden",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----",
            "connection_string": "postgres://user:pass@host/db",
            "database_url": "postgres://admin:secret@db:5432/fms",
            "redis_url": "redis://:password@redis:6379",
            "old_password": "old-pwd",
            "new_password": "new-pwd",
            "confirm_password": "confirm-pwd",
            "encryption_key": "aes-key-material",
            "signing_key": "hmac-key-material",
            "client_secret": "oauth-client-secret"
        });

        let sanitized = sanitize_event_payload(payload);
        for key in &[
            "access_token",
            "refresh_token",
            "private_key",
            "connection_string",
            "database_url",
            "redis_url",
            "old_password",
            "new_password",
            "confirm_password",
            "encryption_key",
            "signing_key",
            "client_secret",
        ] {
            assert_eq!(sanitized[*key], "[REDACTED]", "key '{key}' should be redacted");
        }
    }

    #[test]
    fn test_jwt_and_basic_auth_strings_redacted() {
        let payload = json!({
            "jwt_header": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            "basic_auth": "Basic dXNlcjpwYXNz",
            "normal_text": "this should not be redacted"
        });

        let sanitized = sanitize_event_payload(payload);
        assert_eq!(sanitized["jwt_header"], "[REDACTED]");
        assert_eq!(sanitized["basic_auth"], "[REDACTED]");
        assert_eq!(sanitized["normal_text"], "this should not be redacted");
    }

    #[test]
    fn test_nested_sensitive_keys_in_deep_structure() {
        let payload = json!({
            "level1": {
                "level2": {
                    "access_token": "deep-secret",
                    "nested_array": [
                        { "client_secret": "array-nested-secret" }
                    ]
                }
            }
        });

        let sanitized = sanitize_event_payload(payload);
        assert_eq!(sanitized["level1"]["level2"]["access_token"], "[REDACTED]");
        assert_eq!(
            sanitized["level1"]["level2"]["nested_array"][0]["client_secret"],
            "[REDACTED]"
        );
    }
}
