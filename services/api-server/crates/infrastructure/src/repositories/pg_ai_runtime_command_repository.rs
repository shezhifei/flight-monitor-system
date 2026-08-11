use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};

use fms_domain::models::ai_execution::{AiRuntimeCommandRecord, AiRuntimeCommandStatus, AiRuntimeCommandType};
use fms_domain::ports::ai_execution_repository::{AiExecutionRepositoryError, AiRuntimeCommandRepository};

pub struct PgAiRuntimeCommandRepository {
    pool: PgPool,
}

impl PgAiRuntimeCommandRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<AiRuntimeCommandRecord, AiExecutionRepositoryError> {
        let command_type_str: String = row.try_get("command_type").map_err(db_err)?;
        let status_str: String = row.try_get("status").map_err(db_err)?;
        let payload: serde_json::Value = row.try_get("payload").map_err(db_err)?;

        let command_type = AiRuntimeCommandType::from_str(&command_type_str).ok_or_else(|| {
            AiExecutionRepositoryError::validation(format!("invalid command_type: {command_type_str}"))
        })?;
        let status = AiRuntimeCommandStatus::from_str(&status_str)
            .ok_or_else(|| AiExecutionRepositoryError::validation(format!("invalid command status: {status_str}")))?;

        Ok(AiRuntimeCommandRecord {
            command_id: row.try_get("command_id").map_err(db_err)?,
            run_id: row.try_get("run_id").map_err(db_err)?,
            command_type,
            command_sequence: row.try_get::<i64, _>("command_sequence").map_err(db_err)?,
            tool_call_pk: row.try_get("tool_call_pk").map_err(db_err)?,
            payload,
            status,
            run_owner: row.try_get("run_owner").map_err(db_err)?,
            lease_owner: row.try_get("lease_owner").map_err(db_err)?,
            lease_expires_at: row.try_get("lease_expires_at").map_err(db_err)?,
            created_at: row.try_get("created_at").map_err(db_err)?,
            processed_at: row.try_get("processed_at").map_err(db_err)?,
            attempt_count: row.try_get::<i32, _>("attempt_count").map_err(db_err)?,
            max_attempts: row.try_get::<i32, _>("max_attempts").map_err(db_err)?,
            last_heartbeat_at: row.try_get("last_heartbeat_at").map_err(db_err)?,
            run_owner_lock: row.try_get("run_owner_lock").map_err(db_err)?,
        })
    }
}

