//! Broadcast suggestion: builds a `send` proposal with no send side effects.

use serde_json::{json, Value};

use super::error::OntologyActionError;
use super::support::{arg_str, constraint, required_str, suggestion_envelope};

pub struct NotificationBroadcastAdvisorService;

impl NotificationBroadcastAdvisorService {
    pub fn new() -> Self {
        Self
    }

    pub async fn suggest(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let title = required_str(args, "title")?;
        let body = required_str(args, "body")?;
        let scope = arg_str(args, "scope").unwrap_or("all");
        if !matches!(scope, "all" | "on_duty_teams" | "department") {
            return Err(OntologyActionError::InvalidArguments(
                "`scope` must be one of all|on_duty_teams|department".to_string(),
            ));
        }
        if scope == "department" && arg_str(args, "department_id").is_none() {
            return Err(OntologyActionError::InvalidArguments(
                "`department_id` is required when scope is department".to_string(),
            ));
        }

        let recipients = match scope {
            "on_duty_teams" => json!({ "kind": "team_status", "team_status": "on_duty" }),
            "department" => json!({ "kind": "department", "department_id": arg_str(args, "department_id") }),
            _ => json!({ "kind": "all_users" }),
        };

        let constraint_results = vec![
            constraint("title_present", true, "error", None),
            constraint("body_present", true, "error", None),
            constraint("recipients_resolvable", true, "warning", None),
        ];

        Ok(suggestion_envelope(
            "Notification",
            "broadcast",
            "send",
            json!({ "title": title, "body": body, "recipients": recipients }),
            "medium",
            constraint_results,
            Value::Null,
            json!({ "title": title, "recipients": recipients }),
            0.9,
            &format!("broadcast proposal '{title}' for scope {scope}"),
            json!({ "side_effects": "none until approval" }),
        ))
    }
}

impl Default for NotificationBroadcastAdvisorService {
    fn default() -> Self {
        Self::new()
    }
}
