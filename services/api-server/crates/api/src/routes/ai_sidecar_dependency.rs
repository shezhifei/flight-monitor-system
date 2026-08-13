use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SidecarDependency {
    RustNative,
    PythonRequired,
    TemporarilyUnavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiRouteDescriptor {
    pub path: &'static str,
    pub method: &'static str,
    pub dependency: SidecarDependency,
    pub note: &'static str,
}

pub const AI_ROUTE_DEPENDENCIES: &[AiRouteDescriptor] = &[
    // ai.rs - AI 助手主路由
    AiRouteDescriptor { path: "/api/v2/ai/capabilities", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService" },
    AiRouteDescriptor { path: "/api/v2/ai/tools", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService" },
    AiRouteDescriptor { path: "/api/v2/ai/tools/execute", method: "POST", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService with pending action for side_effect=true" },
    AiRouteDescriptor { path: "/api/v2/ai/tools/categories", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService" },
    AiRouteDescriptor { path: "/api/v2/ai/pending-actions", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService" },
    AiRouteDescriptor { path: "/api/v2/ai/pending-actions/{action_id}/approve", method: "POST", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService" },
    AiRouteDescriptor { path: "/api/v2/ai/pending-actions/{action_id}/reject", method: "POST", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService" },
    AiRouteDescriptor { path: "/api/v2/ai/entities", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust AI entity config management" },
    AiRouteDescriptor { path: "/api/v2/ai/entities/{entity_id}", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust AI entity config management" },
    AiRouteDescriptor { path: "/api/v2/ai/connection/test", method: "POST", dependency: SidecarDependency::RustNative, note: "Rust AI connection test" },
    AiRouteDescriptor { path: "/api/v2/ai/models", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust AI model listing" },
    AiRouteDescriptor { path: "/api/v2/ai/todos/{todo_id}/execute", method: "POST", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService todo execution" },
    AiRouteDescriptor { path: "/api/v2/ai/rate-limit/status", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust rate limiter status" },
    AiRouteDescriptor { path: "/api/v2/ai/metrics/query-routing", method: "GET", dependency: SidecarDependency::RustNative, note: "Rust AiRuntimeService metrics" },
    AiRouteDescriptor { path: "/api/v2/ai/generate_plan", method: "POST", dependency: SidecarDependency::PythonRequired, note: "Rust handler forwards SSE JSON to sidecar runtime" },
    AiRouteDescriptor { path: "/api/v2/ai/events/stream", method: "GET", dependency: SidecarDependency::PythonRequired, note: "Rust handler forwards SSE to sidecar runtime" },

    // nl_query.rs - 自然语言查询
    //   POST /api/v2/ai/nl-query (非 streaming): Rust 治理 + job/run pipeline + Python AI Runtime
    //     - 同步完成策略：Rust 侧创建 ai_job/ai_run，调用 Python /internal/ai/v1/runs，
    //       解析返回 body 语义（success/status/error），proposal ingest 在同步路径中直接执行。
    //       不期待 Python 再 callback /runs/{run_id}/complete。
    //   suggestions/conversations: 仍为 deprecated raw proxy fallback
    AiRouteDescriptor { path: "/api/v2/ai/nl-query", method: "POST", dependency: SidecarDependency::PythonRequired, note: "Rust-governed: creates ai_job/ai_run, calls Python /internal/ai/v1/runs, completes synchronously with proposal ingest" },
    AiRouteDescriptor { path: "/api/v2/ai/nl-query/stream", method: "POST", dependency: SidecarDependency::PythonRequired, note: "Rust-governed streaming: creates ai_job/ai_run, calls Python /internal/ai/v1/runs/stream, finalizes in background" },
    AiRouteDescriptor { path: "/api/v2/ai/nl-query/stream-with-tools", method: "POST", dependency: SidecarDependency::PythonRequired, note: "Tool streaming; gated by AI_RUNTIME_ENABLE_TOOL_STREAMING" },
    AiRouteDescriptor { path: "/api/v2/ai/nl-query/suggestions", method: "GET", dependency: SidecarDependency::TemporarilyUnavailable, note: "Deprecated raw proxy fallback; not migrated to Rust pipeline" },
    AiRouteDescriptor { path: "/api/v2/ai/nl-query/conversations", method: "GET", dependency: SidecarDependency::TemporarilyUnavailable, note: "Deprecated raw proxy fallback; not migrated to Rust pipeline" },

    // ai_eval.rs - LLM 评估
    AiRouteDescriptor { path: "/api/v2/ai/eval/jobs", method: "POST", dependency: SidecarDependency::PythonRequired, note: "LLM eval must run in Python; needs internal AI Runtime API" },
    AiRouteDescriptor { path: "/api/v2/ai/eval/jobs", method: "GET", dependency: SidecarDependency::PythonRequired, note: "LLM eval must run in Python; needs internal AI Runtime API" },
    AiRouteDescriptor { path: "/api/v2/ai/eval/jobs/{job_id}", method: "GET", dependency: SidecarDependency::PythonRequired, note: "LLM eval must run in Python; needs internal AI Runtime API" },
    AiRouteDescriptor { path: "/api/v2/ai/eval/jobs/{job_id}/cancel", method: "POST", dependency: SidecarDependency::PythonRequired, note: "LLM eval must run in Python; needs internal AI Runtime API" },
    AiRouteDescriptor { path: "/api/v2/ai/eval/jobs/{job_id}/compare", method: "GET", dependency: SidecarDependency::PythonRequired, note: "LLM eval must run in Python; needs internal AI Runtime API" },

    // ai_proposals.rs - 动作建议 (Rust 原生)
    AiRouteDescriptor { path: "/api/v2/ai/proposals", method: "POST", dependency: SidecarDependency::RustNative, note: "AiActionProposalService" },
    AiRouteDescriptor { path: "/api/v2/ai/proposals", method: "GET", dependency: SidecarDependency::RustNative, note: "AiActionProposalService" },
    AiRouteDescriptor { path: "/api/v2/ai/proposals/stats", method: "GET", dependency: SidecarDependency::RustNative, note: "AiActionProposalService" },
    AiRouteDescriptor { path: "/api/v2/ai/proposals/{proposal_id}/approve", method: "POST", dependency: SidecarDependency::RustNative, note: "AiActionProposalService" },
    AiRouteDescriptor { path: "/api/v2/ai/proposals/{proposal_id}/reject", method: "POST", dependency: SidecarDependency::RustNative, note: "AiActionProposalService" },
    AiRouteDescriptor { path: "/api/v2/ai/proposals/{proposal_id}/execute", method: "POST", dependency: SidecarDependency::RustNative, note: "AiActionProposalService -> AiRuntimeService" },

    // ai_micro_models.rs - 微模型 (Rust 原生)
    AiRouteDescriptor { path: "/api/v2/ai/micro-models", method: "GET", dependency: SidecarDependency::RustNative, note: "MicroModelRegistry" },
    AiRouteDescriptor { path: "/api/v2/ai/micro-models/{model_id}", method: "GET", dependency: SidecarDependency::RustNative, note: "MicroModelRegistry" },
    AiRouteDescriptor { path: "/api/v2/ai/micro-models/{model_id}/execute", method: "POST", dependency: SidecarDependency::RustNative, note: "MicroModelExecutor (generic dispatcher, Rust deterministic)" },

    // ai_media.rs - ASR/TTS 语音媒体 (Rust 原生)
    AiRouteDescriptor { path: "/api/v2/ai/media/transcribe", method: "POST", dependency: SidecarDependency::RustNative, note: "AiMediaService ASR transcription" },
    AiRouteDescriptor { path: "/api/v2/ai/media/synthesize", method: "POST", dependency: SidecarDependency::RustNative, note: "AiMediaService TTS synthesis" },
    AiRouteDescriptor { path: "/api/v2/ai/media/capabilities", method: "GET", dependency: SidecarDependency::RustNative, note: "AiMediaService capability query" },
    AiRouteDescriptor { path: "/api/v2/ai/media/voices", method: "GET", dependency: SidecarDependency::RustNative, note: "AiMediaService TTS voice listing" },
    AiRouteDescriptor { path: "/api/v2/ai/media/formats", method: "GET", dependency: SidecarDependency::RustNative, note: "AiMediaService supported format listing" },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_dependent_routes_are_documented() {
        assert!(
            AI_ROUTE_DEPENDENCIES
                .iter()
                .any(|route| matches!(route.dependency, SidecarDependency::PythonRequired)),
            "at least one active route should require the sidecar"
        );
        assert!(
            AI_ROUTE_DEPENDENCIES
                .iter()
                .any(|route| matches!(route.dependency, SidecarDependency::TemporarilyUnavailable)),
            "temporarily unavailable routes should remain explicit"
        );
    }

    #[test]
    fn rust_native_routes_are_documented() {
        let rust_native: Vec<_> = AI_ROUTE_DEPENDENCIES
            .iter()
            .filter(|d| matches!(d.dependency, SidecarDependency::RustNative))
            .collect();
        assert!(!rust_native.is_empty(), "at least some routes should be Rust native");
        assert!(
            rust_native.len() >= 20,
            "expected at least 20 Rust-native routes, got {}",
            rust_native.len()
        );
    }

    #[test]
    fn all_routes_have_non_empty_note() {
        for desc in AI_ROUTE_DEPENDENCIES {
            assert!(
                !desc.note.is_empty(),
                "route {} {} has empty note",
                desc.method,
                desc.path
            );
        }
    }

    #[test]
    fn no_duplicate_path_method_pairs() {
        let mut seen = std::collections::HashSet::new();
        for desc in AI_ROUTE_DEPENDENCIES {
            let key = format!("{} {}", desc.method, desc.path);
            assert!(seen.insert(key), "duplicate route: {} {}", desc.method, desc.path);
        }
    }

    #[test]
    fn stream_with_tools_route_is_documented() {
        let found = AI_ROUTE_DEPENDENCIES
            .iter()
            .any(|d| d.path == "/api/v2/ai/nl-query/stream-with-tools");
        assert!(found, "stream-with-tools route must be in the dependency manifest");
    }
}

/// Read-only operational health contract for tool streaming.
///
/// Returns a JSON-serializable diagnostic summary. Does NOT:
/// - access business DB
/// - call OpenAI
/// - call Python sidecar
/// - leak JWT secrets or tokens
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolStreamingHealthContract {
    pub tool_streaming_feature_gate: String,
    pub public_path: &'static str,
    pub internal_path: &'static str,
    pub service_identity_audience: &'static str,
    pub service_identity_issuer: &'static str,
    pub write_tools_policy: &'static str,
    pub python_db_write_policy: &'static str,
}

pub fn tool_streaming_health_contract() -> ToolStreamingHealthContract {
    let gate = std::env::var("AI_RUNTIME_ENABLE_TOOL_STREAMING")
        .map(|v| {
            if v == "1" || v.eq_ignore_ascii_case("true") {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            }
        })
        .unwrap_or_else(|_| "disabled".to_string());

    ToolStreamingHealthContract {
        tool_streaming_feature_gate: gate,
        public_path: "/api/v2/ai/nl-query/stream-with-tools",
        internal_path: "/internal/ai/v1/runs/stream-with-tools",
        service_identity_audience: "python-ai-runtime",
        service_identity_issuer: "fms-rust-api",
        write_tools_policy: "proposals_only",
        python_db_write_policy: "forbidden",
    }
}

#[cfg(test)]
mod health_tests {
    use super::*;

    #[test]
    fn health_contract_default_disabled() {
        // Clear env to ensure default
        std::env::remove_var("AI_RUNTIME_ENABLE_TOOL_STREAMING");
        let contract = tool_streaming_health_contract();
        assert_eq!(contract.tool_streaming_feature_gate, "disabled");
        assert_eq!(contract.public_path, "/api/v2/ai/nl-query/stream-with-tools");
        assert_eq!(contract.internal_path, "/internal/ai/v1/runs/stream-with-tools");
        assert_eq!(contract.service_identity_audience, "python-ai-runtime");
        assert_eq!(contract.service_identity_issuer, "fms-rust-api");
        assert_eq!(contract.write_tools_policy, "proposals_only");
        assert_eq!(contract.python_db_write_policy, "forbidden");
    }

    #[test]
    fn health_contract_enabled() {
        std::env::set_var("AI_RUNTIME_ENABLE_TOOL_STREAMING", "1");
        let contract = tool_streaming_health_contract();
        assert_eq!(contract.tool_streaming_feature_gate, "enabled");
        // Cleanup
        std::env::remove_var("AI_RUNTIME_ENABLE_TOOL_STREAMING");
    }

    #[test]
    fn health_contract_does_not_leak_secrets() {
        let contract = tool_streaming_health_contract();
        let serialized = serde_json::to_string(&contract).unwrap();
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("jwt"));
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("Bearer"));
        assert!(!serialized.contains("sk-"));
    }

    #[test]
    fn health_contract_serializable() {
        let contract = tool_streaming_health_contract();
        let json = serde_json::to_value(&contract).unwrap();
        assert!(json.is_object());
        assert_eq!(json["write_tools_policy"], "proposals_only");
    }
}
