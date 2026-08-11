use serde_json::Value;

use super::types::DomainActionError;

pub(super) fn required_string<'a>(
    arguments: &'a Value,
    keys: &[&str],
    label: &str,
) -> Result<&'a str, DomainActionError> {
    optional_string(arguments, keys)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DomainActionError::Validation(format!("{label} is required")))
}

pub(super) fn optional_string<'a>(arguments: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| arguments.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
