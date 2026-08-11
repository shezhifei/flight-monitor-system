use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEnvelope {
    pub contract_version: String,
    pub job_id: String,
    pub run_id: String,
    pub correlation_id: String,
    pub requester: EnvelopeRequester,
    pub ontology: EnvelopeOntology,
    pub context: EnvelopeContext,
    pub task: EnvelopeTask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeRequester {
    pub user_id: String,
    pub roles: Vec<String>,
    pub department_id: Option<String>,
    pub permission_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeOntology {
    pub version: String,
    pub allowed_object_types: Vec<String>,
    pub allowed_actions: Vec<String>,
    pub risk_ceiling: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeContext {
    pub objects: Vec<EnvelopeObject>,
    pub relations: Vec<EnvelopeRelation>,
    pub evidence: Vec<EnvelopeEvidence>,
    pub limits: EnvelopeLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeObject {
    pub object_type: String,
    pub object_id: String,
    pub version: Option<i64>,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeRelation {
    pub from_type: String,
    pub from_id: String,
    pub to_type: String,
    pub to_id: String,
    pub relation_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeEvidence {
    pub source: String,
    pub object_type: String,
    pub object_id: String,
    pub retrieved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeLimits {
    pub max_objects: i64,
    pub max_tokens: i64,
    pub redaction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeTask {
    pub task_type: String,
    pub user_message: String,
}
