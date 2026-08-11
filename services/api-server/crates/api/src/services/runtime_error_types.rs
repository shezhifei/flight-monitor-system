use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Debug,
    Info,
    Warning,
    Error,
    High,
    Medium,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "error".to_string());
        write!(f, "{s}")
    }
}

impl FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warning" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "critical" => Ok(Self::Critical),
            other => Err(format!("unknown severity: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    System,
    Infrastructure,
    #[serde(other)]
    Other,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::Infrastructure => write!(f, "infrastructure"),
            Self::Other => write!(f, "other"),
        }
    }
}

impl FromStr for ErrorCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => Ok(Self::System),
            "infrastructure" => Ok(Self::Infrastructure),
            _ => Ok(Self::Other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    ApiInternalError,
    ApiServiceUnavailable,
    ApiInfraError,
    SchedulerTask,
    UnhandledActixError,
    SseConnectionLimitReached,
    Custom(String),
}

impl RuntimeErrorKind {
    pub fn label(&self) -> String {
        match self {
            Self::ApiInternalError => "ApiInternalError".to_string(),
            Self::ApiServiceUnavailable => "ApiServiceUnavailable".to_string(),
            Self::ApiInfraError => "ApiInfraError".to_string(),
            Self::SchedulerTask => "scheduler_task".to_string(),
            Self::UnhandledActixError => "UnhandledActixError".to_string(),
            Self::SseConnectionLimitReached => "SseConnectionLimitReached".to_string(),
            Self::Custom(value) => value.clone(),
        }
    }

    pub fn from_label(label: &str) -> Self {
        match label {
            "ApiInternalError" => Self::ApiInternalError,
            "ApiServiceUnavailable" => Self::ApiServiceUnavailable,
            "ApiInfraError" => Self::ApiInfraError,
            "scheduler_task" => Self::SchedulerTask,
            "UnhandledActixError" => Self::UnhandledActixError,
            "SseConnectionLimitReached" => Self::SseConnectionLimitReached,
            other => Self::Custom(other.to_string()),
        }
    }
}

impl Hash for RuntimeErrorKind {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.label().hash(state);
    }
}

impl Serialize for RuntimeErrorKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.label())
    }
}

impl<'de> Deserialize<'de> for RuntimeErrorKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_label(&value))
    }
}
