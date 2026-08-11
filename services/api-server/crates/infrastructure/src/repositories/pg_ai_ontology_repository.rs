use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

use fms_domain::models::ai_ontology::{
    OntologyActionDef, OntologyActionParameter, OntologyConstraint, OntologyFieldDef, OntologyObjectDef,
    OntologyRelationDef, OntologySchema,
};
use fms_domain::ports::ai_ontology_repository::{AiOntologyRepository, AiOntologyRepositoryError};

pub struct PgAiOntologyRepository {
    pool: PgPool,
}

impl PgAiOntologyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AiOntologyRepository for PgAiOntologyRepository {
    async fn load_active_schema(&self) -> Result<Option<OntologySchema>, AiOntologyRepositoryError> {
        let object_rows = sqlx::query(
            r#"
            SELECT name, description, properties, relationships
            FROM aip_ontology_objects
            WHERE is_active = true
            ORDER BY name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        if object_rows.is_empty() {
            return Ok(None);
        }

        let mut objects = HashMap::new();
        for row in object_rows {
            let name: String = row.try_get("name").map_err(db_err)?;
            let description: Option<String> = row.try_get("description").map_err(db_err)?;
            let properties: Value = row.try_get("properties").map_err(db_err)?;
            let relationships: Value = row.try_get("relationships").map_err(db_err)?;

            objects.insert(
                name.clone(),
                OntologyObjectDef {
                    object_id_strategy: object_id_strategy(&name),
                    name: name.clone(),
                    description: description.unwrap_or_default(),
                    fields: parse_fields(&properties),
                    relations: parse_relations(&relationships),
                    actions: HashMap::new(),
                },
            );
        }

        let action_rows = sqlx::query(
            r#"
            SELECT
                a.name,
                a.object_type,
                a.description,
                a.category,
                a.parameters,
                a.requires_approval,
                a.risk_level,
                a.constraint_rules,
                f.permission_required,
                f.parameters_schema
            FROM aip_ontology_actions a
            LEFT JOIN aip_functions f
              ON f.object_type = a.object_type
             AND f.action_name = a.name
             AND f.is_active = true
            WHERE a.is_active = true
            ORDER BY a.object_type ASC, a.name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        for row in action_rows {
            let object_type: String = row.try_get("object_type").map_err(db_err)?;
            let action_name: String = row.try_get("name").map_err(db_err)?;
            let description: Option<String> = row.try_get("description").map_err(db_err)?;
            let category = normalize_category(
                &row.try_get::<Option<String>, _>("category")
                    .map_err(db_err)?
                    .unwrap_or_else(|| "write".to_string()),
            );
            let parameters: Value = row.try_get("parameters").map_err(db_err)?;
            let requires_approval: bool = row.try_get("requires_approval").map_err(db_err)?;
            let risk_level: String = row.try_get("risk_level").map_err(db_err)?;
            let constraint_rules: Value = row.try_get("constraint_rules").map_err(db_err)?;
            let permission_required: Option<String> = row.try_get("permission_required").map_err(db_err)?;
            let parameters_schema: Option<Value> = row.try_get("parameters_schema").map_err(db_err)?;

            if let Some(object) = objects.get_mut(&object_type) {
                let parsed_parameters = parse_parameters(&parameters);
                let schema = parameters_schema.unwrap_or_else(|| schema_from_parameters(&parameters));
                let normalized_risk = normalize_risk(&risk_level);
                let approval_policy = approval_policy_for(requires_approval, &normalized_risk);

                object.actions.insert(
                    action_name.clone(),
                    OntologyActionDef {
                        name: action_name.clone(),
                        description: description.unwrap_or_default(),
                        category,
                        parameters: parsed_parameters,
                        parameters_schema: schema,
                        required_permissions: parse_required_permissions(permission_required),
                        risk_level: normalized_risk,
                        approval_strategy: approval_policy.to_string(),
                        approval_policy: approval_policy.to_string(),
                        constraints: parse_constraints(&constraint_rules),
                        execution_mapping: Some(format!("DomainActionExecutor.{}.{}", object_type, action_name)),
                        idempotency_key_strategy: Some("job_id:object_id:action_name".to_string()),
                        compensation: None,
                    },
                );
            }
        }

        let mut schema = OntologySchema {
            version: "flight-ops.v1".to_string(),
            description: "Flight Operations Ontology Schema V1".to_string(),
            objects,
        };
        self.attach_constraints(&mut schema).await?;
        validate_schema(&schema)?;

        Ok(Some(schema))
    }

    async fn count_active_objects(&self) -> Result<i64, AiOntologyRepositoryError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*)::bigint FROM aip_ontology_objects WHERE is_active = true")
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(count.0)
    }

