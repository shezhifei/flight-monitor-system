use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row};

use fms_domain::ports::ai_object_policy_repository::{
    AiObjectAccessDecision, AiObjectAccessRequest, AiObjectPolicyRepository, AiObjectPolicyRepositoryError,
    AiObjectPolicySubject,
};

pub struct PgAiObjectPolicyRepository {
    pool: PgPool,
}

impl PgAiObjectPolicyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone)]
struct PolicyRow {
    principal_type: String,
    principal_id: String,
    granted: bool,
    conditions: Option<Value>,
}

#[async_trait]
impl AiObjectPolicyRepository for PgAiObjectPolicyRepository {
    async fn evaluate_access(
        &self,
        request: &AiObjectAccessRequest,
    ) -> Result<AiObjectAccessDecision, AiObjectPolicyRepositoryError> {
        let permission_candidates = permission_candidates(&request.object_type, &request.permission);
        let rows = sqlx::query(
            r#"
            SELECT principal_type, principal_id, granted, conditions
            FROM aip_object_policies
            WHERE object_type = $1
              AND (object_id = $2 OR object_id IS NULL OR object_id = '*')
              AND permission = ANY($3)
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        )
        .bind(&request.object_type)
        .bind(request.object_id.as_deref())
        .bind(&permission_candidates)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        if rows.is_empty() {
            return Ok(AiObjectAccessDecision::NoPolicy);
        }

        let rows = rows
            .into_iter()
            .map(|row| {
                Ok(PolicyRow {
                    principal_type: row.try_get("principal_type").map_err(db_err)?,
                    principal_id: row.try_get("principal_id").map_err(db_err)?,
                    granted: row.try_get("granted").map_err(db_err)?,
                    conditions: row.try_get("conditions").map_err(db_err)?,
                })
            })
            .collect::<Result<Vec<_>, AiObjectPolicyRepositoryError>>()?;

        Ok(evaluate_policy_rows(request, &rows))
    }
}

fn db_err(error: sqlx::Error) -> AiObjectPolicyRepositoryError {
    AiObjectPolicyRepositoryError::Database(error.to_string())
}

