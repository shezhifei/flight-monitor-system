/// Permission required to run a read action. `None` means the name is not a read action.
pub fn read_action_permission(action_name: &str) -> Option<&'static str> {
    match action_name {
        "flight.get_context" | "flight.search" => Some("flight:read"),
        "dispatch.get_status" => Some("dispatch:read"),
        "anomaly.list_open" => Some("anomaly:read"),
        "stand.check_availability" => Some("flight:read"),
        "report.generate_briefing" => Some("flight:read"),
        _ => None,
    }
}

/// Permission required to run an advisory action. `None` means the name is not an advisory action.
pub fn advisory_action_permission(action_name: &str) -> Option<&'static str> {
    match action_name {
        "flight.suggest_stand_adjustment" | "flight.suggest_delay_action" => Some("flight:read"),
        "dispatch.suggest_replan" => Some("dispatch:read"),
        "anomaly.suggest_escalation" => Some("anomaly:read"),
        "notification.suggest_broadcast" => Some("notification:send"),
        _ => None,
    }
}
