//! Role-centered dashboard workbench aggregation service.

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::Value;

use crate::schemas::auth_schemas::TokenData;
use crate::schemas::dashboard_workbench_schemas::{
    DashboardAttentionItem, DashboardDegradedSource, DashboardModuleStatus, DashboardQuickLink, DashboardRecentChange,
    DashboardRiskFlightRef, DashboardRiskSummary, DashboardStaleDataIndicator, DashboardUserContext,
    DashboardWorkbenchResponse,
};
use crate::services::anomaly_service::AnomalyResponse;
use crate::services::system_ops_service::SystemOpsService;
use crate::types::{ConcreteAnomalyService, ConcreteDispatchQueryService, ConcreteTodoService};

pub struct DashboardWorkbenchService {
    todo_service: Option<Arc<ConcreteTodoService>>,
    anomaly_service: Option<Arc<ConcreteAnomalyService>>,
    dispatch_query_service: Option<Arc<ConcreteDispatchQueryService>>,
    system_ops_service: Option<Arc<SystemOpsService>>,
}

impl DashboardWorkbenchService {
    pub fn new(
        todo_service: Option<Arc<ConcreteTodoService>>,
        anomaly_service: Option<Arc<ConcreteAnomalyService>>,
        dispatch_query_service: Option<Arc<ConcreteDispatchQueryService>>,
        system_ops_service: Option<Arc<SystemOpsService>>,
    ) -> Self {
        Self {
            todo_service,
            anomaly_service,
            dispatch_query_service,
            system_ops_service,
        }
    }

    pub async fn build_workbench(&self, claims: &TokenData) -> DashboardWorkbenchResponse {
        let generated_at = Utc::now();
        let user_context = build_user_context(claims);
        let role_hint = role_hint(&user_context);
        let mut degraded_sources = Vec::new();
        let mut recent_changes = Vec::new();

        let attention_items = self
            .load_attention_items(&user_context.user_id, &mut degraded_sources)
            .await;
        recent_changes.extend(attention_items.iter().take(4).map(|item| DashboardRecentChange {
            id: item.id.clone(),
            title: item.title.clone(),
            source: item.source.clone(),
            changed_at: item.updated_at.unwrap_or(generated_at),
            severity: Some(item.priority.clone()),
            entity_id: item.source_id.clone(),
        }));

        let (
            (unresolved_anomalies, high_risk_refs, anomaly_degraded, anomaly_changes),
            (dispatch_conflict_refs, dispatch_degraded),
            (stale_data_indicators, stale_degraded),
        ) = tokio::join!(
            self.load_anomaly_risks_concurrent(),
            self.load_dispatch_conflicts_concurrent(),
            self.load_stale_data_indicators_concurrent(),
        );

        degraded_sources.extend(anomaly_degraded);
        degraded_sources.extend(dispatch_degraded);
        degraded_sources.extend(stale_degraded);
        recent_changes.extend(anomaly_changes);

        if recent_changes.is_empty() {
            degraded_sources.push(DashboardDegradedSource {
                source: "recent_changes".to_string(),
                reason: "No dedicated recent event feed is wired; synthesized sources returned no items".to_string(),
            });
        }

        DashboardWorkbenchResponse {
            generated_at,
            user_context,
            role_hint,
            attention_items,
            risk_summary: DashboardRiskSummary {
                unresolved_anomalies,
                high_risk_flights: high_risk_refs.len() as i64,
                dispatch_conflicts: dispatch_conflict_refs.len() as i64,
                stale_data_indicators,
                high_risk_flight_refs: high_risk_refs,
                dispatch_conflict_refs,
            },
            recent_changes,
            quick_links: default_quick_links(),
            module_status: default_module_status(),
            degraded_sources,
        }
    }

    async fn load_attention_items(
        &self,
        user_id: &str,
        degraded_sources: &mut Vec<DashboardDegradedSource>,
    ) -> Vec<DashboardAttentionItem> {
        let Some(todo_service) = &self.todo_service else {
            degraded_sources.push(DashboardDegradedSource {
                source: "todo_service".to_string(),
                reason: "Todo service is not registered for dashboard aggregation".to_string(),
            });
            return Vec::new();
        };

        match todo_service
            .list_todos(
                Some("pending"),
                None,
                None,
                Some(user_id),
                None,
                None,
                None,
                None,
                None,
                1,
                8,
            )
            .await
        {
            Ok(result) => result
                .items
                .into_iter()
                .map(|todo| DashboardAttentionItem {
                    id: todo.id.clone(),
                    title: todo.title,
                    priority: todo.priority,
                    status: todo.status,
                    source: "todo".to_string(),
                    source_id: Some(todo.id),
                    owner_id: todo.assigned_to,
                    due_at: todo.due_date,
                    updated_at: Some(todo.updated_at),
                    recommended_action: "review_todo".to_string(),
                })
                .collect(),
            Err(error) => {
                degraded_sources.push(DashboardDegradedSource {
                    source: "todo_service".to_string(),
                    reason: error.to_string(),
                });
                Vec::new()
            }
        }
    }

