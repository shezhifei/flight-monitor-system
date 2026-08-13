//! API 路由注册

pub mod ai;
pub mod ai_config_v2;
pub mod ai_copilot;
pub mod ai_eval;
pub mod ai_execution_readiness;
pub mod ai_internal;
pub mod ai_jobs;
pub mod ai_media;
pub mod ai_micro_models;
pub mod ai_ontology;
pub mod ai_proposals;
pub mod ai_realtime_audio;
pub mod ai_resume;
pub mod ai_rollback;
pub mod ai_sidecar_dependency;
pub mod anomalies;
pub mod archive;
pub mod auth;
pub mod auth_admin;
pub mod business_case_types;
pub mod business_case_workflows;
pub mod business_cases;
pub mod dashboard;
pub mod dispatch;
pub mod dispatch_chat;
pub mod dispatch_collaboration;
pub mod dispatch_resources;
pub mod event_rules;
pub mod flights;
pub mod flowable;
pub mod health;
pub mod kpi;
pub mod labels;
pub mod metrics;
pub mod mobile;
pub mod nl_query;
pub mod notifications;
pub mod ontology;
pub mod ping;
pub mod reference;
pub mod resource_utilization;
pub mod scheduler;
pub mod shift_handovers;
pub mod static_files;
pub mod system;
pub mod todos;
mod workflow_actor;
pub mod workflow_dispatch;
pub mod workflow_forms;

#[cfg(test)]
mod tests {
    use actix_web::{http::StatusCode, test, web, App};
    use serde::Deserialize;
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::middleware::jwt::JwtSecret;

    #[derive(Debug, Deserialize)]
    struct PythonRouteManifestEntry {
        path: String,
        methods: Vec<String>,
    }

