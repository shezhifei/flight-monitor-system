use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiObjectPolicySubject {
    pub user_id: String,
    pub permissions: Vec<String>,
    pub roles: Vec<String>,
    pub department_id: Option<String>,
}

impl AiObjectPolicySubject {
    pub fn new(user_id: impl Into<String>, permissions: Vec<String>) -> Self {
        Self {
            user_id: user_id.into(),
            permissions,
            roles: Vec::new(),
            department_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiObjectAccessRequest {
    pub subject: AiObjectPolicySubject,
    pub object_type: String,
    pub object_id: Option<String>,
    pub permission: String,
    pub object_snapshot: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiObjectAccessDecision {
    Allow,
    Deny,
    NoPolicy,
}

impl AiObjectAccessDecision {
    pub fn is_denied(self) -> bool {
        self == Self::Deny
    }
}

#[async_trait]
pub trait AiObjectPolicyRepository {
    async fn evaluate_access(
        &self,
        request: &AiObjectAccessRequest,
    ) -> Result<AiObjectAccessDecision, AiObjectPolicyRepositoryError>;
}

#[derive(Debug, Clone)]
pub enum AiObjectPolicyRepositoryError {
    Database(String),
}

impl std::fmt::Display for AiObjectPolicyRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(message) => write!(f, "database error: {}", message),
        }
    }
}

impl std::error::Error for AiObjectPolicyRepositoryError {}
