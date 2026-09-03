use async_trait::async_trait;
use sqlx::{PgPool, Row};
use ulid::Ulid;

use fms_domain::models::ai_proposal::RiskLevel;
use fms_domain::ontology::governed::{load_governed_schema, ActionOverlay};
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
    async fn load_action_overlays(&self) -> Result<Vec<ActionOverlay>, AiOntologyRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT object_type, name, requires_approval, risk_level
            FROM aip_ontology_actions
            WHERE is_active = true AND deleted_at IS NULL
            ORDER BY object_type ASC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        let overlays = rows
            .iter()
            .filter_map(|row| {
                let object: String = row.try_get("object_type").ok()?;
                let action: String = row.try_get("name").ok()?;
                let requires_approval: bool = row.try_get("requires_approval").ok()?;
                let risk_level: String = row.try_get("risk_level").ok()?;
                Some(ActionOverlay {
                    object,
                    action,
                    is_active: Some(true),
                    risk: RiskLevel::from_str_loose(&risk_level),
                    requires_approval: Some(requires_approval),
                })
            })
            .collect();

        Ok(overlays)
    }

    async fn save_action_overlay(&self, overlay: &ActionOverlay) -> Result<(), AiOntologyRepositoryError> {
        let risk_level = overlay.risk.map(RiskLevel::label).unwrap_or("medium");
        let is_active = overlay.is_active.unwrap_or(true);
        let requires_approval = overlay.requires_approval.unwrap_or(true);
        sqlx::query(
            r#"
            INSERT INTO aip_ontology_actions (
                id, name, object_type, category, requires_approval, risk_level,
                constraint_rules, metadata, is_active
            )
            VALUES ($1, $2, $3, 'mutation', $4, $5, '[]'::jsonb, '{}'::jsonb, $6)
            ON CONFLICT (object_type, name) DO UPDATE SET
                requires_approval = EXCLUDED.requires_approval,
                risk_level = EXCLUDED.risk_level,
                is_active = EXCLUDED.is_active,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(format!("ovr_{}", Ulid::new()))
        .bind(&overlay.action)
        .bind(&overlay.object)
        .bind(requires_approval)
        .bind(risk_level)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn delete_action_overlay(&self, object: &str, action: &str) -> Result<(), AiOntologyRepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM aip_ontology_actions
            WHERE object_type = $1 AND name = $2
            "#,
        )
        .bind(object)
        .bind(action)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn count_active_objects(&self) -> Result<i64, AiOntologyRepositoryError> {
        let overlays = self.load_action_overlays().await?;
        Ok(load_governed_schema(&overlays).objects.len() as i64)
    }

    async fn count_active_write_actions(&self) -> Result<i64, AiOntologyRepositoryError> {
        let overlays = self.load_action_overlays().await?;
        let schema = load_governed_schema(&overlays);
        let count = schema
            .objects
            .values()
            .flat_map(|object| object.actions.values())
            .filter(|action| action.category == "write")
            .count();
        Ok(count as i64)
    }
}

fn db_err(error: sqlx::Error) -> AiOntologyRepositoryError {
    AiOntologyRepositoryError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::PgAiOntologyRepository;
    use fms_domain::models::ai_proposal::RiskLevel;

    use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
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

    async fn cleanup_fixture(repo: &PgAiOntologyRepository, object_type: &str) {
        let pool = &repo.pool;
        let _ = sqlx::query("DELETE FROM aip_constraints WHERE object_type = $1")
            .bind(object_type)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM aip_functions WHERE object_type = $1")
            .bind(object_type)
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

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated AIP ontology tables"]
    async fn loads_action_overlays_from_aip_tables() {
        let repo = repository_from_test_database().await;
        let suffix = Ulid::new().to_string();
        let object_type = format!("TestOntology{suffix}");
        let action_name = "summarize";
        cleanup_fixture(&repo, &object_type).await;

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
        .bind(json!([{"name": "title", "type": "string", "required": false}]))
        .bind(json!([]))
        .bind(json!([action_name]))
        .bind(json!(["test"]))
        .execute(pool)
        .await
        .expect("insert ontology object");

        // 生效的 overlay 行。
        sqlx::query(
            r#"
            INSERT INTO aip_ontology_actions (
                id, name, object_type, description, category, parameters,
                requires_approval, risk_level, constraint_rules, is_active
            ) VALUES ($1, $2, $3, $4, 'write', $5, true, 'HIGH', $6, true)
            "#,
        )
        .bind(format!("act_{suffix}"))
        .bind(action_name)
        .bind(&object_type)
        .bind("summarize test object")
        .bind(json!([]))
        .bind(json!([]))
        .execute(pool)
        .await
        .expect("insert active ontology action");

        // 停用的行不应出现在 overlays。
        sqlx::query(
            r#"
            INSERT INTO aip_ontology_actions (
                id, name, object_type, description, category, parameters,
                requires_approval, risk_level, constraint_rules, is_active
            ) VALUES ($1, $2, $3, $4, 'write', $5, false, 'LOW', $6, false)
            "#,
        )
        .bind(format!("act_inactive_{suffix}"))
        .bind("retired_action")
        .bind(&object_type)
        .bind("retired")
        .bind(json!([]))
        .bind(json!([]))
        .execute(pool)
        .await
        .expect("insert inactive ontology action");

        let overlays = repo.load_action_overlays().await.expect("load action overlays");

        let active = overlays
            .iter()
            .find(|o| o.object == object_type && o.action == action_name)
            .expect("active overlay present");
        assert_eq!(active.is_active, Some(true));
        assert_eq!(active.risk, Some(RiskLevel::High));
        assert_eq!(active.requires_approval, Some(true));

        assert!(
            overlays
                .iter()
                .all(|o| !(o.object == object_type && o.action == "retired_action")),
            "inactive action must not appear as overlay"
        );

        cleanup_fixture(&repo, &object_type).await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated AIP ontology tables"]
    async fn load_action_overlays_returns_empty_when_no_active_rows() {
        let repo = repository_from_test_database().await;
        let suffix = Ulid::new().to_string();
        let object_type = format!("OverlayEmpty{suffix}");
        cleanup_fixture(&repo, &object_type).await;

        let pool = &repo.pool;
        sqlx::query(
            r#"
            INSERT INTO aip_ontology_actions (
                id, name, object_type, description, category, parameters,
                requires_approval, risk_level, constraint_rules, is_active
            ) VALUES ($1, $2, $3, $4, 'write', $5, false, 'LOW', $6, false)
            "#,
        )
        .bind(format!("act_inactive_{suffix}"))
        .bind("retired")
        .bind(&object_type)
        .bind("retired")
        .bind(json!([]))
        .bind(json!([]))
        .execute(pool)
        .await
        .expect("insert inactive ontology action");

        let overlays = repo.load_action_overlays().await.expect("load action overlays");
        assert!(
            !overlays.iter().any(|o| o.object == object_type),
            "no overlays expected for an all-inactive fixture"
        );

        cleanup_fixture(&repo, &object_type).await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and migrated AIP ontology tables"]
    async fn counts_derive_from_governed_schema() {
        let repo = repository_from_test_database().await;
        // governed schema 总是至少包含代码真相源的对象与写动作，与 DB 行无关。
        let objects = repo.count_active_objects().await.expect("count objects");
        assert!(objects >= 1, "governed schema must expose at least one object");

        let write_actions = repo.count_active_write_actions().await.expect("count write actions");
        assert!(
            write_actions >= 1,
            "governed schema must expose at least one write action"
        );
    }
}
