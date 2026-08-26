use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use fms_domain::models::ai_ontology::OntologyActionDef;
use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayInput {
    pub proposal_id: String,
    pub ontology_version: Option<String>,
    pub object_type: String,
    pub object_id: String,
    pub action_name: String,
    pub arguments: Value,
    pub expected_object_version: Option<i64>,
    pub actor_permissions: Vec<String>,
    #[serde(default = "default_dry_run_true")]
    pub dry_run: bool,
}

fn default_dry_run_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplayMode {
    DryRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayReport {
    pub proposal_id: String,
    pub mode: ReplayMode,
    pub validation_passed: bool,
    pub executed: bool,
    pub checks: Vec<ReplayCheck>,
    pub would_execute: Option<WouldExecuteReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WouldExecuteReceipt {
    pub object_type: String,
    pub object_id: String,
    pub action_name: String,
    pub arguments: Value,
}

pub struct AiProposalReplayService {
    ontology_repository: Option<Arc<dyn AiOntologyRepository + Send + Sync>>,
}

impl AiProposalReplayService {
    pub fn new() -> Self {
        Self {
            ontology_repository: None,
        }
    }

    pub fn new_for_test(ontology: Arc<dyn AiOntologyRepository + Send + Sync>) -> Self {
        Self {
            ontology_repository: Some(ontology),
        }
    }

    pub fn with_ontology_repository(mut self, repo: Arc<dyn AiOntologyRepository + Send + Sync>) -> Self {
        self.ontology_repository = Some(repo);
        self
    }

    pub async fn replay(&self, input: ReplayInput) -> Result<ReplayReport, String> {
        let mut checks = Vec::new();

        let (ontology_check, action_def) = self.load_action_def(&input).await;
        checks.push(ontology_check);

        if let Some(action) = &action_def {
            checks.push(self.check_arguments(&input, action));
            checks.push(self.check_permissions(&input, action));
        } else {
            checks.push(ReplayCheck {
                name: "arguments".to_string(),
                passed: false,
                message: "skipped: ontology action not found".to_string(),
            });
            checks.push(ReplayCheck {
                name: "permissions".to_string(),
                passed: false,
                message: "skipped: ontology action not found".to_string(),
            });
        }

        let validation_passed = checks.iter().all(|c| c.passed);

        let would_execute = if validation_passed {
            Some(WouldExecuteReceipt {
                object_type: input.object_type.clone(),
                object_id: input.object_id.clone(),
                action_name: input.action_name.clone(),
                arguments: input.arguments.clone(),
            })
        } else {
            None
        };

        Ok(ReplayReport {
            proposal_id: input.proposal_id,
            mode: ReplayMode::DryRun,
            validation_passed,
            executed: false,
            checks,
            would_execute,
        })
    }

    async fn load_action_def(&self, input: &ReplayInput) -> (ReplayCheck, Option<OntologyActionDef>) {
        let Some(repo) = &self.ontology_repository else {
            return (
                ReplayCheck {
                    name: "ontology".to_string(),
                    passed: false,
                    message: "no ontology repository configured".to_string(),
                },
                None,
            );
        };

        let schema = match repo.load_action_overlays().await {
            Ok(overlays) => fms_domain::ontology::governed::load_governed_schema(&overlays),
            Err(e) => {
                return (
                    ReplayCheck {
                        name: "ontology".to_string(),
                        passed: false,
                        message: format!("failed to load ontology overlays: {e}"),
                    },
                    None,
                );
            }
        };

        let object = schema.objects.get(&input.object_type);
        match object {
            Some(obj) => {
                let action = obj.actions.get(&input.action_name);
                match action {
                    Some(action_def) => (
                        ReplayCheck {
                            name: "ontology".to_string(),
                            passed: true,
                            message: format!(
                                "action '{}' found on object '{}'",
                                input.action_name, input.object_type
                            ),
                        },
                        Some(action_def.clone()),
                    ),
                    None => (
                        ReplayCheck {
                            name: "ontology".to_string(),
                            passed: false,
                            message: format!(
                                "action '{}' not found on object '{}'",
                                input.action_name, input.object_type
                            ),
                        },
                        None,
                    ),
                }
            }
            None => (
                ReplayCheck {
                    name: "ontology".to_string(),
                    passed: false,
                    message: format!("object type '{}' not found in governed schema", input.object_type),
                },
                None,
            ),
        }
    }

    fn check_arguments(&self, input: &ReplayInput, action: &OntologyActionDef) -> ReplayCheck {
        if !input.arguments.is_object() && !input.arguments.is_null() {
            return ReplayCheck {
                name: "arguments".to_string(),
                passed: false,
                message: "arguments must be a JSON object".to_string(),
            };
        }

        let args_obj = match input.arguments.as_object() {
            Some(obj) => obj,
            None => {
                if action.parameters.iter().any(|(_, p)| p.required) {
                    return ReplayCheck {
                        name: "arguments".to_string(),
                        passed: false,
                        message: "arguments must be a non-null object for this action".to_string(),
                    };
                }
                return ReplayCheck {
                    name: "arguments".to_string(),
                    passed: true,
                    message: "arguments valid (no parameters required)".to_string(),
                };
            }
        };

        let mut missing = Vec::new();
        let mut type_errors = Vec::new();

        for (_param_name, param) in &action.parameters {
            if param.required && !args_obj.contains_key(&param.name) {
                missing.push(param.name.clone());
            }
            if let Some(value) = args_obj.get(&param.name) {
                let type_ok = match param.param_type.as_str() {
                    "string" => value.is_string(),
                    "number" | "integer" | "float" | "double" => value.is_number(),
                    "boolean" => value.is_boolean(),
                    "object" => value.is_object(),
                    "array" => value.is_array(),
                    _ => true,
                };
                if !type_ok {
                    type_errors.push(format!(
                        "{}: expected {}, got {}",
                        param.name,
                        param.param_type,
                        if value.is_string() {
                            "string"
                        } else if value.is_number() {
                            "number"
                        } else if value.is_boolean() {
                            "boolean"
                        } else if value.is_object() {
                            "object"
                        } else if value.is_array() {
                            "array"
                        } else {
                            "null"
                        }
                    ));
                }
            }
        }

        if !missing.is_empty() {
            return ReplayCheck {
                name: "arguments".to_string(),
                passed: false,
                message: format!("missing required parameters: {}", missing.join(", ")),
            };
        }

        if !type_errors.is_empty() {
            return ReplayCheck {
                name: "arguments".to_string(),
                passed: false,
                message: format!("parameter type mismatches: {}", type_errors.join("; ")),
            };
        }

        ReplayCheck {
            name: "arguments".to_string(),
            passed: true,
            message: format!("arguments valid against {} parameter(s)", action.parameters.len()),
        }
    }

    fn check_permissions(&self, input: &ReplayInput, action: &OntologyActionDef) -> ReplayCheck {
        if input.actor_permissions.is_empty() {
            return ReplayCheck {
                name: "permissions".to_string(),
                passed: false,
                message: "no actor permissions provided".to_string(),
            };
        }

        let has_wildcard = input.actor_permissions.iter().any(|p| p == "*");
        if has_wildcard {
            return ReplayCheck {
                name: "permissions".to_string(),
                passed: true,
                message: "actor has wildcard permission".to_string(),
            };
        }

        if action.required_permissions.is_empty() {
            return ReplayCheck {
                name: "permissions".to_string(),
                passed: true,
                message: "no required permissions for this action".to_string(),
            };
        }

        let missing: Vec<&str> = action
            .required_permissions
            .iter()
            .filter(|rp| !input.actor_permissions.iter().any(|ap| ap == rp.as_str()))
            .map(|s| s.as_str())
            .collect();

        if !missing.is_empty() {
            return ReplayCheck {
                name: "permissions".to_string(),
                passed: false,
                message: format!(
                    "missing required permissions: {} (have: {})",
                    missing.join(", "),
                    input.actor_permissions.join(", ")
                ),
            };
        }

        ReplayCheck {
            name: "permissions".to_string(),
            passed: true,
            message: format!(
                "actor has all {} required permission(s)",
                action.required_permissions.len()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_domain::models::ai_ontology::OntologyActionDef;
    use fms_domain::ontology::governed::ActionOverlay;
    use fms_domain::ports::ai_ontology_repository::{AiOntologyRepository, AiOntologyRepositoryError};

    /// DB 只能交回 overlay：覆盖代码 schema 里已经存在的 `(object, action)` 键。
    /// 测试用 `BusinessCase.create`（代码 schema 现成的写动作）作为已知键。
    struct FakeOntologyRepository {
        overlays: Vec<ActionOverlay>,
    }

    impl FakeOntologyRepository {
        fn with_active_object(object_type: &str, action_name: &str) -> Self {
            let overlays = vec![ActionOverlay {
                object: object_type.to_string(),
                action: action_name.to_string(),
                is_active: Some(true),
                risk: Some(fms_domain::models::ai_proposal::RiskLevel::Low),
                requires_approval: Some(false),
            }];
            Self { overlays }
        }
    }

    #[async_trait::async_trait]
    impl AiOntologyRepository for FakeOntologyRepository {
        async fn load_action_overlays(&self) -> Result<Vec<ActionOverlay>, AiOntologyRepositoryError> {
            Ok(self.overlays.clone())
        }

        async fn save_action_overlay(&self, _overlay: &ActionOverlay) -> Result<(), AiOntologyRepositoryError> {
            Ok(())
        }

        async fn delete_action_overlay(&self, _object: &str, _action: &str) -> Result<(), AiOntologyRepositoryError> {
            Ok(())
        }

        async fn count_active_objects(&self) -> Result<i64, AiOntologyRepositoryError> {
            Ok(1)
        }

        async fn count_active_write_actions(&self) -> Result<i64, AiOntologyRepositoryError> {
            Ok(1)
        }
    }

    #[tokio::test]
    async fn dry_run_replay_validates_without_business_side_effects() {
        let ontology = Arc::new(FakeOntologyRepository::with_active_object("BusinessCase", "create"));
        let service = AiProposalReplayService::new_for_test(ontology);

        let input = ReplayInput {
            proposal_id: "fixture-proposal".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: "BusinessCase".to_string(),
            object_id: "BC_REPLAY_001".to_string(),
            action_name: "create".to_string(),
            arguments: serde_json::json!({
                "flight_id": "FL123",
                "case_type": "delays",
                "description": "Replay case"
            }),
            expected_object_version: None,
            actor_permissions: vec!["business_case:create".to_string()],
            dry_run: true,
        };

        let report = service.replay(input).await.expect("replay");

        assert_eq!(report.mode, ReplayMode::DryRun);
        assert!(report.validation_passed);
        assert!(!report.executed);
    }

    #[tokio::test]
    async fn dry_run_replay_fails_on_missing_ontology_action() {
        let ontology = Arc::new(FakeOntologyRepository::with_active_object("BusinessCase", "create"));
        let service = AiProposalReplayService::new_for_test(ontology);

        let input = ReplayInput {
            proposal_id: "fixture-proposal".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: "BusinessCase".to_string(),
            object_id: "BC_REPLAY_001".to_string(),
            action_name: "nonexistent_action".to_string(),
            arguments: serde_json::json!({}),
            expected_object_version: None,
            actor_permissions: vec!["business_case:create".to_string()],
            dry_run: true,
        };

        let report = service.replay(input).await.expect("replay");

        assert!(!report.validation_passed);
        assert!(report.checks.iter().any(|c| !c.passed && c.name == "ontology"));
    }

    #[tokio::test]
    async fn dry_run_replay_rejects_empty_permissions() {
        let ontology = Arc::new(FakeOntologyRepository::with_active_object("BusinessCase", "create"));
        let service = AiProposalReplayService::new_for_test(ontology);

        let input = ReplayInput {
            proposal_id: "fixture-proposal".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: "BusinessCase".to_string(),
            object_id: "BC_REPLAY_001".to_string(),
            action_name: "create".to_string(),
            arguments: serde_json::json!({}),
            expected_object_version: None,
            actor_permissions: vec![],
            dry_run: true,
        };

        let report = service.replay(input).await.expect("replay");

        assert!(!report.validation_passed);
    }

    #[tokio::test]
    async fn dry_run_replay_returns_would_execute_receipt() {
        let ontology = Arc::new(FakeOntologyRepository::with_active_object("BusinessCase", "create"));
        let service = AiProposalReplayService::new_for_test(ontology);

        let input = ReplayInput {
            proposal_id: "fixture-proposal".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: "BusinessCase".to_string(),
            object_id: "BC_REPLAY_001".to_string(),
            action_name: "create".to_string(),
            arguments: serde_json::json!({
                "flight_id": "FL123",
                "case_type": "delays",
                "description": "Replay case"
            }),
            expected_object_version: None,
            actor_permissions: vec!["business_case:create".to_string()],
            dry_run: true,
        };

        let report = service.replay(input).await.expect("replay");
        let receipt = report.would_execute.expect("would_execute receipt");

        assert_eq!(receipt.object_type, "BusinessCase");
        assert_eq!(receipt.action_name, "create");
    }

    #[tokio::test]
    async fn dry_run_replay_fails_when_missing_required_permission() {
        let ontology = Arc::new(FakeOntologyRepository::with_active_object("BusinessCase", "create"));
        let service = AiProposalReplayService::new_for_test(ontology);

        let input = ReplayInput {
            proposal_id: "fixture-proposal".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: "BusinessCase".to_string(),
            object_id: "BC_REPLAY_001".to_string(),
            action_name: "create".to_string(),
            arguments: serde_json::json!({}),
            expected_object_version: None,
            actor_permissions: vec!["business_case:update".to_string()],
            dry_run: true,
        };

        let report = service.replay(input).await.expect("replay");
        assert!(!report.validation_passed);
        let perm_check = report.checks.iter().find(|c| c.name == "permissions").unwrap();
        assert!(!perm_check.passed);
        assert!(perm_check.message.contains("business_case:create"));
    }

    #[tokio::test]
    async fn dry_run_replay_fails_when_required_parameter_missing() {
        let ontology = Arc::new(FakeOntologyRepository::with_active_object("BusinessCase", "create"));
        let service = AiProposalReplayService::new_for_test(ontology);

        let input = ReplayInput {
            proposal_id: "fixture-proposal".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: "BusinessCase".to_string(),
            object_id: "BC_REPLAY_001".to_string(),
            action_name: "create".to_string(),
            arguments: serde_json::json!({}),
            expected_object_version: None,
            actor_permissions: vec!["business_case:create".to_string()],
            dry_run: true,
        };

        let report = service.replay(input).await.expect("replay");
        assert!(!report.validation_passed);
        let args_check = report.checks.iter().find(|c| c.name == "arguments").unwrap();
        assert!(!args_check.passed);
        assert!(args_check.message.contains("flight_id"));
    }
}
