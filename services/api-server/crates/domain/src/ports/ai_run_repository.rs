//! Repository port for `ai_runs` persistence.

use async_trait::async_trait;
use serde_json::Value;

use crate::models::ai_job::AiRunRecord;

#[async_trait]
pub trait AiRunRepository: Send + Sync {
    async fn insert(
        &self,
        run_id: &str,
        job_id: &str,
        runtime_engine: &str,
        model_id: Option<&str>,
        input_envelope: Option<Value>,
    ) -> Result<AiRunRecord, AiRunRepositoryError>;

    async fn find_by_id(&self, run_id: &str) -> Result<Option<AiRunRecord>, AiRunRepositoryError>;

    async fn list_for_job(&self, job_id: &str) -> Result<Vec<AiRunRecord>, AiRunRepositoryError>;

    async fn update_status(&self, run_id: &str, new_status: &str) -> Result<AiRunRecord, AiRunRepositoryError>;

    async fn update_input_envelope(&self, run_id: &str, input_envelope: Value) -> Result<(), AiRunRepositoryError>;

    /// Fill output fields when the run is already terminal but outputs are NULL.
    async fn fill_terminal_outputs(
        &self,
        run_id: &str,
        output_raw: Option<Value>,
        output_validated: Option<Value>,
        token_usage: Option<Value>,
    ) -> Result<(), AiRunRepositoryError>;

    /// Atomic success: status=succeeded + outputs + finished_at.
    async fn complete(
        &self,
        run_id: &str,
        output_raw: Option<Value>,
        output_validated: Option<Value>,
        token_usage: Option<Value>,
    ) -> Result<(), AiRunRepositoryError>;

    /// Fill error fields when the run is already terminal but errors are NULL.
    async fn fill_terminal_error(
        &self,
        run_id: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
        output_raw: Option<Value>,
    ) -> Result<(), AiRunRepositoryError>;

    /// Atomic failure: status=failed_terminal + error + finished_at.
    async fn fail(
        &self,
        run_id: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
        output_raw: Option<Value>,
    ) -> Result<(), AiRunRepositoryError>;
}

#[derive(Debug, Clone)]
pub enum AiRunRepositoryError {
    NotFound(String),
    Database(String),
    Validation(String),
}

impl AiRunRepositoryError {
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound(id.into())
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::Database(message.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl std::fmt::Display for AiRunRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "ai run not found: {id}"),
            Self::Database(msg) => write!(f, "ai run database error: {msg}"),
            Self::Validation(msg) => write!(f, "ai run validation error: {msg}"),
        }
    }
}

impl std::error::Error for AiRunRepositoryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn trait_is_object_safe() {
        fn assert_object_safe(_: &dyn AiRunRepository) {}

        struct Stub;
        #[async_trait]
        impl AiRunRepository for Stub {
            async fn insert(
                &self,
                run_id: &str,
                job_id: &str,
                runtime_engine: &str,
                _model_id: Option<&str>,
                _input_envelope: Option<Value>,
            ) -> Result<AiRunRecord, AiRunRepositoryError> {
                Ok(AiRunRecord {
                    run_id: run_id.into(),
                    job_id: job_id.into(),
                    runtime_engine: runtime_engine.into(),
                    model_id: None,
                    status: "pending".into(),
                    input_envelope: None,
                    output_raw: None,
                    output_validated: None,
                    token_usage: None,
                    started_at: None,
                    finished_at: None,
                    error_code: None,
                    error_message: None,
                    created_at: Utc::now(),
                })
            }
            async fn find_by_id(&self, _run_id: &str) -> Result<Option<AiRunRecord>, AiRunRepositoryError> {
                Ok(None)
            }
            async fn list_for_job(&self, _job_id: &str) -> Result<Vec<AiRunRecord>, AiRunRepositoryError> {
                Ok(vec![])
            }
            async fn update_status(&self, run_id: &str, new_status: &str) -> Result<AiRunRecord, AiRunRepositoryError> {
                Ok(AiRunRecord {
                    run_id: run_id.into(),
                    job_id: "j".into(),
                    runtime_engine: "e".into(),
                    model_id: None,
                    status: new_status.into(),
                    input_envelope: None,
                    output_raw: None,
                    output_validated: None,
                    token_usage: None,
                    started_at: None,
                    finished_at: None,
                    error_code: None,
                    error_message: None,
                    created_at: Utc::now(),
                })
            }
            async fn update_input_envelope(
                &self,
                _run_id: &str,
                _input_envelope: Value,
            ) -> Result<(), AiRunRepositoryError> {
                Ok(())
            }
            async fn fill_terminal_outputs(
                &self,
                _run_id: &str,
                _output_raw: Option<Value>,
                _output_validated: Option<Value>,
                _token_usage: Option<Value>,
            ) -> Result<(), AiRunRepositoryError> {
                Ok(())
            }
            async fn complete(
                &self,
                _run_id: &str,
                _output_raw: Option<Value>,
                _output_validated: Option<Value>,
                _token_usage: Option<Value>,
            ) -> Result<(), AiRunRepositoryError> {
                Ok(())
            }
            async fn fill_terminal_error(
                &self,
                _run_id: &str,
                _error_code: Option<&str>,
                _error_message: Option<&str>,
                _output_raw: Option<Value>,
            ) -> Result<(), AiRunRepositoryError> {
                Ok(())
            }
            async fn fail(
                &self,
                _run_id: &str,
                _error_code: Option<&str>,
                _error_message: Option<&str>,
                _output_raw: Option<Value>,
            ) -> Result<(), AiRunRepositoryError> {
                Ok(())
            }
        }

        assert_object_safe(&Stub);
    }
}
