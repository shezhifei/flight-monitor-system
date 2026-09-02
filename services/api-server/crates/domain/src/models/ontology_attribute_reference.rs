use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyAttributeReference {
    pub id: Option<i64>,
    pub owner_object_name: String,
    pub owner_object_id: String,
    pub field_name: String,
    pub target_object_name: String,
    pub target_key: String,
    pub created_at: Option<DateTime<Utc>>,
}