    async fn count_active_write_actions(&self) -> Result<i64, AiOntologyRepositoryError> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)::bigint
            FROM aip_ontology_actions a
            JOIN aip_ontology_objects o ON a.object_type = o.name
            WHERE o.is_active = true
              AND a.is_active = true
              AND a.category = 'mutation'
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(count.0)
    }
}

impl PgAiOntologyRepository {
    async fn attach_constraints(&self, schema: &mut OntologySchema) -> Result<(), AiOntologyRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT object_type, action_name, constraint_type, expression, error_message
            FROM aip_constraints
            WHERE is_active = true
            ORDER BY object_type ASC, action_name ASC NULLS FIRST, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        for row in rows {
            let object_type: String = row.try_get("object_type").map_err(db_err)?;
            let action_name: Option<String> = row.try_get("action_name").map_err(db_err)?;
            let constraint = OntologyConstraint {
                constraint_type: row.try_get("constraint_type").map_err(db_err)?,
                expression: row.try_get("expression").map_err(db_err)?,
                description: row
                    .try_get::<Option<String>, _>("error_message")
                    .map_err(db_err)?
                    .unwrap_or_default(),
            };

            let Some(object) = schema.objects.get_mut(&object_type) else {
                continue;
            };
            if let Some(action_name) = action_name.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                if let Some(action) = object.actions.get_mut(action_name) {
                    action.constraints.push(constraint);
                }
            } else {
                for action in object.actions.values_mut() {
                    action.constraints.push(constraint.clone());
                }
            }
        }

        Ok(())
    }
}

fn parse_fields(value: &Value) -> HashMap<String, OntologyFieldDef> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_string();
            Some((
                name.clone(),
                OntologyFieldDef {
                    name,
                    field_type: item.get("type").and_then(Value::as_str).unwrap_or("string").to_string(),
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    required: item.get("required").and_then(Value::as_bool).unwrap_or(false),
                },
            ))
        })
        .collect()
}

fn parse_relations(value: &Value) -> HashMap<String, OntologyRelationDef> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_string();
            Some((
                name.clone(),
                OntologyRelationDef {
                    name,
                    target_object: item
                        .get("target_object")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    relation_type: item
                        .get("cardinality")
                        .and_then(Value::as_str)
                        .unwrap_or("one")
                        .to_string(),
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                },
            ))
        })
        .collect()
}

fn parse_parameters(value: &Value) -> HashMap<String, OntologyActionParameter> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_string();
            Some((
                name.clone(),
                OntologyActionParameter {
                    name,
                    param_type: item.get("type").and_then(Value::as_str).unwrap_or("string").to_string(),
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    required: item.get("required").and_then(Value::as_bool).unwrap_or(false),
                },
            ))
        })
        .collect()
}