    #[allow(dead_code)]
    async fn load_anomaly_risks(
        &self,
        degraded_sources: &mut Vec<DashboardDegradedSource>,
        recent_changes: &mut Vec<DashboardRecentChange>,
    ) -> (i64, Vec<DashboardRiskFlightRef>) {
        let Some(anomaly_service) = &self.anomaly_service else {
            degraded_sources.push(DashboardDegradedSource {
                source: "anomaly_service".to_string(),
                reason: "Anomaly service is not registered for dashboard aggregation".to_string(),
            });
            return (0, Vec::new());
        };

        let stats = match anomaly_service.get_stats(None, None).await {
            Ok(stats) => stats,
            Err(error) => {
                degraded_sources.push(DashboardDegradedSource {
                    source: "anomaly_stats".to_string(),
                    reason: error.to_string(),
                });
                return (0, Vec::new());
            }
        };

        let mut unresolved = Vec::new();
        for status in ["open", "acknowledged"] {
            match anomaly_service
                .list_anomalies(Some(status), None, None, None, 50, 0)
                .await
            {
                Ok(items) => unresolved.extend(items),
                Err(error) => degraded_sources.push(DashboardDegradedSource {
                    source: format!("anomalies_{status}"),
                    reason: error.to_string(),
                }),
            }
        }

        unresolved.sort_by(|left, right| right.detected_at.cmp(&left.detected_at));
        recent_changes.extend(unresolved.iter().take(4).map(anomaly_recent_change));

        let high_risk_refs = unresolved
            .into_iter()
            .filter(is_high_risk_anomaly)
            .take(8)
            .map(|item| DashboardRiskFlightRef {
                flight_id: item.flight_id,
                anomaly_id: item.anomaly_id,
                severity: item.severity,
                title: item.title,
                detected_at: item.detected_at,
            })
            .collect();

        (stats.open + stats.acknowledged, high_risk_refs)
    }

    #[allow(dead_code)]
    async fn load_dispatch_conflicts(&self, degraded_sources: &mut Vec<DashboardDegradedSource>) -> Vec<Value> {
        let Some(dispatch_query_service) = &self.dispatch_query_service else {
            degraded_sources.push(DashboardDegradedSource {
                source: "dispatch_conflicts".to_string(),
                reason: "Dispatch query service is not registered for dashboard aggregation".to_string(),
            });
            return Vec::new();
        };

        let now = Utc::now();
        match dispatch_query_service
            .list_conflicts(now - Duration::hours(1), now + Duration::hours(8), 20)
            .await
        {
            Ok(conflicts) => conflicts,
            Err(error) => {
                degraded_sources.push(DashboardDegradedSource {
                    source: "dispatch_conflicts".to_string(),
                    reason: error.to_string(),
                });
                Vec::new()
            }
        }
    }

    #[allow(dead_code)]
    async fn load_stale_data_indicators(
        &self,
        degraded_sources: &mut Vec<DashboardDegradedSource>,
    ) -> Vec<DashboardStaleDataIndicator> {
        let Some(system_ops_service) = &self.system_ops_service else {
            degraded_sources.push(DashboardDegradedSource {
                source: "system_health".to_string(),
                reason: "System operations service is not registered for dashboard aggregation".to_string(),
            });
            return Vec::new();
        };

        match system_ops_service.get_public_health().await {
            Ok(payload) => {
                let status = payload
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if status == "healthy" {
                    Vec::new()
                } else {
                    vec![DashboardStaleDataIndicator {
                        source: "system_health".to_string(),
                        state: status,
                        detail: "Runtime health is not fully healthy; dashboard source data may be stale".to_string(),
                        observed_at: Utc::now(),
                    }]
                }
            }
            Err(error) => {
                degraded_sources.push(DashboardDegradedSource {
                    source: "system_health".to_string(),
                    reason: error.to_string(),
                });
                vec![DashboardStaleDataIndicator {
                    source: "system_health".to_string(),
                    state: "unknown".to_string(),
                    detail: "System health source unavailable".to_string(),
                    observed_at: Utc::now(),
                }]
            }
        }
    }

