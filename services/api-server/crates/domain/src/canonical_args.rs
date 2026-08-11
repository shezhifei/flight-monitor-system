//! Canonical args serialization and hashing.
//!
//! Both the Python sidecar and the Rust API must produce identical
//! canonical args hashes for the same logical input. The canonical form
//! is the SHA-256 of a deterministic UTF-8 JSON string:
//!
//! * Object keys are sorted alphabetically (recursively).
//! * No whitespace between tokens.
//! * Non-ASCII characters are preserved (not escaped).
//! * Missing keys and explicit `null` values are distinct: missing keys
//!   do not appear, explicit `null` does.
//!
//! Python reference implementation (used to generate the cross-language
//! test vectors committed in this module):
//!
//! ```python
//! import json
//!
//! def canonical_json_args(args: dict) -> str:
//!     return json.dumps(
//!         args,
//!         sort_keys=True,
//!         separators=(",", ":"),
//!         ensure_ascii=False,
//!     )
//!
//! def canonical_args_hash(args: dict) -> str:
//!     import hashlib
//!     return hashlib.sha256(
//!         canonical_json_args(args).encode("utf-8")
//!     ).hexdigest()
//! ```

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Recursively sort object keys so the resulting [`Value`] has a
/// deterministic serialized form.
fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().map(|(k, v)| (k, canonicalize(v))).collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                sorted.insert(k, v);
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize).collect()),
        other => other,
    }
}

/// Serialize an args object into the canonical JSON form used for
/// idempotency hashing. The output matches what the Python
/// implementation produces for the same logical input.
pub fn canonical_json_args(args: &Value) -> String {
    let canonical = canonicalize(args.clone());
    serde_json::to_string(&canonical).unwrap_or_default()
}

/// SHA-256 hex digest of [`canonical_json_args`].
pub fn canonical_args_hash(args: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_json_args(args).as_bytes());
    hex::encode(hasher.finalize())
}

/// Build the canonical idempotency key for a tool call.
///
/// Key shape: `run_id + ":" + round_index + ":" + tool_call_id + ":" + tool_name + ":" + canonical_args_hash`.
pub fn tool_call_idempotency_key(
    run_id: &str,
    round_index: u32,
    tool_call_id: &str,
    tool_name: &str,
    args: &Value,
) -> String {
    let args_hash = canonical_args_hash(args);
    format!("{run_id}:{round_index}:{tool_call_id}:{tool_name}:{args_hash}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_keys_are_sorted_alphabetically() {
        let input = json!({ "b": 1, "a": 2, "c": 3 });
        let output = canonical_json_args(&input);
        assert_eq!(output, r#"{"a":2,"b":1,"c":3}"#);
    }

    #[test]
    fn nested_objects_are_sorted_recursively() {
        let input = json!({
            "outer": { "z": 1, "a": { "y": 2, "b": 3 } },
            "first": "value"
        });
        let output = canonical_json_args(&input);
        assert_eq!(output, r#"{"first":"value","outer":{"a":{"b":3,"y":2},"z":1}}"#);
    }

    #[test]
    fn array_order_is_preserved() {
        let input = json!({ "items": [3, 1, 2] });
        let output = canonical_json_args(&input);
        assert_eq!(output, r#"{"items":[3,1,2]}"#);
    }

    #[test]
    fn null_values_are_distinct_from_missing_keys() {
        let explicit_null = json!({ "a": null });
        let missing = json!({});
        assert_ne!(canonical_json_args(&explicit_null), canonical_json_args(&missing));
        assert_eq!(canonical_json_args(&explicit_null), r#"{"a":null}"#);
        assert_eq!(canonical_json_args(&missing), r#"{}"#);
    }

    #[test]
    fn non_ascii_characters_are_preserved() {
        let input = json!({ "name": "航班CA1234", "city": "北京" });
        let output = canonical_json_args(&input);
        assert_eq!(output, r#"{"city":"北京","name":"航班CA1234"}"#);
    }

    #[test]
    fn empty_object_and_empty_array_serialize() {
        assert_eq!(canonical_json_args(&json!({})), "{}");
        assert_eq!(canonical_json_args(&json!([])), "[]");
    }

    #[test]
    fn hash_is_deterministic_across_key_orders() {
        let a = json!({ "flight_id": "CA1234", "status": "ON_TIME" });
        let b = json!({ "status": "ON_TIME", "flight_id": "CA1234" });
        assert_eq!(canonical_args_hash(&a), canonical_args_hash(&b));
    }

    /// Cross-language vector: this hash MUST match the value the
    /// Python `canonical_args_hash` returns for the same input.
    ///
    /// ```python
    /// canonical_args_hash({
    ///     "flight_id": "CA1234",
    ///     "status": "ON_TIME",
    ///     "tags": ["priority", "vip"],
    ///     "metadata": {"airport": "PEK", "gate": "B12"},
    /// })
    /// ```
    #[test]
    fn cross_language_vector_matches_python_reference() {
        let input = json!({
            "flight_id": "CA1234",
            "status": "ON_TIME",
            "tags": ["priority", "vip"],
            "metadata": {"airport": "PEK", "gate": "B12"}
        });
        // Computed via Python:
        //   json.dumps({"flight_id":"CA1234","metadata":{"airport":"PEK","gate":"B12"},
        //               "status":"ON_TIME","tags":["priority","vip"]},
        //              sort_keys=True, separators=(",", ":"),
        //              ensure_ascii=False).encode("utf-8") -> SHA-256
        let expected = "883af60772bce150610af6602572e27c45bd883acc01ee9bfc072901d6e972d3";
        let actual = canonical_args_hash(&input);
        assert_eq!(
            actual, expected,
            "Cross-language hash mismatch; expected {expected}, got {actual}"
        );
    }

    #[test]
    fn idempotency_key_format_is_stable() {
        let args = json!({ "flight_id": "CA1234" });
        let key = tool_call_idempotency_key("run-1", 0, "call-1", "flight_status_lookup", &args);
        assert!(key.starts_with("run-1:0:call-1:flight_status_lookup:"));
        // Same logical input → same key
        let key2 = tool_call_idempotency_key("run-1", 0, "call-1", "flight_status_lookup", &args);
        assert_eq!(key, key2);
    }

    #[test]
    fn idempotency_key_differs_for_different_args() {
        let args_a = json!({ "flight_id": "CA1234" });
        let args_b = json!({ "flight_id": "CA5678" });
        let key_a = tool_call_idempotency_key("run-1", 0, "call-1", "flight_status_lookup", &args_a);
        let key_b = tool_call_idempotency_key("run-1", 0, "call-1", "flight_status_lookup", &args_b);
        assert_ne!(key_a, key_b);
    }
}
