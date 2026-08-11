use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::models::ai_job::{AiJobRecord, AiJobStatusCount};
use fms_domain::ports::ai_job_repository::{AiJobRepository, AiJobRepositoryError};

const JOB_SELECT: &str = "job_id, job_type, status, requester_user_id, ontology_version, context_policy, risk_ceiling, correlation_id, created_at, started_at, finished_at, cancelled_at, error_code, error_message, timeout_ms, lease_owner, lease_expires_at, last_heartbeat_at, attempt_count, max_attempts, expires_at";

pub struct PgAiJobRepository {
    pool: PgPool,
}

impl PgAiJobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_job(row: &sqlx::postgres::PgRow) -> Result<AiJobRecord, AiJobRepositoryError> {
        Ok(AiJobRecord {
            job_id: row.try_get("job_id").map_err(db_err)?,
            job_type: row.try_get("job_type").map_err(db_err)?,
            status: row.try_get("status").map_err(db_err)?,
            requester_user_id: row.try_get("requester_user_id").map_err(db_err)?,
            ontology_version: row.try_get("ontology_version").map_err(db_err)?,
            context_policy: row.try_get("context_policy").map_err(db_err)?,
            risk_ceiling: row.try_get("risk_ceiling").map_err(db_err)?,
            correlation_id: row.try_get("correlation_id").map_err(db_err)?,
            created_at: row.try_get("created_at").map_err(db_err)?,
            started_at: row.try_get("started_at").map_err(db_err)?,
            finished_at: row.try_get("finished_at").map_err(db_err)?,
            cancelled_at: row.try_get("cancelled_at").map_err(db_err)?,
            error_code: row.try_get("error_code").map_err(db_err)?,
            error_message: row.try_get("error_message").map_err(db_err)?,
            timeout_ms: row.try_get("timeout_ms").map_err(db_err)?,
            lease_owner: row.try_get("lease_owner").map_err(db_err)?,
            lease_expires_at: row.try_get("lease_expires_at").map_err(db_err)?,
            last_heartbeat_at: row.try_get("last_heartbeat_at").map_err(db_err)?,
            attempt_count: row.try_get("attempt_count").map_err(db_err)?,
            max_attempts: row.try_get("max_attempts").map_err(db_err)?,
            expires_at: row.try_get("expires_at").map_err(db_err)?,
        })
    }
}

#[async_trait]
impl AiJobRepository for PgAiJobRepository {
    async fn insert(
        &self,
        job_id: &str,
        job_type: &str,
        requester_user_id: Option<&str>,
        correlation_id: Option<&str>,
        ontology_version: Option<&str>,
        risk_ceiling: Option<&str>,
    ) -> Result<AiJobRecord, AiJobRepositoryError> {
        let sql = format!(
            "INSERT INTO ai_jobs (job_id, job_type, status, requester_user_id, correlation_id, ontology_version, risk_ceiling)
             VALUES ($1, $2, 'pending', $3, $4, $5, $6)
             RETURNING {JOB_SELECT}"
        );
        let row = sqlx::query(&sql)
            .bind(job_id)
            .bind(job_type)
            .bind(requester_user_id)
            .bind(correlation_id)
            .bind(ontology_version)
            .bind(risk_ceiling)
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Self::row_to_job(&row)
    }