    async fn load_anomaly_risks_concurrent(
        &self,
    ) -> (
        i64,
        Vec<DashboardRiskFlightRef>,
        Vec<DashboardDegradedSource>,
        Vec<DashboardRecentChange>,
    ) {
        let mut degraded = Vec::new();
        let mut recent_changes = Vec::new();

        let Some(anomaly_service) = &self.anomaly_service else {
            degraded.push(DashboardDegradedSource {
                source: "anomaly_service".to_string(),
                reason: "Anomaly service is not registered for dashboard aggregation".to_string(),
            });
            return (0, Vec::new(), degraded, recent_changes);
        };

        let stats = match anomaly_service.get_stats(None, None).await {
            Ok(stats) => stats,
            Err(error) => {
                degraded.push(DashboardDegradedSource {
                    source: "anomaly_stats".to_string(),
                    reason: error.to_string(),
                });
                return (0, Vec::new(), degraded, recent_changes);
            }
        };

        let mut unresolved = Vec::new();
        for status in ["open", "acknowledged"] {
            match anomaly_service
                .list_anomalies(Some(status), None, None, None, 50, 0)
                .await
            {
                Ok(items) => unresolved.extend(items),
                Err(error) => degraded.push(DashboardDegradedSource {
                    source: format!("anomalies_{status}"),
                    reason: error.to_string(),
                }),
            }
        }

        unresolved.sort_by(|left, right| right.detected_at.cmp(&left.detected_at));
        recent_changes.extend(unresolved.iter().take(4).map(anomaly_recent_change));

        let high_risk_refs = unresolved
            .into_iter()
            .filter(is_high_risk_anomaly)
            .take(8)
            .map(|item| DashboardRiskFlightRef {
                flight_id: item.flight_id,
                anomaly_id: item.anomaly_id,
                severity: item.severity,
                title: item.title,
                detected_at: item.detected_at,
            })
            .collect();

        (
            stats.open + stats.acknowledged,
            high_risk_refs,
            degraded,
            recent_changes,
        )
    }

    async fn load_dispatch_conflicts_concurrent(&self) -> (Vec<Value>, Vec<DashboardDegradedSource>) {
        let mut degraded = Vec::new();

        let Some(dispatch_query_service) = &self.dispatch_query_service else {
            degraded.push(DashboardDegradedSource {
                source: "dispatch_conflicts".to_string(),
                reason: "Dispatch query service is not registered for dashboard aggregation".to_string(),
            });
            return (Vec::new(), degraded);
        };

        let now = Utc::now();
        match dispatch_query_service
            .list_conflicts(now - Duration::hours(1), now + Duration::hours(8), 20)
            .await
        {
            Ok(conflicts) => (conflicts, degraded),
            Err(error) => {
                degraded.push(DashboardDegradedSource {
                    source: "dispatch_conflicts".to_string(),
                    reason: error.to_string(),
                });
                (Vec::new(), degraded)
            }
        }
    }

    async fn load_stale_data_indicators_concurrent(
        &self,
    ) -> (Vec<DashboardStaleDataIndicator>, Vec<DashboardDegradedSource>) {
        let mut degraded = Vec::new();

        let Some(system_ops_service) = &self.system_ops_service else {
            degraded.push(DashboardDegradedSource {
                source: "system_health".to_string(),
                reason: "System operations service is not registered for dashboard aggregation".to_string(),
            });
            return (Vec::new(), degraded);
        };

        match system_ops_service.get_public_health().await {
            Ok(payload) => {
                let status = payload
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if status == "healthy" {
                    (Vec::new(), degraded)
                } else {
                    (
                        vec![DashboardStaleDataIndicator {
                            source: "system_health".to_string(),
                            state: status,
                            detail: "Runtime health is not fully healthy; dashboard source data may be stale"
                                .to_string(),
                            observed_at: Utc::now(),
                        }],
                        degraded,
                    )
                }
            }
            Err(error) => {
                degraded.push(DashboardDegradedSource {
                    source: "system_health".to_string(),
                    reason: error.to_string(),
                });
                (
                    vec![DashboardStaleDataIndicator {
                        source: "system_health".to_string(),
                        state: "unknown".to_string(),
                        detail: "System health source unavailable".to_string(),
                        observed_at: Utc::now(),
                    }],
                    degraded,
                )
            }
        }
    }
}