fn parse_constraints(value: &Value) -> Vec<OntologyConstraint> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| OntologyConstraint {
            constraint_type: item
                .get("type")
                .or_else(|| item.get("constraint_type"))
                .and_then(Value::as_str)
                .unwrap_or("business_rule")
                .to_string(),
            expression: item
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: item
                .get("error_message")
                .or_else(|| item.get("description"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
        .collect()
}

fn schema_from_parameters(value: &Value) -> Value {
    let required: Vec<Value> = value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item.get("required").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .map(|name| json!(name))
        .collect();
    json!({
        "type": "object",
        "required": required,
    })
}

fn parse_required_permissions(permission_required: Option<String>) -> Vec<String> {
    permission_required
        .as_deref()
        .unwrap_or_default()
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn approval_policy_for(requires_approval: bool, risk_level: &str) -> &'static str {
    match risk_level {
        "critical" | "high" => "require_approval",
        "medium" => "require_approval",
        "low" if !requires_approval => "auto_execute",
        _ => "require_approval",
    }
}

fn normalize_category(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "read" | "query" | "read_only" | "readonly" => "read".to_string(),
        "advisory" | "advisor" | "analysis" | "recommendation" => "advisory".to_string(),
        "write" | "mutation" | "object_action" | "action" | "tool" => "write".to_string(),
        other => other.to_string(),
    }
}

fn validate_schema(schema: &OntologySchema) -> Result<(), AiOntologyRepositoryError> {
    if schema.version.trim().is_empty() {
        return Err(AiOntologyRepositoryError::Validation(
            "ontology schema version is empty".to_string(),
        ));
    }
    if schema.objects.is_empty() {
        return Err(AiOntologyRepositoryError::Validation(
            "ontology schema has no objects".to_string(),
        ));
    }

    for (object_name, object) in &schema.objects {
        if object.name.trim().is_empty() {
            return Err(AiOntologyRepositoryError::Validation(format!(
                "ontology object {object_name} has empty name"
            )));
        }
        for (action_name, action) in &object.actions {
            if !matches!(action.category.as_str(), "read" | "advisory" | "write") {
                return Err(AiOntologyRepositoryError::Validation(format!(
                    "ontology action {object_name}.{action_name} has invalid category {}",
                    action.category
                )));
            }
            if !matches!(action.risk_level.as_str(), "low" | "medium" | "high" | "critical") {
                return Err(AiOntologyRepositoryError::Validation(format!(
                    "ontology action {object_name}.{action_name} has invalid risk {}",
                    action.risk_level
                )));
            }
            let schema_type = action
                .parameters_schema
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if schema_type != "object" {
                return Err(AiOntologyRepositoryError::Validation(format!(
                    "ontology action {object_name}.{action_name} parameters_schema must be object"
                )));
            }
            if action.required_permissions.iter().any(|value| value.trim().is_empty()) {
                return Err(AiOntologyRepositoryError::Validation(format!(
                    "ontology action {object_name}.{action_name} has empty required permission"
                )));
            }
        }
    }

    Ok(())
}

fn object_id_strategy(name: &str) -> String {
    match name {
        "Flight" => "flight_id",
        "FlightLeg" => "leg_id",
        "DispatchOrder" => "dispatch_order_id",
        "BusinessCase" => "business_case_id",
        "WorkflowRun" => "workflow_run_id",
        "Notification" => "notification_id",
        "Todo" => "todo_id",
        "Anomaly" => "anomaly_id",
        "Stand" => "stand_id",
        "Team" => "team_id",
        "Equipment" => "equipment_id",
        _ => "id",
    }
    .to_string()
}

fn normalize_risk(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => "medium".to_string(),
        "low" | "medium" | "high" | "critical" => value.trim().to_ascii_lowercase(),
        _ => "medium".to_string(),
    }
}