    fn build_smoke_test_app() -> App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::health::configure)
            .configure(super::auth_admin::configure)
            .configure(super::auth::configure)
            .configure(super::ai_eval::configure)
            .configure(super::nl_query::configure)
            .configure(super::system::configure)
            // ai_config_v2 routes are composed into super::ai's single /api/v2/ai
            // scope (registering it separately would shadow ai's routes).
            .configure(super::ai_ontology::configure)
            .configure(super::ai_proposals::configure)
            .configure(super::ai_media::configure)
            .configure(super::ai_copilot::configure)
            .configure(super::ai_realtime_audio::configure)
            .configure(super::ai_micro_models::configure)
            .configure(super::ai::configure)
            .configure(super::anomalies::configure)
            .configure(super::flights::configure)
    }

    fn build_full_route_order_app() -> App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::health::configure)
            .configure(super::auth_admin::configure)
            .configure(super::auth::configure)
            .configure(super::flights::configure)
            .configure(super::todos::configure)
            .configure(super::dispatch::configure)
            .configure(super::dispatch_resources::configure)
            .configure(super::dispatch_resources::configure_dispatch_direct_routes)
            .configure(super::event_rules::configure_event_rules_routes)
            .configure(super::dispatch_collaboration::configure)
            .configure(super::dispatch_chat::configure)
            .configure(super::notifications::configure)
            .configure(super::business_cases::configure)
            .configure(super::dashboard::configure)
            .configure(super::business_case_types::configure)
            .configure(super::reference::configure)
            .configure(super::anomalies::configure)
            .configure(super::kpi::configure)
            .configure(super::labels::configure)
            .configure(super::ontology::configure)
            .configure(super::shift_handovers::configure)
            .configure(super::scheduler::configure)
            .configure(super::system::configure)
            .configure(super::archive::configure)
            .configure(super::mobile::configure)
            .configure(super::resource_utilization::configure)
            .configure(super::workflow_dispatch::configure)
            .configure(super::ai_eval::configure)
            .configure(super::nl_query::configure)
            // ai_config_v2 routes are composed into super::ai's single /api/v2/ai
            // scope (registering it separately would shadow ai's routes).
            .configure(super::ai_ontology::configure)
            .configure(super::ai_proposals::configure)
            .configure(super::ai_media::configure)
            .configure(super::ai_copilot::configure)
            .configure(super::ai_realtime_audio::configure)
            .configure(super::ai_micro_models::configure)
            .configure(super::ai::configure)
            .configure(crate::sse::handler::configure)
            .configure(super::flowable::configure)
            .configure(super::workflow_forms::configure)
            .configure(super::static_files::configure)
    }

    fn build_until_dispatch_collaboration_then_system_app() -> App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::health::configure)
            .configure(super::auth_admin::configure)
            .configure(super::auth::configure)
            .configure(super::flights::configure)
            .configure(super::todos::configure)
            .configure(super::dispatch::configure)
            .configure(super::dispatch_resources::configure)
            .configure(super::dispatch_resources::configure_dispatch_direct_routes)
            .configure(super::dispatch_collaboration::configure)
            .configure(super::system::configure)
    }

    fn build_until_notifications_then_system_app() -> App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        App::new()
            .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
            .configure(super::health::configure)
            .configure(super::auth_admin::configure)
            .configure(super::auth::configure)
            .configure(super::flights::configure)
            .configure(super::todos::configure)
            .configure(super::dispatch::configure)
            .configure(super::dispatch_resources::configure)
            .configure(super::dispatch_resources::configure_dispatch_direct_routes)
            .configure(super::dispatch_collaboration::configure)
            .configure(super::notifications::configure)
            .configure(super::system::configure)
    }

    #[actix_web::test]
    async fn python_compatible_v2_paths_are_mounted() {
        let app = test::init_service(build_smoke_test_app()).await;

        let cases = [
            "/api/v2/auth/admin/permission-templates",
            "/api/v2/health",
            "/api/v2/system/airport-context",
            "/api/v2/ai/capabilities",
            "/api/v2/ai/eval/jobs?limit=1",
            "/api/v2/ai/nl-query/suggestions",
            "/api/v2/ai/copilot/business-case-operational-metrics",
            "/api/v2/ai/entities/default/capabilities",
            "/api/v2/ai/entities/default/mcp/servers",
            "/api/v2/ai/skills",
            "/api/v2/ai/cache/metrics?hours=24",
            "/api/v2/anomalies",
            "/api/v2/flights?page=1&page_size=1",
        ];

        for path in cases {
            let response = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected route to be mounted: {path}"
            );
        }
    }

    #[actix_web::test]
    async fn full_route_order_keeps_late_v2_modules_mounted() {
        let app = test::init_service(build_full_route_order_app()).await;

        let cases = [
            "/api/v2/reference/departments",
            "/api/v2/reference/departments/test-id",
            "/api/v2/reference/business-case-types",
            "/api/v2/dispatch-orders",
            "/api/v2/dispatch/collaboration/groups",
            "/api/v2/dispatch/schedule/templates",
            "/api/v2/dispatch/analytics/summary",
            "/api/v2/dispatch/task-types",
            "/api/v2/dispatch/team-types",
            "/api/v2/dispatch/teams",
            "/api/v2/dispatch/equipment-types",
            "/api/v2/dispatch/equipment",
            "/api/v2/dispatch-orders/my/assigned",
            "/api/v2/dispatch-orders/test-order",
            "/api/v2/dispatch-orders/test-order/timeline",
            "/api/v2/dispatch-orders/timeline",
            "/api/v2/dispatch-orders/conflicts",
            "/api/v2/dispatch-orders/cascade-preview?flight_id=test&task_type=test&delay_minutes=5",
            "/api/v2/dispatch-orders/followup-queue",
            "/api/v2/dispatch-orders/burden-metrics",
            "/api/v2/dispatch/collaboration/orders/test-order",
            "/api/v2/notifications/sent-receipt-groups?limit=1",
            "/api/v2/dispatch/collaboration/groups",
            "/api/v2/system/airport-context",
            "/api/v2/labels?active_only=true",
            "/api/v2/system/runtime/streaming/buffer-status",
            "/api/v2/system/runtime/streaming/sse-stats",
            "/api/v2/system/scheduler/status",
            "/api/v2/system/scheduler/flight-sync/status",
            "/api/v2/anomalies",
            "/api/v2/shift-handovers?limit=1",
            "/api/v2/mobile/workbench?pending_sync_action_count=0&max_orders=1",
            "/api/v2/workflows/definitions",
            "/api/v2/workflows/instances",
            "/api/v2/workflows/history/instances",
            "/api/v2/ai/capabilities",
            "/api/v2/ai/eval/jobs?limit=1",
            "/api/v2/ai/nl-query/suggestions",
            "/api/v2/ai/copilot/business-case-operational-metrics",
            "/api/v2/auth/admin/permission-templates",
            "/api/v2/ai/events/stream",
            "/api/v2/sse/stream",
        ];

        for path in cases {
            let response = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected route to remain mounted in full route order: {path}"
            );
        }
    }

    #[actix_web::test]
    async fn critical_python_route_surfaces_remain_mounted() {
        let app = test::init_service(build_full_route_order_app()).await;

        let get_cases = [
            "/api/v2/notifications/dispatch/online-users",
            "/api/v2/workflows/integrations/dispatch/pending?page=1&page_size=1",
            "/api/v2/workflows/integrations/dispatch/test-order/recommendations",
            "/api/v2/workflows/deployments",
            "/api/v2/workflows/history/tasks",
        ];

        for path in get_cases {
            let response = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected GET route to remain mounted: {path}"
            );
        }

        let post_cases = [
            "/api/v2/workflows/integrations/dispatch/trigger",
            "/api/v2/notifications/dispatch/send",
            "/api/v2/labels",
            "/api/v2/labels/flights/test-flight",
            "/api/v2/labels/flights/test-flight/legs/inbound",
            "/api/v2/workflows/definitions/drafts/assistant-chat",
            "/api/v2/workflows/definitions/drafts/assistant-chat/stream",
            "/api/v2/workflows/instances",
        ];

        for path in post_cases {
            let response = test::call_service(&app, test::TestRequest::post().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected POST route to remain mounted: {path}"
            );
        }

        let put_cases = ["/api/v2/business-case-types/test-case/bpmn"];

        for path in put_cases {
            let response = test::call_service(&app, test::TestRequest::put().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected PUT route to remain mounted: {path}"
            );
        }

        let removed_get_cases = [
            "/api/ai/v2/entities",
            "/api/v2/dispatch-collaboration/orders/test-order",
            "/api/v2/dispatch/orders",
        ];

        for path in removed_get_cases {
            let response = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected removed GET route to stay absent: {path}"
            );
        }
    }

    #[actix_web::test]
    async fn python_compatible_kpi_and_mobile_routes_remain_mounted() {
        let app = test::init_service(build_full_route_order_app()).await;

        let get_cases = [
            "/api/v2/kpi/snapshot",
            "/api/v2/kpi/trend",
            "/api/v2/kpi/service-nodes?date=2026-01-01",
            "/api/v2/kpi/compare?base_start_date=2026-01-01&base_end_date=2026-01-02&compare_start_date=2026-01-03&compare_end_date=2026-01-04",
            "/api/v2/kpi/trend-with-anomalies",
            "/api/v2/kpi/baseline-compare?date=2026-01-01",
            "/api/v2/dashboard/workbench",
            "/api/v2/mobile/workbench?pending_sync_action_count=0&max_orders=1",
            "/api/v2/mobile/operations/events?limit=1",
            "/api/v2/mobile/uploads/test/content",
        ];

        for path in get_cases {
            let response = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected GET route to remain mounted: {path}"
            );
        }

        let delete_cases = ["/api/v2/mobile/devices/test-device"];

        for path in delete_cases {
            let response = test::call_service(&app, test::TestRequest::delete().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected DELETE route to remain mounted: {path}"
            );
        }

        let post_cases = ["/api/v2/mobile/devices/test-device/heartbeat"];

        for path in post_cases {
            let response = test::call_service(&app, test::TestRequest::post().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected POST route to remain mounted: {path}"
            );
        }

        let additional_post_cases = ["/api/v2/mobile/uploads", "/api/v2/mobile/devices/register"];

        for path in additional_post_cases {
            let response = test::call_service(&app, test::TestRequest::post().uri(path).to_request()).await;
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected POST route to remain mounted: {path}"
            );
        }
    }

    #[actix_web::test]
    async fn route_order_until_dispatch_collaboration_still_keeps_system_path() {
        let app = test::init_service(build_until_dispatch_collaboration_then_system_app()).await;
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v2/system/airport-context")
                .to_request(),
        )
        .await;
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn route_order_until_notifications_still_keeps_system_path() {
        let app = test::init_service(build_until_notifications_then_system_app()).await;
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v2/system/airport-context")
                .to_request(),
        )
        .await;
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn removed_public_routes_are_absent() {
        let app = test::init_service(build_full_route_order_app()).await;

        let removed_get_cases = [
            "/api/v2/anomalies/stream",
            "/api/v2/anomalies/ws",
            "/api/v2/flights/stream",
            "/api/v2/flights/ws?access_token=test",
            "/api/v2/notifications/stream",
            "/api/v2/dispatch/collaboration/stream",
            "/api/v2/kpi/stream",
            "/api/v2/health/stream/status",
            "/api/v2/sse/stats",
            "/api/v2/archived/flights",
            "/case-types",
            "/api/v1/case-types",
            "/api/v2/business_cases",
            "/api/v2/resources/utilization/summary",
            "/api/v2/workflow-dispatch/pending?page=1&page_size=1",
            "/api/v2/workflows/process-drafts/assistant-chat",
            "/api/ai/v2/entities",
            "/api/v2/ai/v2/entities",
            "/api/v2/ai/v2/entities/default/status",
            "/api/v2/ai/v2/ws/user-1",
            "/api/v2/shadow/diffs",
            "/api/v2/shadow/stats",
            "/api/v2/shadow/health",
            "/api/v2/verification/stats",
            "/api/v2/verification/health",
            "/api/ping",
            "/api/v2/ping",
            "/api/v2/business-case-workflows/runs/run-1",
            "/api/v2/business_cases/case-1/workflow",
            // Task 15: ai_proxy module is dead code (never mounted in any
            // composition root, routes lack ensure_permission). These paths
            // must remain absent to prevent accidental future mounting.
            "/api/v2/ai-proxy/health",
            "/api/v2/ai-proxy/generate_plan",
            "/api/v2/ai-proxy/nl-query/suggestions",
            "/api/v2/ai-proxy/events/stream",
        ];

        for path in removed_get_cases {
            let response = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected removed public route to stay absent: {path}"
            );
        }

        let removed_post_cases = ["/api/v2/ai/v2/batch/default", "/api/v2/verification/compare"];

        for path in removed_post_cases {
            let response = test::call_service(&app, test::TestRequest::post().uri(path).to_request()).await;
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "expected removed public route to stay absent: {path}"
            );
        }
    }

    #[actix_web::test]
    async fn route_order_until_business_cases_still_keeps_system_path() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtSecret("test-secret".to_string())))
                .configure(super::health::configure)
                .configure(super::auth_admin::configure)
                .configure(super::auth::configure)
                .configure(super::flights::configure)
                .configure(super::todos::configure)
                .configure(super::dispatch::configure)
                .configure(super::dispatch_resources::configure)
                .configure(super::dispatch_resources::configure_dispatch_direct_routes)
                .configure(super::dispatch_collaboration::configure)
                .configure(super::notifications::configure)
                .configure(super::business_cases::configure)
                .configure(super::system::configure),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v2/system/airport-context")
                .to_request(),
        )
        .await;
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn python_canonical_manifest_routes_are_mounted_in_rust() {
        let manifest = load_python_canonical_manifest();
        let app = test::init_service(build_full_route_order_app()).await;

        let mut missing = Vec::new();
        for entry in manifest {
            let path = materialize_path(&entry.path);
            for method in entry.methods {
                let request = match method.as_str() {
                    "GET" => test::TestRequest::get().uri(&path).to_request(),
                    "POST" => test::TestRequest::post().uri(&path).set_json(json!({})).to_request(),
                    "PUT" => test::TestRequest::put().uri(&path).set_json(json!({})).to_request(),
                    "PATCH" => test::TestRequest::patch().uri(&path).set_json(json!({})).to_request(),
                    "DELETE" => test::TestRequest::delete().uri(&path).to_request(),
                    other => panic!("unsupported method in manifest: {other}"),
                };
                let response = test::call_service(&app, request).await;
                if matches!(
                    response.status(),
                    StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
                ) {
                    missing.push(format!("{method} {}", entry.path));
                }
            }
        }

        assert!(
            missing.is_empty(),
            "Rust public mounts are missing Python canonical routes:\n{}",
            missing.join("\n")
        );
    }

    fn load_python_canonical_manifest() -> Vec<PythonRouteManifestEntry> {
        let repo_root = repository_root();
        let python_exe = repo_root.join(".venv").join("Scripts").join("python.exe");
        let script = repo_root
            .join("scripts")
            .join("tools")
            .join("export_python_canonical_route_manifest.py");
        let output_path = temp_manifest_path();

        let output = Command::new(&python_exe)
            .arg(&script)
            .arg("--output")
            .arg(&output_path)
            .current_dir(&repo_root)
            .output()
            .unwrap_or_else(|error| panic!("failed to run python route manifest exporter: {error}"));

        assert!(
            output.status.success(),
            "python route manifest exporter failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let manifest_raw = fs::read_to_string(&output_path)
            .unwrap_or_else(|error| panic!("failed to read python route manifest: {error}"));
        let _ = fs::remove_file(&output_path);
        serde_json::from_str(&manifest_raw)
            .unwrap_or_else(|error| panic!("invalid python route manifest json: {error}"))
    }

    fn materialize_path(path: &str) -> String {
        let mut result = String::with_capacity(path.len());
        let mut chars = path.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                while let Some(inner) = chars.next() {
                    if inner == '}' {
                        break;
                    }
                }
                result.push_str("test-id");
            } else {
                result.push(ch);
            }
        }

        result
    }

    fn repository_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .and_then(Path::parent)
            .expect("repository root")
            .to_path_buf()
    }

    fn temp_manifest_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("python-route-manifest-{unique}.json"))
    }
}