fn permission_candidates(object_type: &str, permission: &str) -> Vec<String> {
    let mut candidates = vec!["*".to_string(), permission.to_string()];
    if let Some((_, verb)) = permission.split_once(':') {
        candidates.push(verb.to_string());
    }
    let resource = object_type_to_permission_resource(object_type);
    if !permission.contains(':') {
        candidates.push(format!("{resource}:{permission}"));
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn object_type_to_permission_resource(object_type: &str) -> String {
    let mut out = String::new();
    for (index, ch) in object_type.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn evaluate_policy_rows(request: &AiObjectAccessRequest, rows: &[PolicyRow]) -> AiObjectAccessDecision {
    let applicable_rows = rows
        .iter()
        .filter(|row| conditions_are_applicable(row.conditions.as_ref(), request))
        .collect::<Vec<_>>();
    if applicable_rows.is_empty() {
        return AiObjectAccessDecision::NoPolicy;
    }

    let mut matched_allow = false;
    for row in applicable_rows {
        if !principal_matches(&request.subject, row) {
            continue;
        }
        if !row.granted {
            return AiObjectAccessDecision::Deny;
        }
        matched_allow = true;
    }

    if matched_allow {
        AiObjectAccessDecision::Allow
    } else {
        AiObjectAccessDecision::Deny
    }
}

fn conditions_are_applicable(conditions: Option<&Value>, request: &AiObjectAccessRequest) -> bool {
    match conditions {
        None | Some(Value::Null) => true,
        Some(Value::Object(map)) => {
            map.is_empty()
                || map
                    .iter()
                    .all(|(path, expected)| condition_matches(path, expected, request))
        }
        _ => false,
    }
}

fn condition_matches(path: &str, expected: &Value, request: &AiObjectAccessRequest) -> bool {
    let actual = resolve_condition_path(path, request);
    match expected {
        Value::Object(operators) => operators
            .iter()
            .all(|(operator, operand)| operator_matches(operator, actual.as_ref(), operand)),
        _ => actual
            .as_ref()
            .map(|actual| value_matches(actual, expected))
            .unwrap_or(false),
    }
}

fn operator_matches(operator: &str, actual: Option<&Value>, operand: &Value) -> bool {
    match operator.trim().to_ascii_lowercase().as_str() {
        "eq" => actual.map(|value| value_matches(value, operand)).unwrap_or(false),
        "ne" => actual.map(|value| !value_matches(value, operand)).unwrap_or(true),
        "in" => actual
            .map(|value| value_in_expected_set(value, operand))
            .unwrap_or(false),
        "contains" => actual
            .map(|value| value_contains_expected(value, operand))
            .unwrap_or(false),
        "exists" => operand
            .as_bool()
            .map(|required| actual.is_some() == required)
            .unwrap_or(false),
        _ => false,
    }
}

fn value_matches(actual: &Value, expected: &Value) -> bool {
    match expected {
        Value::Array(items) => items.iter().any(|item| value_matches(actual, item)),
        _ => actual == expected,
    }
}

fn value_in_expected_set(actual: &Value, expected_set: &Value) -> bool {
    match expected_set {
        Value::Array(items) => items.iter().any(|item| value_matches(actual, item)),
        _ => value_matches(actual, expected_set),
    }
}

fn value_contains_expected(actual: &Value, expected: &Value) -> bool {
    match actual {
        Value::Array(items) => items.iter().any(|item| value_matches(item, expected)),
        Value::String(text) => expected.as_str().map(|needle| text.contains(needle)).unwrap_or(false),
        _ => false,
    }
}

fn resolve_condition_path(path: &str, request: &AiObjectAccessRequest) -> Option<Value> {
    let path = path.trim();
    match path {
        "object_type" | "object.object_type" => return Some(Value::String(request.object_type.clone())),
        "object_id" | "object.object_id" => {
            return request.object_id.as_ref().map(|id| Value::String(id.clone()));
        }
        "permission" => return Some(Value::String(request.permission.clone())),
        "subject.user_id" | "requester.user_id" | "user_id" => {
            return Some(Value::String(request.subject.user_id.clone()));
        }
        "subject.department_id" | "requester.department_id" | "department_id" => {
            return request
                .subject
                .department_id
                .as_ref()
                .map(|id| Value::String(id.clone()));
        }
        "subject.permissions" | "requester.permissions" => {
            return Some(Value::Array(
                request.subject.permissions.iter().cloned().map(Value::String).collect(),
            ));
        }
        "subject.roles" | "requester.roles" => {
            return Some(Value::Array(
                request.subject.roles.iter().cloned().map(Value::String).collect(),
            ));
        }
        _ => {}
    }

    let object_path = path
        .strip_prefix("object.")
        .or_else(|| path.strip_prefix("$."))
        .unwrap_or(path);
    resolve_json_path(request.object_snapshot.as_ref()?, object_path).cloned()
}

fn resolve_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => {
                let index = segment.parse::<usize>().ok()?;
                items.get(index)?
            }
            _ => return None,
        };
    }
    Some(current)
}

