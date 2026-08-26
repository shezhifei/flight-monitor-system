//! PostgreSQL shift handover repository implementation.

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::models::shift_handover::{ShiftHandover, ShiftHandoverItem};
use fms_domain::ports::shift_handover_repository::ShiftHandoverRepository;

pub struct PgShiftHandoverRepository {
    pool: PgPool,
}

impl PgShiftHandoverRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ShiftHandoverRepository for PgShiftHandoverRepository {
    async fn create(&self, handover: &ShiftHandover) -> Result<ShiftHandover, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        sqlx::query(
            r#"INSERT INTO shift_handovers (
                handover_id,
                shift_date,
                shift_code,
                from_user_id,
                to_user_id,
                position_user_id,
                from_operator_name,
                from_operator_job_title,
                to_operator_name,
                to_operator_job_title,
                status,
                summary,
                risk_level,
                signed_at,
                submitted_at,
                created_at,
                updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17
            )"#,
        )
        .bind(&handover.handover_id)
        .bind(handover.shift_date)
        .bind(&handover.shift_code)
        .bind(&handover.from_user_id)
        .bind(&handover.to_user_id)
        .bind(&handover.position_user_id)
        .bind(&handover.from_operator_name)
        .bind(&handover.from_operator_job_title)
        .bind(&handover.to_operator_name)
        .bind(&handover.to_operator_job_title)
        .bind(&handover.status)
        .bind(&handover.summary)
        .bind(&handover.risk_level)
        .bind(handover.signed_at)
        .bind(handover.submitted_at)
        .bind(handover.created_at)
        .bind(handover.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if !handover.items.is_empty() {
            let mut query_builder = QueryBuilder::<Postgres>::new(
                "INSERT INTO shift_handover_items (
                    item_id, handover_id, item_type, title, detail,
                    owner_user_id, due_at, is_mandatory, acknowledged,
                    acknowledged_at, acknowledged_by, created_at, updated_at
                )",
            );
            query_builder.push_values(&handover.items, |mut b, item| {
                b.push_bind(&item.item_id)
                    .push_bind(&item.handover_id)
                    .push_bind(&item.item_type)
                    .push_bind(&item.title)
                    .push_bind(&item.detail)
                    .push_bind(&item.owner_user_id)
                    .push_bind(item.due_at)
                    .push_bind(item.is_mandatory)
                    .push_bind(item.acknowledged)
                    .push_bind(item.acknowledged_at)
                    .push_bind(&item.acknowledged_by)
                    .push_bind(item.created_at)
                    .push_bind(item.updated_at);
            });
            query_builder
                .build()
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;

        self.find_by_id(&handover.handover_id)
            .await?
            .ok_or_else(|| DomainError::Internal("failed to load created shift handover".into()))
    }

    async fn find_by_id(&self, handover_id: &str) -> Result<Option<ShiftHandover>, DomainError> {
        let row = sqlx::query("SELECT * FROM ai_query.v_shift_handovers WHERE handover_id = $1 LIMIT 1")
            .bind(handover_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let items = load_items_for_ids(&self.pool, &[handover_id.to_string()]).await?;
        Ok(Some(row_to_handover(
            &row,
            items.get(handover_id).cloned().unwrap_or_default(),
        )))
    }

    async fn find_all(
        &self,
        shift_date: Option<NaiveDate>,
        shift_code: Option<&str>,
        status: Option<&str>,
        from_user_id: Option<&str>,
        to_user_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShiftHandover>, DomainError> {
        let mut query = QueryBuilder::<Postgres>::new("SELECT * FROM ai_query.v_shift_handovers WHERE 1 = 1");
        if let Some(value) = shift_date {
            query.push(" AND shift_date = ").push_bind(value);
        }
        if let Some(value) = shift_code {
            query.push(" AND shift_code = ").push_bind(value);
        }
        if let Some(value) = status {
            query.push(" AND status = ").push_bind(value);
        }
        if let Some(value) = from_user_id {
            query.push(" AND from_user_id = ").push_bind(value);
        }
        if let Some(value) = to_user_id {
            query.push(" AND to_user_id = ").push_bind(value);
        }
        query
            .push(" ORDER BY shift_date DESC, created_at DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows = query
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        if rows.is_empty() {
            return Ok(vec![]);
        }

        let handover_ids: Vec<String> = rows.iter().map(|row| row.get("handover_id")).collect();
        let items_by_handover = load_items_for_ids(&self.pool, &handover_ids).await?;

        Ok(rows
            .iter()
            .map(|row| {
                let handover_id: String = row.get("handover_id");
                row_to_handover(row, items_by_handover.get(&handover_id).cloned().unwrap_or_default())
            })
            .collect())
    }

    async fn submit(&self, handover_id: &str) -> Result<Option<ShiftHandover>, DomainError> {
        let row = sqlx::query(
            r#"UPDATE shift_handovers
               SET status = 'pending',
                   submitted_at = COALESCE(submitted_at, CURRENT_TIMESTAMP),
                   updated_at = CURRENT_TIMESTAMP
               WHERE handover_id = $1
                 AND status = 'draft'
               RETURNING handover_id"#,
        )
        .bind(handover_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if row.is_none() {
            return Ok(None);
        }
        self.find_by_id(handover_id).await
    }

    async fn acknowledge_item(
        &self,
        handover_id: &str,
        item_id: &str,
        acknowledged_by: &str,
        acknowledged: bool,
    ) -> Result<Option<ShiftHandoverItem>, DomainError> {
        let row = if acknowledged {
            sqlx::query(
                r#"UPDATE shift_handover_items
                   SET acknowledged = TRUE,
                       acknowledged_at = COALESCE(acknowledged_at, CURRENT_TIMESTAMP),
                       acknowledged_by = $1,
                       updated_at = CURRENT_TIMESTAMP
                   WHERE handover_id = $2
                     AND item_id = $3
                   RETURNING *"#,
            )
            .bind(acknowledged_by)
            .bind(handover_id)
            .bind(item_id)
            .fetch_optional(&self.pool)
            .await
        } else {
            sqlx::query(
                r#"UPDATE shift_handover_items
                   SET acknowledged = FALSE,
                       acknowledged_at = NULL,
                       acknowledged_by = NULL,
                       updated_at = CURRENT_TIMESTAMP
                   WHERE handover_id = $1
                     AND item_id = $2
                   RETURNING *"#,
            )
            .bind(handover_id)
            .bind(item_id)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(|value| row_to_item(&value)))
    }

    async fn list_unacked_mandatory_titles(&self, handover_id: &str) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query(
            r#"SELECT title
               FROM shift_handover_items
               WHERE handover_id = $1
                 AND is_mandatory = TRUE
                 AND acknowledged = FALSE
               ORDER BY created_at ASC"#,
        )
        .bind(handover_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.iter().map(|row| row.get("title")).collect())
    }

    async fn complete(
        &self,
        handover_id: &str,
        to_operator_name: Option<&str>,
        to_operator_job_title: Option<&str>,
    ) -> Result<Option<ShiftHandover>, DomainError> {
        let row = sqlx::query(
            r#"UPDATE shift_handovers
               SET status = 'completed',
                   to_operator_name = COALESCE($1, to_operator_name),
                   to_operator_job_title = COALESCE($2, to_operator_job_title),
                   signed_at = COALESCE(signed_at, CURRENT_TIMESTAMP),
                   updated_at = CURRENT_TIMESTAMP
               WHERE handover_id = $3
                 AND status IN ('pending', 'sign_off')
               RETURNING handover_id"#,
        )
        .bind(to_operator_name)
        .bind(to_operator_job_title)
        .bind(handover_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if row.is_none() {
            return Ok(None);
        }
        self.find_by_id(handover_id).await
    }
}

async fn load_items_for_ids(
    pool: &PgPool,
    handover_ids: &[String],
) -> Result<std::collections::HashMap<String, Vec<ShiftHandoverItem>>, DomainError> {
    if handover_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let rows = sqlx::query(
        r#"SELECT *
           FROM shift_handover_items
           WHERE handover_id = ANY($1)
           ORDER BY is_mandatory DESC, created_at ASC"#,
    )
    .bind(handover_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut items_by_handover = std::collections::HashMap::<String, Vec<ShiftHandoverItem>>::new();
    for row in rows {
        let item = row_to_item(&row);
        items_by_handover
            .entry(item.handover_id.clone())
            .or_default()
            .push(item);
    }
    Ok(items_by_handover)
}

fn row_to_handover(row: &sqlx::postgres::PgRow, items: Vec<ShiftHandoverItem>) -> ShiftHandover {
    ShiftHandover {
        handover_id: row.get("handover_id"),
        shift_date: row.get("shift_date"),
        shift_code: row.get("shift_code"),
        from_user_id: row.get("from_user_id"),
        to_user_id: row.get("to_user_id"),
        position_user_id: row.try_get::<Option<String>, _>("position_user_id").unwrap_or(None),
        from_operator_name: row.try_get::<Option<String>, _>("from_operator_name").unwrap_or(None),
        from_operator_job_title: row
            .try_get::<Option<String>, _>("from_operator_job_title")
            .unwrap_or(None),
        from_operator_label: row.try_get::<Option<String>, _>("from_operator_label").unwrap_or(None),
        to_operator_name: row.try_get::<Option<String>, _>("to_operator_name").unwrap_or(None),
        to_operator_job_title: row
            .try_get::<Option<String>, _>("to_operator_job_title")
            .unwrap_or(None),
        to_operator_label: row.try_get::<Option<String>, _>("to_operator_label").unwrap_or(None),
        status: row.try_get("status").unwrap_or_else(|_| "draft".to_string()),
        summary: row.try_get::<Option<String>, _>("summary").unwrap_or(None),
        risk_level: row.try_get("risk_level").unwrap_or_else(|_| "medium".to_string()),
        signed_at: row
            .try_get::<Option<chrono::DateTime<Utc>>, _>("signed_at")
            .unwrap_or(None),
        submitted_at: row
            .try_get::<Option<chrono::DateTime<Utc>>, _>("submitted_at")
            .unwrap_or(None),
        created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
        updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
        items,
    }
}

fn row_to_item(row: &sqlx::postgres::PgRow) -> ShiftHandoverItem {
    ShiftHandoverItem {
        item_id: row.get("item_id"),
        handover_id: row.get("handover_id"),
        item_type: row.try_get("item_type").unwrap_or_else(|_| "other".to_string()),
        title: row.get("title"),
        detail: row.try_get::<Option<String>, _>("detail").unwrap_or(None),
        owner_user_id: row.try_get::<Option<String>, _>("owner_user_id").unwrap_or(None),
        due_at: row
            .try_get::<Option<chrono::DateTime<Utc>>, _>("due_at")
            .unwrap_or(None),
        is_mandatory: row.try_get("is_mandatory").unwrap_or(true),
        acknowledged: row.try_get("acknowledged").unwrap_or(false),
        acknowledged_at: row
            .try_get::<Option<chrono::DateTime<Utc>>, _>("acknowledged_at")
            .unwrap_or(None),
        acknowledged_by: row.try_get::<Option<String>, _>("acknowledged_by").unwrap_or(None),
        created_at: row.try_get("created_at").unwrap_or_else(|_| Utc::now()),
        updated_at: row.try_get("updated_at").unwrap_or_else(|_| Utc::now()),
    }
}