    async fn find_by_id(&self, job_id: &str) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        let sql = format!("SELECT {JOB_SELECT} FROM ai_jobs WHERE job_id = $1");
        let row = sqlx::query(&sql)
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        match row {
            Some(r) => Ok(Some(Self::row_to_job(&r)?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        status_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError> {
        let rows = if let Some(status) = status_filter {
            let sql = format!(
                "SELECT {JOB_SELECT} FROM ai_jobs WHERE status = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            );
            sqlx::query(&sql)
                .bind(status)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
        } else {
            let sql = format!("SELECT {JOB_SELECT} FROM ai_jobs ORDER BY created_at DESC LIMIT $1 OFFSET $2");
            sqlx::query(&sql).bind(limit).bind(offset).fetch_all(&self.pool).await
        };
        let rows = rows.map_err(db_err)?;
        rows.iter().map(Self::row_to_job).collect()
    }

    async fn update_status(&self, job_id: &str, new_status: &str) -> Result<AiJobRecord, AiJobRepositoryError> {
        let sql = format!(
            "UPDATE ai_jobs SET
                 status = $2,
                 started_at = CASE WHEN $2 = 'running' AND started_at IS NULL THEN now() ELSE started_at END,
                 finished_at = CASE WHEN $2 IN ('succeeded', 'failed_terminal') THEN now() ELSE finished_at END,
                 cancelled_at = CASE WHEN $2 = 'cancelled' THEN now() ELSE cancelled_at END
             WHERE job_id = $1
             RETURNING {JOB_SELECT}"
        );
        let row = sqlx::query(&sql)
            .bind(job_id)
            .bind(new_status)
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        Self::row_to_job(&row)
    }

    async fn set_error_message(&self, job_id: &str, error_message: &str) -> Result<(), AiJobRepositoryError> {
        sqlx::query("UPDATE ai_jobs SET error_message = $2 WHERE job_id = $1")
            .bind(job_id)
            .bind(error_message)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn claim_pending(&self, job_type: Option<&str>) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        let row = if let Some(jt) = job_type {
            let sql = format!(
                "UPDATE ai_jobs SET status = 'claimed'
                 WHERE job_id = (
                     SELECT job_id FROM ai_jobs
                     WHERE status = 'pending' AND job_type = $1
                     ORDER BY created_at ASC LIMIT 1
                     FOR UPDATE SKIP LOCKED
                 )
                 RETURNING {JOB_SELECT}"
            );
            sqlx::query(&sql).bind(jt).fetch_optional(&self.pool).await
        } else {
            let sql = format!(
                "UPDATE ai_jobs SET status = 'claimed'
                 WHERE job_id = (
                     SELECT job_id FROM ai_jobs
                     WHERE status = 'pending'
                     ORDER BY created_at ASC LIMIT 1
                     FOR UPDATE SKIP LOCKED
                 )
                 RETURNING {JOB_SELECT}"
            );
            sqlx::query(&sql).fetch_optional(&self.pool).await
        };
        let row = row.map_err(db_err)?;
        match row {
            Some(r) => Ok(Some(Self::row_to_job(&r)?)),
            None => Ok(None),
        }
    }

    async fn count_by_status(&self) -> Result<Vec<AiJobStatusCount>, AiJobRepositoryError> {
        let rows = sqlx::query("SELECT status, COUNT(*) as count FROM ai_jobs GROUP BY status ORDER BY status")
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            out.push(AiJobStatusCount {
                status: row.try_get("status").map_err(db_err)?,
                count: row.try_get("count").map_err(db_err)?,
            });
        }
        Ok(out)
    }

    async fn lease_pending(
        &self,
        job_type: Option<&str>,
        lease_owner: &str,
        lease_seconds: i64,
    ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        let sql = if job_type.is_some() {
            format!(
                "UPDATE ai_jobs SET
                     status = 'claimed',
                     lease_owner = $2,
                     lease_expires_at = now() + make_interval(secs => $3::double precision),
                     last_heartbeat_at = now(),
                     attempt_count = attempt_count + 1
                 WHERE job_id = (
                     SELECT job_id FROM ai_jobs
                     WHERE status = 'pending' AND job_type = $1
                     ORDER BY created_at ASC LIMIT 1
                     FOR UPDATE SKIP LOCKED
                 )
                 RETURNING {JOB_SELECT}"
            )
        } else {
            format!(
                "UPDATE ai_jobs SET
                     status = 'claimed',
                     lease_owner = $2,
                     lease_expires_at = now() + make_interval(secs => $3::double precision),
                     last_heartbeat_at = now(),
                     attempt_count = attempt_count + 1
                 WHERE job_id = (
                     SELECT job_id FROM ai_jobs
                     WHERE status = 'pending'
                     ORDER BY created_at ASC LIMIT 1
                     FOR UPDATE SKIP LOCKED
                 )
                 RETURNING {JOB_SELECT}"
            )
        };
        let row = if job_type.is_some() {
            sqlx::query(&sql)
                .bind(job_type)
                .bind(lease_owner)
                .bind(lease_seconds as f64)
                .fetch_optional(&self.pool)
                .await
        } else {
            sqlx::query(&sql)
                .bind(lease_owner)
                .bind(lease_seconds as f64)
                .fetch_optional(&self.pool)
                .await
        };
        let row = row.map_err(db_err)?;
        match row {
            Some(r) => Ok(Some(Self::row_to_job(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_expired_leases(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError> {
        let sql = format!(
            "SELECT {JOB_SELECT} FROM ai_jobs
             WHERE status IN ('claimed', 'running')
               AND lease_expires_at IS NOT NULL
               AND lease_expires_at < $1
               AND attempt_count < max_attempts
             ORDER BY lease_expires_at ASC
             LIMIT $2"
        );
        let rows = sqlx::query(&sql)
            .bind(now)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        rows.iter().map(Self::row_to_job).collect()
    }

    async fn heartbeat(
        &self,
        job_id: &str,
        lease_owner: &str,
        lease_seconds: i64,
    ) -> Result<bool, AiJobRepositoryError> {
        let result = sqlx::query(
            "UPDATE ai_jobs SET
                 lease_expires_at = now() + make_interval(secs => $3::double precision),
                 last_heartbeat_at = now()
             WHERE job_id = $1
               AND lease_owner = $2
               AND status IN ('claimed', 'running')",
        )
        .bind(job_id)
        .bind(lease_owner)
        .bind(lease_seconds as f64)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(result.rows_affected() > 0)
    }

    async fn take_over(
        &self,
        job_id: &str,
        new_owner: &str,
        lease_seconds: i64,
    ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        let sql = format!(
            "UPDATE ai_jobs SET
                 lease_owner = $2,
                 lease_expires_at = now() + make_interval(secs => $3::double precision),
                 last_heartbeat_at = now(),
                 status = 'claimed'
             WHERE job_id = $1
               AND status IN ('claimed', 'running')
               AND (lease_expires_at IS NULL OR lease_expires_at < now())
             RETURNING {JOB_SELECT}"
        );
        let row = sqlx::query(&sql)
            .bind(job_id)
            .bind(new_owner)
            .bind(lease_seconds as f64)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        match row {
            Some(r) => Ok(Some(Self::row_to_job(&r)?)),
            None => Ok(None),
        }
    }
}

fn db_err(err: impl ToString) -> AiJobRepositoryError {
    AiJobRepositoryError::database(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_builds() {
        // Compile/link surface: PgAiJobRepository is constructible from PgPool type.
        let _ = std::any::type_name::<PgAiJobRepository>();
    }
}
