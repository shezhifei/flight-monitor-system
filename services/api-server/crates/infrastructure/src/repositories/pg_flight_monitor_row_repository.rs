use async_trait::async_trait;
use chrono::NaiveDate;
use fms_domain::error::DomainError;
use fms_domain::models::flight_monitor_row::FlightMonitorRow;
use fms_domain::ports::flight_monitor_row_repository::{
    FlightMonitorRowQuery, FlightMonitorRowRepository, FlightMonitorRowTransactionalRepository,
};
use sqlx::{PgPool, Row};

pub struct PgFlightMonitorRowRepository {
    pool: PgPool,
}
impl PgFlightMonitorRowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUMNS: &str = "row_id, link_id, kind, inbound_flight_id, outbound_flight_id, inbound_flight_no, outbound_flight_no, inbound_scheduled_at, outbound_scheduled_at, inbound_estimated_at, outbound_estimated_at, inbound_actual_at, outbound_actual_at, inbound_station_code, outbound_station_code, inbound_is_vip, outbound_is_vip, registration, aircraft_type, stand_code, gate_code, terminal_code, baggage_carousel_code, status, workspace_date, sort_time, has_open_anomaly, version, updated_at";

fn map_row(row: sqlx::postgres::PgRow) -> FlightMonitorRow {
    FlightMonitorRow {
        row_id: row.get("row_id"),
        link_id: row.get("link_id"),
        kind: row.get("kind"),
        inbound_flight_id: row.get("inbound_flight_id"),
        outbound_flight_id: row.get("outbound_flight_id"),
        inbound_flight_no: row.get("inbound_flight_no"),
        outbound_flight_no: row.get("outbound_flight_no"),
        inbound_scheduled_at: row.get("inbound_scheduled_at"),
        outbound_scheduled_at: row.get("outbound_scheduled_at"),
        inbound_estimated_at: row.get("inbound_estimated_at"),
        outbound_estimated_at: row.get("outbound_estimated_at"),
        inbound_actual_at: row.get("inbound_actual_at"),
        outbound_actual_at: row.get("outbound_actual_at"),
        inbound_station_code: row.get("inbound_station_code"),
        outbound_station_code: row.get("outbound_station_code"),
        inbound_is_vip: row.get("inbound_is_vip"),
        outbound_is_vip: row.get("outbound_is_vip"),
        registration: row.get("registration"),
        aircraft_type: row.get("aircraft_type"),
        stand_code: row.get("stand_code"),
        gate_code: row.get("gate_code"),
        terminal_code: row.get("terminal_code"),
        baggage_carousel_code: row.get("baggage_carousel_code"),
        status: row.get("status"),
        workspace_date: row.get("workspace_date"),
        sort_time: row.get("sort_time"),
        has_open_anomaly: row.get("has_open_anomaly"),
        version: row.get("version"),
        updated_at: row.get("updated_at"),
    }
}

