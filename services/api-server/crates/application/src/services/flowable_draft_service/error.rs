#[derive(Debug, thiserror::Error)]
pub enum FlowableDraftServiceError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{message}")]
    ProcessDocument {
        status_code: u16,
        code: String,
        message: String,
    },
    #[error("{0}")]
    AIUnavailable(String),
    #[error("{message}")]
    BpmnDraftValidation { code: String, message: String },
}
