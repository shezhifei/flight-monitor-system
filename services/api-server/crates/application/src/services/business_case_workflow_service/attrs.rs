use fms_domain::error::DomainError;

pub(super) fn required_attr(node: &roxmltree::Node<'_, '_>, attr: &str, label: &str) -> Result<String, DomainError> {
    node.attribute(attr)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| DomainError::BusinessRuleViolation(format!("{label} missing {attr}")))
}

pub(super) fn parse_int_attr(
    value: Option<&str>,
    default: i32,
    minimum: i32,
    maximum: Option<i32>,
    label: &str,
) -> Result<i32, DomainError> {
    let parsed = match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value
            .parse::<i32>()
            .map_err(|_| DomainError::BusinessRuleViolation(format!("{label} must be an integer")))?,
        None => default,
    };
    if parsed < minimum {
        return Err(DomainError::BusinessRuleViolation(format!(
            "{label} must be >= {minimum}"
        )));
    }
    if let Some(maximum) = maximum {
        if parsed > maximum {
            return Err(DomainError::BusinessRuleViolation(format!(
                "{label} must be <= {maximum}"
            )));
        }
    }
    Ok(parsed)
}

pub(super) fn parse_bool_attr(value: Option<&str>, default: bool) -> bool {
    match value.map(|item| item.trim().to_ascii_lowercase()) {
        Some(value) if !value.is_empty() => {
            matches!(value.as_str(), "1" | "true" | "yes" | "y" | "on")
        }
        _ => default,
    }
}

pub(super) fn normalize_notification_severity(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" => "critical".to_string(),
        "error" => "error".to_string(),
        "warning" | "warn" => "warning".to_string(),
        "success" => "success".to_string(),
        "info" | "normal" => "info".to_string(),
        _ => "warning".to_string(),
    }
}

pub(super) fn split_csv(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