fn db_err(error: sqlx::Error) -> AiOntologyRepositoryError {
    AiOntologyRepositoryError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        approval_policy_for, normalize_category, parse_required_permissions, validate_schema, PgAiOntologyRepository,
    };
    use fms_domain::models::ai_ontology::{OntologyActionDef, OntologyObjectDef, OntologySchema};
    use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use std::collections::HashMap;
    use ulid::Ulid;

    async fn repository_from_test_database() -> PgAiOntologyRepository {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect TEST_DATABASE_URL");

        let aip_objects: Option<String> = sqlx::query_scalar("SELECT to_regclass('public.aip_ontology_objects')::text")
            .fetch_one(&pool)
            .await
            .expect("check aip_ontology_objects table");
        if aip_objects.is_none() {
            sqlx::raw_sql(include_str!(
                "../../../../../../migrations/073_create_aip_ontology_customization_tables.sql"
            ))
            .execute(&pool)
            .await
            .expect("apply aip ontology migration");
        }

        PgAiOntologyRepository::new(pool)
    }

    async fn cleanup_fixture(repo: &PgAiOntologyRepository, object_type: &str, action_name: &str) {
        let pool = &repo.pool;
        let _ = sqlx::query("DELETE FROM aip_constraints WHERE object_type = $1")
            .bind(object_type)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM aip_functions WHERE object_type = $1 AND action_name = $2")
            .bind(object_type)
            .bind(action_name)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM aip_ontology_actions WHERE object_type = $1")
            .bind(object_type)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM aip_ontology_objects WHERE name = $1")
            .bind(object_type)
            .execute(pool)
            .await;
    }

    #[test]
    fn parses_permission_required_into_permission_list() {
        let parsed =
            parse_required_permissions(Some("flight:write, dispatch:publish;business_case:update".to_string()));
        assert_eq!(
            parsed,
            vec![
                "flight:write".to_string(),
                "dispatch:publish".to_string(),
                "business_case:update".to_string(),
            ]
        );
    }

    #[test]
    fn high_risk_actions_are_never_auto_execute() {
        assert_eq!(approval_policy_for(false, "high"), "require_approval");
        assert_eq!(approval_policy_for(false, "critical"), "require_approval");
        assert_eq!(approval_policy_for(false, "medium"), "require_approval");
        assert_eq!(approval_policy_for(false, "low"), "auto_execute");
    }

    #[test]
    fn normalizes_legacy_db_action_categories() {
        assert_eq!(normalize_category("mutation"), "write");
        assert_eq!(normalize_category("object_action"), "write");
        assert_eq!(normalize_category("query"), "read");
        assert_eq!(normalize_category("analysis"), "advisory");
    }

    #[test]
    fn validates_action_parameter_schema_contract() {
        let mut actions = HashMap::new();
        actions.insert(
            "bad_action".to_string(),
            OntologyActionDef {
                name: "bad_action".to_string(),
                description: "bad".to_string(),
                category: "write".to_string(),
                parameters: HashMap::new(),
                parameters_schema: json!({"type": "string"}),
                required_permissions: vec!["flight:write".to_string()],
                risk_level: "low".to_string(),
                approval_strategy: "auto_execute".to_string(),
                approval_policy: "auto_execute".to_string(),
                constraints: vec![],
                execution_mapping: None,
                idempotency_key_strategy: None,
                compensation: None,
            },
        );
        let mut objects = HashMap::new();
        objects.insert(
            "Flight".to_string(),
            OntologyObjectDef {
                name: "Flight".to_string(),
                description: "Flight".to_string(),
                object_id_strategy: "flight_id".to_string(),
                fields: HashMap::new(),
                relations: HashMap::new(),
                actions,
            },
        );

        let schema = OntologySchema {
            version: "flight-ops.v1".to_string(),
            description: "test".to_string(),
            objects,
        };

        let err = validate_schema(&schema).expect_err("invalid schema must be rejected");
        assert!(err.to_string().contains("parameters_schema must be object"));
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated AIP ontology tables"]
    async fn pg_ai_ontology_repository_loads_active_schema_from_aip_tables() {
        let repo = repository_from_test_database().await;
        let suffix = Ulid::new().to_string();
        let object_type = format!("TestOntology{suffix}");
        let action_name = "summarize";
        cleanup_fixture(&repo, &object_type, action_name).await;

        let pool = &repo.pool;
        sqlx::query(
            r#"
            INSERT INTO aip_ontology_objects (
                id, name, plural_name, description, properties, relationships, actions, tags, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true)
            "#,
        )
        .bind(format!("obj_{suffix}"))
        .bind(&object_type)
        .bind(format!("{object_type}s"))
        .bind("test ontology object")
        .bind(json!([
            {"name": "title", "type": "string", "required": true, "description": "title field"},
            {"name": "severity", "type": "string", "required": false, "description": "severity field"}
        ]))
        .bind(json!([
            {"name": "related_flight", "target_object": "Flight", "cardinality": "one", "description": "related flight"}
        ]))
        .bind(json!([action_name]))
        .bind(json!(["test"]))
        .execute(pool)
        .await
        .expect("insert ontology object");

        sqlx::query(
            r#"
            INSERT INTO aip_ontology_actions (
                id, name, object_type, description, category, parameters,
                requires_approval, risk_level, constraint_rules, is_active
            ) VALUES ($1, $2, $3, $4, 'advisory', $5, false, 'LOW', $6, true)
            "#,
        )
        .bind(format!("act_{suffix}"))
        .bind(action_name)
        .bind(&object_type)
        .bind("summarize test object")
        .bind(json!([
            {"name": "note", "type": "string", "required": true, "description": "operator note"}
        ]))
        .bind(json!([
            {"type": "business_rule", "expression": "note != ''", "error_message": "note required"}
        ]))
        .execute(pool)
        .await
        .expect("insert ontology action");

        sqlx::query(
            r#"
            INSERT INTO aip_functions (
                id, name, category, object_type, action_name, description,
                parameters_schema, requires_approval, risk_level, permission_required, is_active
            ) VALUES ($1, $2, 'object_action', $3, $4, $5, $6, false, 'LOW', $7, true)
            "#,
        )
        .bind(format!("fn_{suffix}"))
        .bind(format!("{object_type}.{action_name}"))
        .bind(&object_type)
        .bind(action_name)
        .bind("summarize function")
        .bind(json!({
            "type": "object",
            "properties": {
                "note": {"type": "string", "description": "operator note"}
            },
            "required": ["note"]
        }))
        .bind("test:read test:advise")
        .execute(pool)
        .await
        .expect("insert function");

        sqlx::query(
            r#"
            INSERT INTO aip_constraints (
                id, name, object_type, action_name, constraint_type,
                expression, error_message, severity, is_active
            ) VALUES ($1, $2, $3, $4, 'validation', 'note.length > 0', 'note cannot be empty', 'ERROR', true)
            "#,
        )
        .bind(format!("const_{suffix}"))
        .bind(format!("note_required_{suffix}"))
        .bind(&object_type)
        .bind(action_name)
        .execute(pool)
        .await
        .expect("insert constraint");

        let schema = repo
            .load_active_schema()
            .await
            .expect("load active schema")
            .expect("schema exists");
        let object = schema.objects.get(&object_type).expect("fixture object exists");
        assert_eq!(object.object_id_strategy, "id");
        assert!(object.fields.contains_key("title"));
        assert_eq!(
            object
                .relations
                .get("related_flight")
                .expect("relation exists")
                .target_object,
            "Flight"
        );

        let action = object.actions.get(action_name).expect("action exists");
        assert_eq!(action.category, "advisory");
        assert_eq!(action.parameters["note"].param_type, "string");
        assert_eq!(action.parameters_schema["type"], "object");
        assert_eq!(
            action.required_permissions,
            vec!["test:read".to_string(), "test:advise".to_string()]
        );
        assert_eq!(action.risk_level, "low");
        assert_eq!(action.approval_policy, "auto_execute");
        assert!(action
            .constraints
            .iter()
            .any(|constraint| constraint.expression == "note != ''"));
        assert!(action
            .constraints
            .iter()
            .any(|constraint| constraint.expression == "note.length > 0"));

        cleanup_fixture(&repo, &object_type, action_name).await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated AIP ontology tables"]
    async fn inactive_object_excluded_from_active_schema() {
        let repo = repository_from_test_database().await;
        let suffix = Ulid::new().to_string();
        let object_type = format!("InactiveObj{suffix}");
        let action_name = "test_action";
        cleanup_fixture(&repo, &object_type, action_name).await;

        let pool = &repo.pool;
        sqlx::query(
            r#"
            INSERT INTO aip_ontology_objects (
                id, name, plural_name, description, properties, relationships, actions, tags, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false)
            "#,
        )
        .bind(format!("obj_inactive_{suffix}"))
        .bind(&object_type)
        .bind(format!("{object_type}s"))
        .bind("inactive test object")
        .bind(json!([{"name": "title", "type": "string", "required": true, "description": "title"}]))
        .bind(json!([]))
        .bind(json!([action_name]))
        .bind(json!(["test"]))
        .execute(pool)
        .await
        .expect("insert inactive ontology object");

        let schema = repo.load_active_schema().await.expect("load active schema");
        if let Some(ref s) = schema {
            assert!(
                !s.objects.contains_key(&object_type),
                "inactive object must not appear in active schema"
            );
        }

        cleanup_fixture(&repo, &object_type, action_name).await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated AIP ontology tables"]
    async fn active_schema_returns_none_when_no_active_objects() {
        let repo = repository_from_test_database().await;
        let pool = &repo.pool;

        let active_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM aip_ontology_objects WHERE is_active = true")
            .fetch_one(pool)
            .await
            .expect("count active objects");

        if active_count > 0 {
            // This test only meaningful on a clean DB; skip if seed data exists.
            return;
        }

        let result = repo.load_active_schema().await.expect("load active schema");
        assert!(result.is_none(), "repo must return None when no active objects exist");
    }
}
