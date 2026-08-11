pub const RUNTIME_STARTED: &str = "runtime_started";
pub const RUNTIME_COMPLETED: &str = "runtime_completed";
pub const PROVIDER_STREAM_STARTED: &str = "provider_stream_started";
pub const FIRST_TOKEN_EMITTED: &str = "first_token_emitted";
pub const PROVIDER_STREAM_COMPLETED: &str = "provider_stream_completed";
pub const PROVIDER_STREAM_ABORTED: &str = "provider_stream_aborted";
pub const FINALIZATION_FAILED_TRANSPORT_ERROR: &str = "finalization_failed_transport_error";
pub const FINALIZATION_FAILED_MISSING_TERMINAL: &str = "finalization_failed_missing_terminal";
pub const PROPOSAL_INGEST_STARTED: &str = "proposal_ingest_started";
pub const PROPOSAL_INGEST_SUCCEEDED: &str = "proposal_ingest_succeeded";
pub const PROPOSAL_INGEST_FAILED: &str = "proposal_ingest_failed";
pub const GRAPH_ORCHESTRATION_STARTED: &str = "graph_orchestration_started";
pub const GRAPH_ORCHESTRATION_FALLBACK: &str = "graph_orchestration_fallback";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_used() {
        assert_eq!(RUNTIME_STARTED, "runtime_started");
        assert_eq!(RUNTIME_COMPLETED, "runtime_completed");
        assert_eq!(PROVIDER_STREAM_STARTED, "provider_stream_started");
        assert_eq!(FIRST_TOKEN_EMITTED, "first_token_emitted");
        assert_eq!(PROVIDER_STREAM_COMPLETED, "provider_stream_completed");
        assert_eq!(PROVIDER_STREAM_ABORTED, "provider_stream_aborted");
        assert_eq!(
            FINALIZATION_FAILED_TRANSPORT_ERROR,
            "finalization_failed_transport_error"
        );
        assert_eq!(
            FINALIZATION_FAILED_MISSING_TERMINAL,
            "finalization_failed_missing_terminal"
        );
        assert_eq!(PROPOSAL_INGEST_STARTED, "proposal_ingest_started");
        assert_eq!(PROPOSAL_INGEST_SUCCEEDED, "proposal_ingest_succeeded");
        assert_eq!(PROPOSAL_INGEST_FAILED, "proposal_ingest_failed");
        assert_eq!(GRAPH_ORCHESTRATION_STARTED, "graph_orchestration_started");
        assert_eq!(GRAPH_ORCHESTRATION_FALLBACK, "graph_orchestration_fallback");
    }
}
