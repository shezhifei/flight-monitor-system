//! 操作员身份上下文模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorIdentityContext {
    pub user_id: String,
    pub context_type: String,
    pub context_id: String,
    pub operator_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
