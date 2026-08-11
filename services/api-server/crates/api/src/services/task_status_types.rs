use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Skipped,
    Completed,
    Failed,
    Running,
    Error,
    Active,
    Registered,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "active".to_string());
        write!(f, "{s}")
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "skipped" => Ok(Self::Skipped),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "running" => Ok(Self::Running),
            "error" => Ok(Self::Error),
            "active" => Ok(Self::Active),
            "registered" => Ok(Self::Registered),
            other => Err(format!("unknown task status: {other}")),
        }
    }
}
