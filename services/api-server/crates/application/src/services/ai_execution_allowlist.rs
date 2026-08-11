#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionAllowlist {
    Disabled,
    AllowAll,
    AllowList(Vec<String>),
}

impl ExecutionAllowlist {
    pub fn from_env() -> Self {
        Self::parse(&std::env::var("FMS_AI_PROPOSAL_EXECUTION_ENABLED").unwrap_or_default())
    }

    pub fn parse(value: &str) -> Self {
        let trimmed = value.trim();
        let lower = trimmed.to_ascii_lowercase();

        if trimmed.is_empty() || matches!(lower.as_str(), "false" | "0" | "no" | "off") {
            return Self::Disabled;
        }

        if matches!(lower.as_str(), "true" | "1" | "yes" | "on") {
            return Self::AllowAll;
        }

        let mut actions: Vec<String> = trimmed
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect();
        actions.sort_by_key(|item| item.to_ascii_lowercase());
        actions.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

        if actions.is_empty() {
            Self::Disabled
        } else {
            Self::AllowList(actions)
        }
    }

    pub fn is_execution_enabled(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    pub fn requires_readiness_override(&self) -> bool {
        self.is_execution_enabled()
    }

    pub fn execution_mode(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::AllowAll => "allow_all",
            Self::AllowList(_) => "allowlist",
        }
    }

    pub fn allowed_actions(&self) -> Vec<String> {
        match self {
            Self::AllowList(actions) => actions.clone(),
            Self::Disabled | Self::AllowAll => Vec::new(),
        }
    }

    pub fn allows(&self, object_type: &str, action_name: &str) -> bool {
        match self {
            Self::Disabled => false,
            Self::AllowAll => true,
            Self::AllowList(actions) => {
                let key = format!("{object_type}.{action_name}");
                actions.iter().any(|item| item.eq_ignore_ascii_case(&key))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_disabled_values() {
        assert_eq!(ExecutionAllowlist::parse(""), ExecutionAllowlist::Disabled);
        assert_eq!(ExecutionAllowlist::parse("false"), ExecutionAllowlist::Disabled);
        assert_eq!(ExecutionAllowlist::parse("0"), ExecutionAllowlist::Disabled);
        assert_eq!(ExecutionAllowlist::parse("off"), ExecutionAllowlist::Disabled);
    }

    #[test]
    fn parses_allow_all_values() {
        assert_eq!(ExecutionAllowlist::parse("true"), ExecutionAllowlist::AllowAll);
        assert_eq!(ExecutionAllowlist::parse("1"), ExecutionAllowlist::AllowAll);
        assert_eq!(ExecutionAllowlist::parse("on"), ExecutionAllowlist::AllowAll);
    }

    #[test]
    fn parses_allowlist_values() {
        assert_eq!(
            ExecutionAllowlist::parse("Todo.create, Notification.send"),
            ExecutionAllowlist::AllowList(vec!["Notification.send".to_string(), "Todo.create".to_string(),])
        );
    }

    #[test]
    fn allowlist_matches_case_insensitively() {
        let allowlist = ExecutionAllowlist::parse("Todo.create");
        assert!(allowlist.allows("todo", "CREATE"));
        assert!(!allowlist.allows("Flight", "add_note"));
    }

    #[test]
    fn execution_mode_is_explicit() {
        assert_eq!(ExecutionAllowlist::Disabled.execution_mode(), "disabled");
        assert_eq!(ExecutionAllowlist::AllowAll.execution_mode(), "allow_all");
        assert_eq!(
            ExecutionAllowlist::AllowList(vec!["Todo.create".to_string()]).execution_mode(),
            "allowlist"
        );
    }
}
