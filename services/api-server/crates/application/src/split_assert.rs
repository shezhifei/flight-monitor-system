//! TD-27 / TD-35 split assertions.
//!
//! Locks in two invariants after the large-file refactor:
//!  1. The 3 monolith packages (`domain_action_executor`, `nl_query_service`,
//!     `flight_runtime_service`) still re-export their pre-split public API at
//!     the same module paths — i.e. downstream `use` paths did not break.
//!  2. No refactored sub-file exceeds the 1500-line ceiling from the execution
//!     plan (`docs/plans/2026-06-29-td-27-28-execution.md`). Files are embedded
//!     at compile time via `include_str!`; line counts are checked at runtime.

#[cfg(test)]
mod split_assert {
    #![allow(clippy::module_inception)]
    // --- (1) Re-export reachability -----------------------------------------
    // These `use` declarations compile-check that each refactored package still
    // exposes its public types at the same path. `#[allow(unused_imports)]`
    // suppresses the unused-import warning while keeping the path resolution
    // check active.
    #[allow(unused_imports)]
    use crate::services::domain_action_executor::{DomainActionError, DomainActionExecutor, DomainActionReceipt};
    #[allow(unused_imports)]
    use crate::services::flight_runtime_service::{DispatchTimelineEventWriteResult, FlightRuntimeService};
    #[allow(unused_imports)]
    use crate::services::nl_query_service::{
        NLQueryRuntimeContext, NLQueryService, NLQueryServiceError, NLQueryStreamEvent,
    };

    // --- (2) Line-count ceiling ---------------------------------------------
    // Plan acceptance: every refactored file < 1500 lines.
    const CEILING: usize = 1500;

    fn assert_under_ceiling(path: &str, content: &'static str) {
        let count = content.lines().count();
        assert!(count < CEILING, "{path} is {count} lines, exceeds ceiling of {CEILING}");
    }

    #[test]
    fn flight_runtime_service_files_under_ceiling() {
        assert_under_ceiling(
            "flight_runtime_service/service.rs",
            include_str!("services/flight_runtime_service/service.rs"),
        );
        assert_under_ceiling(
            "flight_runtime_service/history.rs",
            include_str!("services/flight_runtime_service/history.rs"),
        );
        assert_under_ceiling(
            "flight_runtime_service/timeline.rs",
            include_str!("services/flight_runtime_service/timeline.rs"),
        );
    }

    #[test]
    fn nl_query_service_files_under_ceiling() {
        assert_under_ceiling(
            "nl_query_service/analyze.rs",
            include_str!("services/nl_query_service/analyze.rs"),
        );
        assert_under_ceiling(
            "nl_query_service/service.rs",
            include_str!("services/nl_query_service/service.rs"),
        );
        assert_under_ceiling(
            "nl_query_service/helpers.rs",
            include_str!("services/nl_query_service/helpers.rs"),
        );
    }

    #[test]
    fn domain_action_executor_files_under_ceiling() {
        assert_under_ceiling(
            "domain_action_executor/service.rs",
            include_str!("services/domain_action_executor/service.rs"),
        );
        assert_under_ceiling(
            "domain_action_executor/tests.rs",
            include_str!("services/domain_action_executor/tests.rs"),
        );
    }

    #[test]
    fn workflow_helpers_split_files_under_ceiling() {
        assert_under_ceiling(
            "business_case_workflow_service/snapshots.rs",
            include_str!("services/business_case_workflow_service/snapshots.rs"),
        );
        assert_under_ceiling(
            "business_case_workflow_service/templates.rs",
            include_str!("services/business_case_workflow_service/templates.rs"),
        );
        assert_under_ceiling(
            "business_case_workflow_service/service.rs",
            include_str!("services/business_case_workflow_service/service.rs"),
        );
    }

    #[test]
    fn replan_snapshot_split_files_under_ceiling() {
        assert_under_ceiling(
            "dispatch_frontend_replan_service/snapshot_anchors.rs",
            include_str!("services/dispatch_frontend_replan_service/snapshot_anchors.rs"),
        );
        assert_under_ceiling(
            "dispatch_frontend_replan_service/snapshot_slots.rs",
            include_str!("services/dispatch_frontend_replan_service/snapshot_slots.rs"),
        );
        assert_under_ceiling(
            "dispatch_frontend_replan_service/snapshot_build.rs",
            include_str!("services/dispatch_frontend_replan_service/snapshot_build.rs"),
        );
    }

    #[test]
    fn copilot_service_split_files_under_ceiling() {
        assert_under_ceiling(
            "ai_business_case_copilot_service/service.rs",
            include_str!("services/ai_business_case_copilot_service/service.rs"),
        );
        assert_under_ceiling(
            "ai_business_case_copilot_service/draft.rs",
            include_str!("services/ai_business_case_copilot_service/draft.rs"),
        );
        assert_under_ceiling(
            "ai_business_case_copilot_service/helpers.rs",
            include_str!("services/ai_business_case_copilot_service/helpers.rs"),
        );
    }
}