#[async_trait]
impl AiRuntimeCommandRepository for PgAiRuntimeCommandRepository {
    async fn enqueue(&self, command: AiRuntimeCommandRecord) -> Result<(), AiExecutionRepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO ai_runtime_commands (
                command_id, run_id, command_type, command_sequence, tool_call_pk,
                payload, status, run_owner, lease_owner, lease_expires_at,
                created_at, processed_at, attempt_count, max_attempts,
                last_heartbeat_at, run_owner_lock
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16
            )
            ON CONFLICT (run_id, command_sequence) DO NOTHING
            "#,
        )
        .bind(&command.command_id)
        .bind(&command.run_id)
        .bind(command.command_type.as_str())
        .bind(command.command_sequence)
        .bind(&command.tool_call_pk)
        .bind(&command.payload)
        .bind(command.status.as_str())
        .bind(&command.run_owner)
        .bind(&command.lease_owner)
        .bind(command.lease_expires_at)
        .bind(command.created_at)
        .bind(command.processed_at)
        .bind(command.attempt_count)
        .bind(command.max_attempts)
        .bind(command.last_heartbeat_at)
        .bind(&command.run_owner_lock)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(())
    }

    async fn lease_pending(
        &self,
        owner: &str,
        lease_seconds: u32,
        batch_size: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let expiry = Utc::now() + Duration::seconds(lease_seconds as i64);
        let mut tx = self.pool.begin().await.map_err(db_err)?;

        let rows = sqlx::query(
            r#"
            SELECT command_id, run_id, command_type, command_sequence, tool_call_pk,
                   payload, status, run_owner, lease_owner, lease_expires_at,
                   created_at, processed_at, attempt_count, max_attempts,
                   last_heartbeat_at, run_owner_lock
            FROM ai_runtime_commands
            WHERE status = 'pending'
            ORDER BY created_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(batch_size as i64)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        for row in &rows {
            let command_id: String = row.try_get("command_id").map_err(db_err)?;
            sqlx::query(
                r#"
                UPDATE ai_runtime_commands
                SET status = 'leased', lease_owner = $2, lease_expires_at = $3
                WHERE command_id = $1
                "#,
            )
            .bind(&command_id)
            .bind(owner)
            .bind(expiry)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }

        tx.commit().await.map_err(db_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut record = Self::row_to_record(row)?;
            record.status = AiRuntimeCommandStatus::Leased;
            record.lease_owner = Some(owner.to_string());
            record.lease_expires_at = Some(expiry);
            out.push(record);
        }
        Ok(out)
    }

    async fn lease_pending_with_owner_check(
        &self,
        owner: &str,
        lease_seconds: u32,
        batch_size: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let now = Utc::now();
        let expiry = now + Duration::seconds(lease_seconds as i64);
        let mut tx = self.pool.begin().await.map_err(db_err)?;

        let rows = sqlx::query(
            r#"
            SELECT command_id, run_id, command_type, command_sequence, tool_call_pk,
                   payload, status, run_owner, lease_owner, lease_expires_at,
                   created_at, processed_at, attempt_count, max_attempts,
                   last_heartbeat_at, run_owner_lock
            FROM ai_runtime_commands
            WHERE (
                status = 'pending'
                OR (status = 'leased' AND lease_expires_at < $2)
            )
            AND (run_owner_lock IS NULL OR run_owner_lock = $3 OR (status = 'leased' AND lease_expires_at < $2))
            ORDER BY created_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(batch_size as i64)
        .bind(now)
        .bind(owner)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        let mut leased_records = Vec::new();
        for row in &rows {
            let command_id: String = row.try_get("command_id").map_err(db_err)?;
            let attempt_count: i32 = row.try_get("attempt_count").map_err(db_err)?;
            let max_attempts: i32 = row.try_get("max_attempts").map_err(db_err)?;

            if attempt_count >= max_attempts {
                sqlx::query(
                    r#"
                    UPDATE ai_runtime_commands
                    SET status = 'failed', processed_at = $2
                    WHERE command_id = $1
                    "#,
                )
                .bind(&command_id)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
                continue;
            }

            let run_id: String = row.try_get("run_id").map_err(db_err)?;
            let run_owner_lock: Option<String> = row.try_get("run_owner_lock").map_err(db_err)?;
            let effective_lock = if run_owner_lock.is_none() {
                Some(owner.to_string())
            } else {
                run_owner_lock
            };

            sqlx::query(
                r#"
                UPDATE ai_runtime_commands
                SET status = 'leased',
                    lease_owner = $2,
                    lease_expires_at = $3,
                    attempt_count = attempt_count + 1,
                    last_heartbeat_at = $4,
                    run_owner_lock = COALESCE(run_owner_lock, $5)
                WHERE command_id = $1
                "#,
            )
            .bind(&command_id)
            .bind(owner)
            .bind(expiry)
            .bind(now)
            .bind(effective_lock)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;

            let mut record = Self::row_to_record(row)?;
            record.status = AiRuntimeCommandStatus::Leased;
            record.lease_owner = Some(owner.to_string());
            record.lease_expires_at = Some(expiry);
            record.attempt_count = attempt_count + 1;
            record.last_heartbeat_at = Some(now);
            if record.run_owner_lock.is_none() {
                record.run_owner_lock = Some(owner.to_string());
            }
            leased_records.push(record);
            let _ = run_id;
        }

        tx.commit().await.map_err(db_err)?;
        Ok(leased_records)
    }

    async fn complete(&self, command_id: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_runtime_commands
            SET status = 'completed', processed_at = $2
            WHERE command_id = $1
            "#,
        )
        .bind(command_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn fail(&self, command_id: &str, _error: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE ai_runtime_commands
            SET status = 'failed', processed_at = $2
            WHERE command_id = $1
            "#,
        )
        .bind(command_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, command_id: &str) -> Result<Option<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT command_id, run_id, command_type, command_sequence, tool_call_pk,
                   payload, status, run_owner, lease_owner, lease_expires_at,
                   created_at, processed_at, attempt_count, max_attempts,
                   last_heartbeat_at, run_owner_lock
            FROM ai_runtime_commands WHERE command_id = $1
            "#,
        )
        .bind(command_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.as_ref().map(Self::row_to_record).transpose()
    }

    async fn heartbeat_command(&self, command_id: &str) -> Result<(), AiExecutionRepositoryError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE ai_runtime_commands
            SET last_heartbeat_at = $2
            WHERE command_id = $1 AND status = 'leased'
            "#,
        )
        .bind(command_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        if result.rows_affected() == 0 {
            return Err(AiExecutionRepositoryError::validation(format!(
                "command {command_id} is not leased"
            )));
        }
        Ok(())
    }

    async fn take_over_run(
        &self,
        run_id: &str,
        new_owner: &str,
        lease_seconds: u32,
    ) -> Result<Option<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let now = Utc::now();
        let expiry = now + Duration::seconds(lease_seconds as i64);
        let mut tx = self.pool.begin().await.map_err(db_err)?;

        sqlx::query(
            r#"
            UPDATE ai_runtime_commands
            SET run_owner_lock = $2
            WHERE run_id = $1 AND status IN ('pending', 'leased')
            "#,
        )
        .bind(run_id)
        .bind(new_owner)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        let start_row = sqlx::query(
            r#"
            SELECT command_id, run_id, command_type, command_sequence, tool_call_pk,
                   payload, status, run_owner, lease_owner, lease_expires_at,
                   created_at, processed_at, attempt_count, max_attempts,
                   last_heartbeat_at, run_owner_lock
            FROM ai_runtime_commands
            WHERE run_id = $1 AND command_type = 'start_run' AND status IN ('pending', 'leased')
            ORDER BY created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        let Some(row) = start_row else {
            tx.commit().await.map_err(db_err)?;
            return Ok(None);
        };

        let command_id: String = row.try_get("command_id").map_err(db_err)?;
        let attempt_count: i32 = row.try_get("attempt_count").map_err(db_err)?;

        sqlx::query(
            r#"
            UPDATE ai_runtime_commands
            SET status = 'leased',
                lease_owner = $2,
                lease_expires_at = $3,
                attempt_count = attempt_count + 1,
                last_heartbeat_at = $4,
                run_owner_lock = $2
            WHERE command_id = $1
            "#,
        )
        .bind(&command_id)
        .bind(new_owner)
        .bind(expiry)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        let mut record = Self::row_to_record(&row)?;
        record.status = AiRuntimeCommandStatus::Leased;
        record.lease_owner = Some(new_owner.to_string());
        record.lease_expires_at = Some(expiry);
        record.attempt_count = attempt_count + 1;
        record.last_heartbeat_at = Some(now);
        record.run_owner_lock = Some(new_owner.to_string());
        Ok(Some(record))
    }

    async fn list_expired_leases(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiExecutionRepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT command_id, run_id, command_type, command_sequence, tool_call_pk,
                   payload, status, run_owner, lease_owner, lease_expires_at,
                   created_at, processed_at, attempt_count, max_attempts,
                   last_heartbeat_at, run_owner_lock
            FROM ai_runtime_commands
            WHERE status = 'leased' AND lease_expires_at < $1 AND attempt_count < max_attempts
            ORDER BY lease_expires_at ASC
            LIMIT $2
            "#,
        )
        .bind(now)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.iter().map(Self::row_to_record).collect()
    }
}

fn db_err(error: sqlx::Error) -> AiExecutionRepositoryError {
    AiExecutionRepositoryError::Database(error.to_string())
}