#[async_trait]
impl FlightMonitorRowRepository for PgFlightMonitorRowRepository {
    async fn list(
        &self,
        workspace_date: Option<NaiveDate>,
        query: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FlightMonitorRow>, DomainError> {
        let sql = format!("SELECT {COLUMNS} FROM flight_monitor_rows WHERE is_active AND ($1::date IS NULL OR workspace_date = $1) AND ($2::text IS NULL OR inbound_flight_no ILIKE $2 OR outbound_flight_no ILIKE $2) ORDER BY sort_time DESC NULLS LAST, row_id LIMIT $3 OFFSET $4");
        let rows = sqlx::query(&sql)
            .bind(workspace_date)
            .bind(query.map(|q| format!("%{q}%")))
            .bind(limit.max(1))
            .bind(offset.max(0))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(map_row).collect())
    }
    async fn count(&self, workspace_date: Option<NaiveDate>, query: Option<&str>) -> Result<i64, DomainError> {
        let row = sqlx::query("SELECT COUNT(*)::bigint AS count FROM flight_monitor_rows WHERE is_active AND ($1::date IS NULL OR workspace_date = $1) AND ($2::text IS NULL OR inbound_flight_no ILIKE $2 OR outbound_flight_no ILIKE $2)").bind(workspace_date).bind(query.map(|q| format!("%{q}%"))).fetch_one(&self.pool).await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.get("count"))
    }
    async fn upsert(&self, row: &FlightMonitorRow) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO flight_monitor_rows (row_id,link_id,kind,inbound_flight_id,outbound_flight_id,inbound_flight_no,outbound_flight_no,inbound_scheduled_at,outbound_scheduled_at,inbound_estimated_at,outbound_estimated_at,inbound_actual_at,outbound_actual_at,inbound_station_code,outbound_station_code,inbound_is_vip,outbound_is_vip,registration,aircraft_type,stand_code,gate_code,terminal_code,baggage_carousel_code,status,workspace_date,sort_time,has_open_anomaly,version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28) ON CONFLICT (row_id) DO UPDATE SET link_id=EXCLUDED.link_id,kind=EXCLUDED.kind,inbound_flight_id=EXCLUDED.inbound_flight_id,outbound_flight_id=EXCLUDED.outbound_flight_id,inbound_flight_no=EXCLUDED.inbound_flight_no,outbound_flight_no=EXCLUDED.outbound_flight_no,inbound_scheduled_at=EXCLUDED.inbound_scheduled_at,outbound_scheduled_at=EXCLUDED.outbound_scheduled_at,inbound_estimated_at=EXCLUDED.inbound_estimated_at,outbound_estimated_at=EXCLUDED.outbound_estimated_at,inbound_actual_at=EXCLUDED.inbound_actual_at,outbound_actual_at=EXCLUDED.outbound_actual_at,inbound_station_code=EXCLUDED.inbound_station_code,outbound_station_code=EXCLUDED.outbound_station_code,inbound_is_vip=EXCLUDED.inbound_is_vip,outbound_is_vip=EXCLUDED.outbound_is_vip,registration=EXCLUDED.registration,aircraft_type=EXCLUDED.aircraft_type,stand_code=EXCLUDED.stand_code,gate_code=EXCLUDED.gate_code,terminal_code=EXCLUDED.terminal_code,baggage_carousel_code=EXCLUDED.baggage_carousel_code,status=EXCLUDED.status,workspace_date=EXCLUDED.workspace_date,sort_time=EXCLUDED.sort_time,has_open_anomaly=EXCLUDED.has_open_anomaly,version=EXCLUDED.version,is_active=TRUE,updated_at=NOW()")
            .bind(&row.row_id).bind(&row.link_id).bind(&row.kind).bind(&row.inbound_flight_id).bind(&row.outbound_flight_id).bind(&row.inbound_flight_no).bind(&row.outbound_flight_no).bind(row.inbound_scheduled_at).bind(row.outbound_scheduled_at).bind(row.inbound_estimated_at).bind(row.outbound_estimated_at).bind(row.inbound_actual_at).bind(row.outbound_actual_at).bind(&row.inbound_station_code).bind(&row.outbound_station_code).bind(row.inbound_is_vip).bind(row.outbound_is_vip).bind(&row.registration).bind(&row.aircraft_type).bind(&row.stand_code).bind(&row.gate_code).bind(&row.terminal_code).bind(&row.baggage_carousel_code).bind(&row.status).bind(row.workspace_date).bind(row.sort_time).bind(row.has_open_anomaly).bind(row.version).execute(&self.pool).await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn deactivate_flight(&self, flight_id: &str) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE flight_monitor_rows
             SET inbound_flight_id = CASE WHEN inbound_flight_id = $1 THEN NULL ELSE inbound_flight_id END,
                 outbound_flight_id = CASE WHEN outbound_flight_id = $1 THEN NULL ELSE outbound_flight_id END,
                 inbound_flight_no = CASE WHEN inbound_flight_id = $1 THEN NULL ELSE inbound_flight_no END,
                 outbound_flight_no = CASE WHEN outbound_flight_id = $1 THEN NULL ELSE outbound_flight_no END,
                 kind = CASE
                     WHEN inbound_flight_id = $1 AND outbound_flight_id IS NOT NULL THEN 'single'
                     WHEN outbound_flight_id = $1 AND inbound_flight_id IS NOT NULL THEN 'single'
                     ELSE kind
                 END,
                 is_active = CASE
                     WHEN (inbound_flight_id = $1 AND outbound_flight_id IS NULL)
                       OR (outbound_flight_id = $1 AND inbound_flight_id IS NULL)
                     THEN FALSE ELSE is_active END,
                 updated_at = NOW()
             WHERE is_active AND (inbound_flight_id = $1 OR outbound_flight_id = $1)",
        )
        .bind(flight_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn refresh_anomaly_flag(&self, flight_id: &str) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE flight_monitor_rows r
             SET has_open_anomaly = EXISTS (
                 SELECT 1 FROM anomalies a
                 WHERE a.flight_id = $1
                   AND a.status IN ('open', 'acknowledged')
             ),
                 updated_at = NOW()
             WHERE r.is_active
               AND (r.inbound_flight_id = $1 OR r.outbound_flight_id = $1)",
        )
        .bind(flight_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn merge_turnaround(
        &self,
        link_id: &str,
        inbound_flight_id: &str,
        outbound_flight_id: &str,
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        <Self as FlightMonitorRowTransactionalRepository<sqlx::Transaction<'_, sqlx::Postgres>>>
            ::merge_turnaround_in_tx(self, &mut tx, link_id, inbound_flight_id, outbound_flight_id)
            .await?;
        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))
    }

    async fn search(
        &self,
        criteria: &FlightMonitorRowQuery,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FlightMonitorRow>, DomainError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM flight_monitor_rows
             WHERE is_active
               AND ($1::date IS NULL OR workspace_date = $1)
               AND ($2::text IS NULL OR inbound_flight_no ILIKE $2 OR outbound_flight_no ILIKE $2)
               AND ($3::text IS NULL OR status = $3)
               AND ($4::text IS NULL OR outbound_station_code ILIKE $4)
               AND ($5::text IS NULL OR inbound_station_code ILIKE $5)
               AND ($6::bool IS NULL OR has_open_anomaly = $6)
             ORDER BY sort_time DESC NULLS LAST, row_id LIMIT $7 OFFSET $8"
        ))
        .bind(criteria.workspace_date)
        .bind(criteria.query.as_deref().map(|q| format!("%{q}%")))
        .bind(criteria.status.as_deref())
        .bind(criteria.origin.as_deref().map(|q| format!("%{q}%")))
        .bind(criteria.destination.as_deref().map(|q| format!("%{q}%")))
        .bind(criteria.has_open_anomaly)
        .bind(limit.max(1))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(map_row).collect())
    }

    async fn count_filtered(&self, criteria: &FlightMonitorRowQuery) -> Result<i64, DomainError> {
        let row = sqlx::query(
            "SELECT COUNT(*)::bigint AS count FROM flight_monitor_rows
             WHERE is_active
               AND ($1::date IS NULL OR workspace_date = $1)
               AND ($2::text IS NULL OR inbound_flight_no ILIKE $2 OR outbound_flight_no ILIKE $2)
               AND ($3::text IS NULL OR status = $3)
               AND ($4::text IS NULL OR outbound_station_code ILIKE $4)
               AND ($5::text IS NULL OR inbound_station_code ILIKE $5)
               AND ($6::bool IS NULL OR has_open_anomaly = $6)",
        )
        .bind(criteria.workspace_date)
        .bind(criteria.query.as_deref().map(|q| format!("%{q}%")))
        .bind(criteria.status.as_deref())
        .bind(criteria.origin.as_deref().map(|q| format!("%{q}%")))
        .bind(criteria.destination.as_deref().map(|q| format!("%{q}%")))
        .bind(criteria.has_open_anomaly)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.get("count"))
    }
}