fn principal_matches(subject: &AiObjectPolicySubject, row: &PolicyRow) -> bool {
    let principal_type = row.principal_type.trim().to_ascii_lowercase();
    let principal_id = row.principal_id.trim();
    if principal_id == "*" || principal_id.eq_ignore_ascii_case("all") {
        return true;
    }

    match principal_type.as_str() {
        "user" => subject.user_id == principal_id,
        "permission" => {
            subject.permissions.iter().any(|item| item == "*")
                || subject.permissions.iter().any(|item| item == principal_id)
        }
        "role" => subject.roles.iter().any(|item| item == principal_id),
        "department" => subject.department_id.as_deref() == Some(principal_id),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_policy_rows, permission_candidates, AiObjectAccessDecision, AiObjectAccessRequest,
        AiObjectPolicySubject, PgAiObjectPolicyRepository, PolicyRow,
    };
    use chrono::{Duration, Utc};
    use fms_domain::ports::ai_object_policy_repository::AiObjectPolicyRepository;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use ulid::Ulid;

    fn subject(user_id: &str, permissions: &[&str]) -> AiObjectPolicySubject {
        AiObjectPolicySubject::new(user_id, permissions.iter().map(|item| item.to_string()).collect())
    }

    fn policy(principal_type: &str, principal_id: &str, granted: bool) -> PolicyRow {
        PolicyRow {
            principal_type: principal_type.to_string(),
            principal_id: principal_id.to_string(),
            granted,
            conditions: None,
        }
    }

    fn request(subject: AiObjectPolicySubject) -> AiObjectAccessRequest {
        AiObjectAccessRequest {
            subject,
            object_type: "Flight".to_string(),
            object_id: Some("flight-1".to_string()),
            permission: "flight:write".to_string(),
            object_snapshot: Some(json!({
                "stand": "S01",
                "status": "scheduled",
                "tags": ["ops", "vip"],
                "nested": {"owner_department_id": "ops-1"}
            })),
        }
    }

    async fn repository_from_test_database() -> PgAiObjectPolicyRepository {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect TEST_DATABASE_URL");

        let policy_table: Option<String> = sqlx::query_scalar("SELECT to_regclass('public.aip_object_policies')::text")
            .fetch_one(&pool)
            .await
            .expect("check aip_object_policies table");
        if policy_table.is_none() {
            sqlx::raw_sql(include_str!(
                "../../../../../../migrations/073_create_aip_ontology_customization_tables.sql"
            ))
            .execute(&pool)
            .await
            .expect("apply aip ontology migration");
        }

        PgAiObjectPolicyRepository::new(pool)
    }

    async fn cleanup_policies(pool: &PgPool, object_type: &str) {
        let _ = sqlx::query("DELETE FROM aip_object_policies WHERE object_type = $1")
            .bind(object_type)
            .execute(pool)
            .await;
    }

    async fn insert_policy(
        pool: &PgPool,
        object_type: &str,
        object_id: Option<&str>,
        principal_type: &str,
        principal_id: &str,
        permission: &str,
        granted: bool,
        conditions: Option<serde_json::Value>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) {
        sqlx::query(
            r#"
            INSERT INTO aip_object_policies (
                id, object_type, object_id, principal_type, principal_id,
                permission, granted, conditions, expires_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(format!("policy_{}", Ulid::new()))
        .bind(object_type)
        .bind(object_id)
        .bind(principal_type)
        .bind(principal_id)
        .bind(permission)
        .bind(granted)
        .bind(conditions)
        .bind(expires_at)
        .execute(pool)
        .await
        .expect("insert object policy");
    }

    fn db_request(object_type: &str, object_id: &str, subject: AiObjectPolicySubject) -> AiObjectAccessRequest {
        AiObjectAccessRequest {
            subject,
            object_type: object_type.to_string(),
            object_id: Some(object_id.to_string()),
            permission: "test_object:write".to_string(),
            object_snapshot: Some(json!({
                "stand": "S01",
                "status": "scheduled",
                "tags": ["ops", "vip"],
                "nested": {"owner_department_id": "ops-1"}
            })),
        }
    }

    #[test]
    fn object_policy_deny_overrides_allow() {
        let decision = evaluate_policy_rows(
            &request(subject("user-1", &["flight:write"])),
            &[
                policy("permission", "flight:write", true),
                policy("user", "user-1", false),
            ],
        );

        assert_eq!(decision, AiObjectAccessDecision::Deny);
    }

    #[test]
    fn object_policy_scoped_rows_default_deny_unmatched_principal() {
        let decision = evaluate_policy_rows(
            &request(subject("user-2", &["flight:write"])),
            &[policy("user", "user-1", true)],
        );

        assert_eq!(decision, AiObjectAccessDecision::Deny);
    }

    #[test]
    fn object_policy_allows_matching_department_principal() {
        let mut subject = subject("user-2", &["flight:write"]);
        subject.department_id = Some("ops-1".to_string());

        let decision = evaluate_policy_rows(&request(subject), &[policy("department", "ops-1", true)]);

        assert_eq!(decision, AiObjectAccessDecision::Allow);
    }

    #[test]
    fn object_policy_unknown_condition_operator_is_not_applicable() {
        let mut conditional = policy("user", "user-1", true);
        conditional.conditions = Some(json!({"stand": {"starts_with": "S"}}));

        let decision = evaluate_policy_rows(&request(subject("user-1", &["flight:write"])), &[conditional]);

        assert_eq!(decision, AiObjectAccessDecision::NoPolicy);
    }

    #[test]
    fn object_policy_applies_matching_object_snapshot_conditions() {
        let mut conditional = policy("user", "user-1", true);
        conditional.conditions = Some(json!({
            "stand": "S01",
            "status": {"in": ["scheduled", "arrived"]},
            "tags": {"contains": "ops"},
            "nested.owner_department_id": "ops-1"
        }));

        let decision = evaluate_policy_rows(&request(subject("user-1", &["flight:write"])), &[conditional]);

        assert_eq!(decision, AiObjectAccessDecision::Allow);
    }

    #[test]
    fn object_policy_non_matching_conditions_are_not_applicable() {
        let mut conditional = policy("user", "user-1", true);
        conditional.conditions = Some(json!({"stand": "S99"}));

        let decision = evaluate_policy_rows(&request(subject("user-1", &["flight:write"])), &[conditional]);

        assert_eq!(decision, AiObjectAccessDecision::NoPolicy);
    }

    #[test]
    fn object_policy_supports_subject_conditions() {
        let mut conditional = policy("user", "user-1", true);
        conditional.conditions = Some(json!({"subject.department_id": "ops-1"}));
        let mut subject = subject("user-1", &["flight:write"]);
        subject.department_id = Some("ops-1".to_string());

        let decision = evaluate_policy_rows(&request(subject), &[conditional]);

        assert_eq!(decision, AiObjectAccessDecision::Allow);
    }

    #[test]
    fn permission_candidates_include_resource_and_verb_aliases() {
        let candidates = permission_candidates("DispatchOrder", "dispatch:write");

        assert!(candidates.contains(&"*".to_string()));
        assert!(candidates.contains(&"dispatch:write".to_string()));
        assert!(candidates.contains(&"write".to_string()));
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated AIP object policy table"]
    async fn pg_ai_object_policy_repository_evaluates_db_allow_deny_and_scoped_default() {
        let repo = repository_from_test_database().await;
        let object_type = format!("TestObjectPolicy{}", Ulid::new());
        let object_id = "object-1";
        cleanup_policies(&repo.pool, &object_type).await;

        insert_policy(
            &repo.pool,
            &object_type,
            Some(object_id),
            "permission",
            "test_object:write",
            "write",
            true,
            None,
            None,
        )
        .await;
        insert_policy(
            &repo.pool,
            &object_type,
            Some(object_id),
            "user",
            "user-1",
            "test_object:write",
            false,
            None,
            None,
        )
        .await;

        let deny_decision = repo
            .evaluate_access(&db_request(
                &object_type,
                object_id,
                subject("user-1", &["test_object:write"]),
            ))
            .await
            .expect("evaluate deny");
        assert_eq!(deny_decision, AiObjectAccessDecision::Deny);

        let allow_decision = repo
            .evaluate_access(&db_request(
                &object_type,
                object_id,
                subject("user-2", &["test_object:write"]),
            ))
            .await
            .expect("evaluate allow");
        assert_eq!(allow_decision, AiObjectAccessDecision::Allow);

        cleanup_policies(&repo.pool, &object_type).await;
        insert_policy(
            &repo.pool,
            &object_type,
            Some(object_id),
            "user",
            "other-user",
            "test_object:write",
            true,
            None,
            None,
        )
        .await;

        let scoped_default = repo
            .evaluate_access(&db_request(
                &object_type,
                object_id,
                subject("user-3", &["test_object:write"]),
            ))
            .await
            .expect("evaluate scoped default");
        assert_eq!(scoped_default, AiObjectAccessDecision::Deny);

        cleanup_policies(&repo.pool, &object_type).await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated AIP object policy table"]
    async fn pg_ai_object_policy_repository_evaluates_conditions_department_and_expiry() {
        let repo = repository_from_test_database().await;
        let object_type = format!("TestObjectPolicy{}", Ulid::new());
        let object_id = "object-1";
        cleanup_policies(&repo.pool, &object_type).await;

        insert_policy(
            &repo.pool,
            &object_type,
            None,
            "department",
            "ops-1",
            "*",
            true,
            Some(json!({
                "stand": "S01",
                "status": {"in": ["scheduled", "arrived"]},
                "tags": {"contains": "ops"},
                "subject.department_id": "ops-1"
            })),
            None,
        )
        .await;

        let mut ops_subject = subject("user-1", &["test_object:write"]);
        ops_subject.department_id = Some("ops-1".to_string());
        let allow_decision = repo
            .evaluate_access(&db_request(&object_type, object_id, ops_subject))
            .await
            .expect("evaluate matching department condition");
        assert_eq!(allow_decision, AiObjectAccessDecision::Allow);

        let mut other_subject = subject("user-2", &["test_object:write"]);
        other_subject.department_id = Some("ops-2".to_string());
        let deny_decision = repo
            .evaluate_access(&db_request(&object_type, object_id, other_subject))
            .await
            .expect("evaluate non-matching department condition");
        assert_eq!(deny_decision, AiObjectAccessDecision::NoPolicy);

        cleanup_policies(&repo.pool, &object_type).await;
        insert_policy(
            &repo.pool,
            &object_type,
            Some(object_id),
            "user",
            "user-1",
            "test_object:write",
            true,
            None,
            Some(Utc::now() - Duration::minutes(1)),
        )
        .await;

        let expired_decision = repo
            .evaluate_access(&db_request(
                &object_type,
                object_id,
                subject("user-1", &["test_object:write"]),
            ))
            .await
            .expect("evaluate expired policy");
        assert_eq!(expired_decision, AiObjectAccessDecision::NoPolicy);

        cleanup_policies(&repo.pool, &object_type).await;
    }
}