fn build_user_context(claims: &TokenData) -> DashboardUserContext {
    DashboardUserContext {
        user_id: claims
            .sub
            .as_deref()
            .or(claims.username.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("current_user")
            .to_string(),
        username: claims.username.clone(),
        department: claims.department.clone().or_else(|| claims.department_id.clone()),
        is_admin: claims.is_admin.unwrap_or(false),
        permissions: claims.permissions.clone(),
    }
}

fn role_hint(user_context: &DashboardUserContext) -> String {
    if user_context.is_admin {
        return "operations_admin".to_string();
    }
    if has_permission(user_context, "dispatch:view") || has_permission(user_context, "dispatch:*") {
        return "dispatcher".to_string();
    }
    if has_permission(user_context, "flight:read") {
        return "flight_operator".to_string();
    }
    "operator".to_string()
}

fn has_permission(user_context: &DashboardUserContext, permission: &str) -> bool {
    user_context
        .permissions
        .iter()
        .any(|item| item == "*" || item == permission)
}

fn anomaly_recent_change(item: &AnomalyResponse) -> DashboardRecentChange {
    DashboardRecentChange {
        id: item.anomaly_id.clone(),
        title: item.title.clone(),
        source: "anomaly".to_string(),
        changed_at: item.detected_at,
        severity: Some(item.severity.clone()),
        entity_id: Some(item.flight_id.clone()),
    }
}

fn is_high_risk_anomaly(item: &AnomalyResponse) -> bool {
    matches!(
        item.severity.to_ascii_lowercase().as_str(),
        "high" | "critical" | "严重" | "高"
    ) || item.escalation_level > 0
}

fn default_quick_links() -> Vec<DashboardQuickLink> {
    vec![
        DashboardQuickLink {
            id: "flight_monitor".to_string(),
            label: "Flight monitor".to_string(),
            href: "/frontend/flight_monitor.html".to_string(),
            module: "flight_monitor".to_string(),
            intent: "inspect_flight_risk".to_string(),
        },
        DashboardQuickLink {
            id: "dispatch_board".to_string(),
            label: "Dispatch board".to_string(),
            href: "/frontend/dispatch_board.html".to_string(),
            module: "dispatch".to_string(),
            intent: "resolve_dispatch_work".to_string(),
        },
        DashboardQuickLink {
            id: "system_status".to_string(),
            label: "System status".to_string(),
            href: "/frontend/system_status.html".to_string(),
            module: "system".to_string(),
            intent: "inspect_runtime_health".to_string(),
        },
        DashboardQuickLink {
            id: "shift_handover".to_string(),
            label: "Shift handover".to_string(),
            href: "/frontend/operations_review_report.html".to_string(),
            module: "handover".to_string(),
            intent: "review_operational_closure".to_string(),
        },
    ]
}

fn default_module_status() -> Vec<DashboardModuleStatus> {
    vec![
        DashboardModuleStatus {
            module: "flight_monitor".to_string(),
            status: "available".to_string(),
            detail: "Rust flight monitoring routes are mounted".to_string(),
        },
        DashboardModuleStatus {
            module: "dispatch".to_string(),
            status: "available".to_string(),
            detail: "Rust dispatch routes are mounted".to_string(),
        },
        DashboardModuleStatus {
            module: "anomalies".to_string(),
            status: "available".to_string(),
            detail: "Rust anomaly routes are mounted".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{build_user_context, role_hint};
    use crate::schemas::auth_schemas::TokenData;

    #[test]
    fn role_hint_prefers_admin() {
        let claims = TokenData {
            sub: Some("u1".to_string()),
            username: None,
            email: None,
            token_kind: Some("access".to_string()),
            is_admin: Some(true),
            permissions: vec!["flight:read".to_string()],
            department: None,
            department_id: None,
            pv: None,
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        };

        assert_eq!(role_hint(&build_user_context(&claims)), "operations_admin");
    }

    #[test]
    fn role_hint_uses_safe_operator_default() {
        let claims = TokenData {
            sub: None,
            username: None,
            email: None,
            token_kind: Some("access".to_string()),
            is_admin: None,
            permissions: vec![],
            department: None,
            department_id: None,
            pv: None,
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        };

        let context = build_user_context(&claims);

        assert_eq!(context.user_id, "current_user");
        assert_eq!(role_hint(&context), "operator");
    }
}