#[async_trait]
impl<'tx> FlightMonitorRowTransactionalRepository<sqlx::Transaction<'tx, sqlx::Postgres>>
    for PgFlightMonitorRowRepository
{
    async fn deactivate_flight_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        flight_id: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE flight_monitor_rows
             SET inbound_flight_id = CASE WHEN inbound_flight_id = $1 THEN NULL ELSE inbound_flight_id END,
                 outbound_flight_id = CASE WHEN outbound_flight_id = $1 THEN NULL ELSE outbound_flight_id END,
                 inbound_flight_no = CASE WHEN inbound_flight_id = $1 THEN NULL ELSE inbound_flight_no END,
                 outbound_flight_no = CASE WHEN outbound_flight_id = $1 THEN NULL ELSE outbound_flight_no END,
                 kind = CASE
                     WHEN inbound_flight_id = $1 AND outbound_flight_id IS NOT NULL THEN 'single'
                     WHEN outbound_flight_id = $1 AND inbound_flight_id IS NOT NULL THEN 'single'
                     ELSE kind
                 END,
                 is_active = CASE
                     WHEN (inbound_flight_id = $1 AND outbound_flight_id IS NULL)
                       OR (outbound_flight_id = $1 AND inbound_flight_id IS NULL)
                     THEN FALSE ELSE is_active END,
                 updated_at = NOW()
             WHERE is_active AND (inbound_flight_id = $1 OR outbound_flight_id = $1)",
        )
        .bind(flight_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn upsert_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        row: &FlightMonitorRow,
    ) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO flight_monitor_rows (row_id,link_id,kind,inbound_flight_id,outbound_flight_id,inbound_flight_no,outbound_flight_no,inbound_scheduled_at,outbound_scheduled_at,inbound_estimated_at,outbound_estimated_at,inbound_actual_at,outbound_actual_at,inbound_station_code,outbound_station_code,inbound_is_vip,outbound_is_vip,registration,aircraft_type,stand_code,gate_code,terminal_code,baggage_carousel_code,status,workspace_date,sort_time,has_open_anomaly,version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28) ON CONFLICT (row_id) DO UPDATE SET link_id=EXCLUDED.link_id,kind=EXCLUDED.kind,inbound_flight_id=EXCLUDED.inbound_flight_id,outbound_flight_id=EXCLUDED.outbound_flight_id,inbound_flight_no=EXCLUDED.inbound_flight_no,outbound_flight_no=EXCLUDED.outbound_flight_no,inbound_scheduled_at=EXCLUDED.inbound_scheduled_at,outbound_scheduled_at=EXCLUDED.outbound_scheduled_at,inbound_estimated_at=EXCLUDED.inbound_estimated_at,outbound_estimated_at=EXCLUDED.outbound_estimated_at,inbound_actual_at=EXCLUDED.inbound_actual_at,outbound_actual_at=EXCLUDED.outbound_actual_at,inbound_station_code=EXCLUDED.inbound_station_code,outbound_station_code=EXCLUDED.outbound_station_code,inbound_is_vip=EXCLUDED.inbound_is_vip,outbound_is_vip=EXCLUDED.outbound_is_vip,registration=EXCLUDED.registration,aircraft_type=EXCLUDED.aircraft_type,stand_code=EXCLUDED.stand_code,gate_code=EXCLUDED.gate_code,terminal_code=EXCLUDED.terminal_code,baggage_carousel_code=EXCLUDED.baggage_carousel_code,status=EXCLUDED.status,workspace_date=EXCLUDED.workspace_date,sort_time=EXCLUDED.sort_time,has_open_anomaly=EXCLUDED.has_open_anomaly,version=EXCLUDED.version,is_active=TRUE,updated_at=NOW()")
            .bind(&row.row_id).bind(&row.link_id).bind(&row.kind).bind(&row.inbound_flight_id).bind(&row.outbound_flight_id).bind(&row.inbound_flight_no).bind(&row.outbound_flight_no).bind(row.inbound_scheduled_at).bind(row.outbound_scheduled_at).bind(row.inbound_estimated_at).bind(row.outbound_estimated_at).bind(row.inbound_actual_at).bind(row.outbound_actual_at).bind(&row.inbound_station_code).bind(&row.outbound_station_code).bind(row.inbound_is_vip).bind(row.outbound_is_vip).bind(&row.registration).bind(&row.aircraft_type).bind(&row.stand_code).bind(&row.gate_code).bind(&row.terminal_code).bind(&row.baggage_carousel_code).bind(&row.status).bind(row.workspace_date).bind(row.sort_time).bind(row.has_open_anomaly).bind(row.version)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn merge_turnaround_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        link_id: &str,
        inbound_flight_id: &str,
        outbound_flight_id: &str,
    ) -> Result<(), DomainError> {
        let merged = sqlx::query(
            "UPDATE flight_monitor_rows AS inbound
             SET link_id = $1, kind = 'turnaround', outbound_flight_id = outbound.outbound_flight_id,
                 outbound_flight_no = outbound.outbound_flight_no,
                 outbound_scheduled_at = outbound.outbound_scheduled_at,
                 outbound_estimated_at = outbound.outbound_estimated_at,
                 outbound_actual_at = outbound.outbound_actual_at,
                 outbound_station_code = outbound.outbound_station_code,
                 outbound_is_vip = outbound.outbound_is_vip,
                 version = GREATEST(inbound.version, outbound.version), updated_at = NOW()
             FROM flight_monitor_rows AS outbound
             WHERE inbound.row_id = $2 AND outbound.row_id = $3",
        )
        .bind(link_id)
        .bind(inbound_flight_id)
        .bind(outbound_flight_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        if merged.rows_affected() == 0 {
            return Err(DomainError::Conflict(format!(
                "无法合并监控行：缺少 inbound/outbound row ({inbound_flight_id}, {outbound_flight_id})"
            )));
        }
        sqlx::query(
            "UPDATE flight_monitor_rows SET is_active = FALSE, link_id = $1, updated_at = NOW() WHERE row_id = $2 AND row_id <> $3",
        )
        .bind(link_id)
        .bind(outbound_flight_id)
        .bind(inbound_flight_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn break_turnaround_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'tx, sqlx::Postgres>,
        link_id: &str,
        inbound_flight_id: &str,
        outbound_flight_id: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE flight_monitor_rows
             SET is_active = TRUE, link_id = NULL, kind = 'single',
                 inbound_flight_id = NULL, inbound_flight_no = NULL,
                 inbound_scheduled_at = NULL, inbound_estimated_at = NULL,
                 inbound_actual_at = NULL, inbound_station_code = NULL,
                 inbound_is_vip = FALSE, updated_at = NOW()
             WHERE row_id = $1 AND (link_id = $2 OR link_id IS NULL)",
        )
        .bind(outbound_flight_id)
        .bind(link_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        let cleared = sqlx::query(
            "UPDATE flight_monitor_rows
             SET link_id = NULL, kind = 'single', outbound_flight_id = NULL,
                 outbound_flight_no = NULL, outbound_scheduled_at = NULL,
                 outbound_estimated_at = NULL, outbound_actual_at = NULL,
                 outbound_station_code = NULL, outbound_is_vip = FALSE, updated_at = NOW()
             WHERE row_id = $1 AND link_id = $2",
        )
        .bind(inbound_flight_id)
        .bind(link_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        if cleared.rows_affected() == 0 {
            return Err(DomainError::Conflict(format!(
                "无法拆分监控行：inbound row {inbound_flight_id} 不属于 link {link_id}"
            )));
        }
        Ok(())
    }
}
