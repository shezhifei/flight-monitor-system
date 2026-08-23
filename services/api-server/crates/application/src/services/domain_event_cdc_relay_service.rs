use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use fms_domain::ports::message_queue::MessageQueue;
use pgwire_replication::{Lsn, ReplicationClient, ReplicationConfig, ReplicationEvent, SslMode, TlsConfig};
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use fms_domain::error::DomainError;
use fms_domain::pgoutput_decoder::{PgOutputDecoder, PgOutputInsert, PgOutputMessage};
use fms_infrastructure::cdc::PgCdcAdmin;

use crate::services::domain_event_outbox_delivery::{
    event_type_metric_label, DomainEventOutboxDelivery, DomainEventOutboxRow,
};
use crate::sqlx_transactional_repositories::SqlxDomainEventOutboxTransactionalRepository;

const OUTBOX_RELATION_NAME: &str = "public.domain_event_outbox";
const PUBLISHED_TOTAL_METRIC: &str = "domain_event_relay_published_total";
const PUBLISH_FAILED_TOTAL_METRIC: &str = "domain_event_relay_publish_failed_total";
const CDC_RECONNECT_TOTAL_METRIC: &str = "domain_event_cdc_reconnect_total";
const CDC_DECODE_FAILED_TOTAL_METRIC: &str = "domain_event_cdc_decode_failed_total";
const CDC_LAST_COMMIT_LSN_METRIC: &str = "domain_event_cdc_last_commit_lsn";

/// 解析 outbox `occurred_at`。pgoutput 文本协议下发的是 PostgreSQL
/// timestamptz 文本格式（`2026-08-12 21:57:15.408335+08`，空格分隔、无 'T'），
/// 而历史数据/测试也可能是 RFC3339（`2026-03-27T12:30:00Z`）——两种都兼容，
/// 避免单行解码失败卡死整个复制流。
fn parse_outbox_occurred_at(raw: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    if let Ok(value) = DateTime::parse_from_rfc3339(raw) {
        return Ok(value.with_timezone(&Utc));
    }
    DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f%#z")
        .or_else(|_| DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f%:z"))
        .map(|value| value.with_timezone(&Utc))
}

#[derive(Debug, Clone)]
pub struct DomainEventCdcConfig {
    publication_name: String,
    slot_name: String,
    status_interval_seconds: u64,
    reconnect_backoff_seconds: u64,
}

impl DomainEventCdcConfig {
    pub fn new(
        publication_name: impl Into<String>,
        slot_name: impl Into<String>,
        status_interval_seconds: i64,
        reconnect_backoff_seconds: i64,
    ) -> Result<Self, DomainError> {
        let publication_name = validate_identifier(publication_name.into(), "publication_name")?;
        let slot_name = validate_identifier(slot_name.into(), "slot_name")?;

        Ok(Self {
            publication_name,
            slot_name,
            status_interval_seconds: status_interval_seconds.max(1) as u64,
            reconnect_backoff_seconds: reconnect_backoff_seconds.max(1) as u64,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ReplicationDatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub ssl_mode: String,
    pub ssl_root_cert: Option<String>,
    pub ssl_sni_hostname: Option<String>,
    pub ssl_client_cert: Option<String>,
    pub ssl_client_key: Option<String>,
}

#[derive(Debug, Default)]
struct DomainEventCdcLifecycle {
    handle: Option<JoinHandle<()>>,
    stop_tx: Option<watch::Sender<bool>>,
}

pub struct DomainEventCdcRelayService {
    pool: PgPool,
    cdc_admin: PgCdcAdmin,
    message_queue: Option<Arc<dyn MessageQueue + Send + Sync>>,
    enabled: bool,
    delivery: DomainEventOutboxDelivery,
    config: DomainEventCdcConfig,
    replication_db_config: ReplicationDatabaseConfig,
    lifecycle: Mutex<DomainEventCdcLifecycle>,
}

impl DomainEventCdcRelayService {
    pub fn new(
        pool: PgPool,
        message_queue: Option<Arc<dyn MessageQueue + Send + Sync>>,
        enabled: bool,
        topic: Option<String>,
        base_backoff_seconds: i64,
        config: DomainEventCdcConfig,
        replication_db_config: ReplicationDatabaseConfig,
        outbox_repo: Arc<dyn SqlxDomainEventOutboxTransactionalRepository>,
    ) -> Self {
        Self {
            pool: pool.clone(),
            cdc_admin: PgCdcAdmin::new(pool),
            message_queue,
            enabled,
            delivery: DomainEventOutboxDelivery::new(base_backoff_seconds, topic, outbox_repo),
            config,
            replication_db_config,
            lifecycle: Mutex::new(DomainEventCdcLifecycle::default()),
        }
    }

    pub fn topic(&self) -> &str {
        self.delivery.topic()
    }

    pub async fn start(self: &Arc<Self>) -> Result<(), DomainError> {
        if !self.enabled {
            info!("Domain event CDC relay disabled by config");
            return Ok(());
        }

        self.ensure_publication_exists().await?;
        self.ensure_replication_slot().await?;

        let mut lifecycle = self.lifecycle.lock().await;
        let already_running = lifecycle
            .handle
            .as_ref()
            .map(|handle| !handle.is_finished())
            .unwrap_or(false);
        if already_running {
            return Ok(());
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        let service = Arc::clone(self);
        let handle = tokio::spawn(async move {
            service.run_forever(stop_rx).await;
        });

        lifecycle.stop_tx = Some(stop_tx);
        lifecycle.handle = Some(handle);

        info!(
            publication = %self.config.publication_name,
            slot = %self.config.slot_name,
            database = %self.replication_db_config.database,
            stream = %self.delivery.topic(),
            "Domain event CDC relay started"
        );

        Ok(())
    }

    pub async fn stop(&self) -> Result<(), DomainError> {
        let (stop_tx, handle) = {
            let mut lifecycle = self.lifecycle.lock().await;
            (lifecycle.stop_tx.take(), lifecycle.handle.take())
        };

        if let Some(stop_tx) = stop_tx {
            let _ = stop_tx.send(true);
        }

        if let Some(handle) = handle {
            handle
                .await
                .map_err(|error| DomainError::Internal(format!("failed to join CDC runtime: {error}")))?;
        }

        Ok(())
    }

    async fn run_forever(self: Arc<Self>, mut stop_rx: watch::Receiver<bool>) {
        while !*stop_rx.borrow() {
            match self.stream_changes(&mut stop_rx).await {
                Ok(()) => break,
                Err(error) => {
                    if *stop_rx.borrow() {
                        break;
                    }
                    metrics::counter!(CDC_RECONNECT_TOTAL_METRIC).increment(1);
                    warn!(error = %error, "Domain event CDC relay stream failed");
                    let sleep = tokio::time::sleep(Duration::from_secs(self.config.reconnect_backoff_seconds));
                    tokio::pin!(sleep);
                    tokio::select! {
                        _ = &mut sleep => {}
                        _ = stop_rx.changed() => break,
                    }
                }
            }
        }
    }

    async fn stream_changes(&self, stop_rx: &mut watch::Receiver<bool>) -> Result<(), DomainError> {
        let mut client = ReplicationClient::connect(self.build_replication_config()?)
            .await
            .map_err(|error| DomainError::Internal(format!("failed to start logical replication stream: {error}")))?;
        let mut decoder = PgOutputDecoder::default();

        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    client.shutdown().await.map_err(|error| {
                        DomainError::Internal(format!("failed to stop logical replication stream: {error}"))
                    })?;
                    return Ok(());
                }
                event = client.recv() => {
                    let Some(event) = event.map_err(|error| {
                        DomainError::Internal(format!("failed to receive logical replication event: {error}"))
                    })? else {
                        return Ok(());
                    };

                    match event {
                        ReplicationEvent::KeepAlive { .. } | ReplicationEvent::Message { .. } => {}
                        ReplicationEvent::Begin { .. } => {}
                        ReplicationEvent::StoppedAt { .. } => return Ok(()),
                        ReplicationEvent::Commit { end_lsn, .. } => {
                            client.update_applied_lsn(end_lsn);
                            metrics::gauge!(CDC_LAST_COMMIT_LSN_METRIC).set(end_lsn.as_u64() as f64);
                        }
                        ReplicationEvent::XLogData { data, .. } => {
                            self.process_xlog_data(&mut decoder, data.as_ref()).await?;
                        }
                    }
                }
            }
        }
    }

    async fn process_xlog_data(&self, decoder: &mut PgOutputDecoder, payload: &[u8]) -> Result<(), DomainError> {
        let decoded = decoder.decode(payload).map_err(|error| {
            metrics::counter!(CDC_DECODE_FAILED_TOTAL_METRIC).increment(1);
            DomainError::Internal(format!("failed to decode pgoutput payload: {error}"))
        })?;

        match decoded {
            Some(PgOutputMessage::Insert(insert)) if insert.relation.full_name() == OUTBOX_RELATION_NAME => {
                let row = Self::coerce_outbox_row(&insert)?;
                self.deliver_row(&row).await?;
            }
            _ => {}
        }

        Ok(())
    }

    async fn deliver_row(&self, row: &DomainEventOutboxRow) -> Result<(), DomainError> {
        self.delivery.observe_relay_lag(row);

        let message_queue = self
            .message_queue
            .as_ref()
            .ok_or_else(|| DomainError::Internal("message queue gateway unavailable".to_string()))?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| DomainError::Internal(format!("failed to start CDC delivery transaction: {error}")))?;

        match self.delivery.publish_row(message_queue.as_ref(), row).await {
            Ok(()) => {
                metrics::counter!(
                    PUBLISHED_TOTAL_METRIC,
                    "event_type" => event_type_metric_label(row)
                )
                .increment(1);
                self.delivery
                    .mark_published(&mut tx, std::slice::from_ref(&row.event_id))
                    .await?;
            }
            Err(error) => {
                metrics::counter!(
                    PUBLISH_FAILED_TOTAL_METRIC,
                    "event_type" => event_type_metric_label(row)
                )
                .increment(1);
                self.delivery.mark_failed(&mut tx, row, &error.to_string()).await?;
            }
        }

        tx.commit()
            .await
            .map_err(|error| DomainError::Internal(format!("failed to commit CDC delivery transaction: {error}")))?;

        Ok(())
    }

    fn coerce_outbox_row(insert: &PgOutputInsert) -> Result<DomainEventOutboxRow, DomainError> {
        let values = &insert.values;
        let event_id = required_value(values, "event_id")?;
        let aggregate_type = optional_value(values, "aggregate_type");
        let aggregate_id = optional_value(values, "aggregate_id");
        let event_type = optional_value(values, "event_type");
        let payload = match required_value(values, "payload") {
            Ok(raw) => Value::String(raw),
            Err(error) => {
                metrics::counter!(CDC_DECODE_FAILED_TOTAL_METRIC).increment(1);
                return Err(error);
            }
        };
        let occurred_at_raw = required_value(values, "occurred_at")?;
        let occurred_at = parse_outbox_occurred_at(&occurred_at_raw)
            .map_err(|error| {
                metrics::counter!(CDC_DECODE_FAILED_TOTAL_METRIC).increment(1);
                DomainError::Internal(format!(
                    "failed to decode outbox occurred_at '{occurred_at_raw}': {error}"
                ))
            })?;
        let publish_attempts = values
            .get("publish_attempts")
            .and_then(|value| value.as_deref())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.parse::<i32>())
            .transpose()
            .map_err(|error| DomainError::Internal(format!("failed to decode outbox publish_attempts: {error}")))?
            .unwrap_or(0);
        let source_change_id = optional_value(values, "source_change_id");

        Ok(DomainEventOutboxRow {
            event_id,
            aggregate_type,
            aggregate_id,
            event_type,
            payload,
            occurred_at,
            publish_attempts,
            source_change_id,
        })
    }

    async fn ensure_publication_exists(&self) -> Result<(), DomainError> {
        self.cdc_admin
            .ensure_publication_exists(&self.config.publication_name)
            .await
    }

    async fn ensure_replication_slot(&self) -> Result<(), DomainError> {
        self.cdc_admin
            .ensure_replication_slot(&self.config.slot_name, &self.replication_db_config.database)
            .await
    }

    fn build_replication_config(&self) -> Result<ReplicationConfig, DomainError> {
        let mut config = ReplicationConfig::new(
            self.replication_db_config.host.clone(),
            self.replication_db_config.user.clone(),
            self.replication_db_config.password.clone(),
            self.replication_db_config.database.clone(),
            self.config.slot_name.clone(),
            self.config.publication_name.clone(),
        );
        config.port = self.replication_db_config.port;
        config.start_lsn = Lsn::ZERO;
        config.status_interval = Duration::from_secs(self.config.status_interval_seconds);
        config.idle_wakeup_interval = Duration::from_secs(self.config.status_interval_seconds);
        config.tls = build_tls_config(&self.replication_db_config)?;
        Ok(config)
    }
}

fn required_value(values: &HashMap<String, Option<String>>, field: &'static str) -> Result<String, DomainError> {
    values
        .get(field)
        .and_then(|value| value.clone())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            metrics::counter!(CDC_DECODE_FAILED_TOTAL_METRIC).increment(1);
            DomainError::Internal(format!("domain_event_outbox insert missing column: {field}"))
        })
}

fn optional_value(values: &HashMap<String, Option<String>>, field: &'static str) -> Option<String> {
    values
        .get(field)
        .and_then(|value| value.clone())
        .filter(|value| !value.trim().is_empty())
}

fn validate_identifier(value: String, field_name: &str) -> Result<String, DomainError> {
    let trimmed = value.trim();
    let is_valid = !trimmed.is_empty()
        && trimmed.chars().enumerate().all(|(index, ch)| match index {
            0 => ch == '_' || ch.is_ascii_alphabetic(),
            _ => ch == '_' || ch.is_ascii_alphanumeric(),
        });

    if is_valid {
        Ok(trimmed.to_string())
    } else {
        Err(DomainError::Internal(format!(
            "invalid PostgreSQL identifier for {field_name}: {value}"
        )))
    }
}

fn build_tls_config(config: &ReplicationDatabaseConfig) -> Result<TlsConfig, DomainError> {
    let mode = match config.ssl_mode.trim().to_ascii_lowercase().as_str() {
        "" | "disable" => SslMode::Disable,
        "prefer" => SslMode::Prefer,
        "require" => SslMode::Require,
        "verify-ca" => SslMode::VerifyCa,
        "verify-full" => SslMode::VerifyFull,
        other => {
            return Err(DomainError::Internal(format!(
                "unsupported replication ssl mode: {other}"
            )))
        }
    };

    Ok(TlsConfig {
        mode,
        ca_pem_path: config.ssl_root_cert.as_ref().map(PathBuf::from),
        sni_hostname: config
            .ssl_sni_hostname
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        client_cert_pem_path: config.ssl_client_cert.as_ref().map(PathBuf::from),
        client_key_pem_path: config.ssl_client_key.as_ref().map(PathBuf::from),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::{TimeZone, Utc};
    use pgwire_replication::SslMode;

    use super::{build_tls_config, validate_identifier, DomainEventCdcRelayService, ReplicationDatabaseConfig};
    use fms_domain::pgoutput_decoder::{PgOutputColumn, PgOutputInsert, PgOutputRelation};

    #[test]
    fn validates_identifiers_like_python() {
        assert_eq!(validate_identifier("fms_slot".to_string(), "slot").unwrap(), "fms_slot");
        assert!(validate_identifier("123bad".to_string(), "slot").is_err());
        assert!(validate_identifier("bad-name".to_string(), "slot").is_err());
    }

    #[test]
    fn builds_tls_config_from_replication_settings() {
        let tls = build_tls_config(&ReplicationDatabaseConfig {
            host: "127.0.0.1".to_string(),
            port: 5432,
            database: "flight_monitor".to_string(),
            user: "replicator".to_string(),
            password: "secret".to_string(),
            ssl_mode: "verify-full".to_string(),
            ssl_root_cert: Some("/tmp/root.pem".to_string()),
            ssl_sni_hostname: Some("db.internal".to_string()),
            ssl_client_cert: Some("/tmp/client.crt".to_string()),
            ssl_client_key: Some("/tmp/client.key".to_string()),
        })
        .unwrap();

        assert_eq!(tls.mode, SslMode::VerifyFull);
        assert_eq!(tls.sni_hostname.as_deref(), Some("db.internal"));
    }

    #[test]
    fn parse_outbox_occurred_at_accepts_pg_text_and_rfc3339() {
        // pgoutput 文本协议实际下发的 timestamptz 格式（曾因此卡死复制流）
        let pg_text = super::parse_outbox_occurred_at("2026-08-12 21:57:15.408335+08").unwrap();
        assert_eq!(pg_text.to_rfc3339(), "2026-08-12T13:57:15.408335+00:00");

        // 历史 RFC3339 格式保持兼容
        let rfc3339 = super::parse_outbox_occurred_at("2026-03-27T12:30:00Z").unwrap();
        assert_eq!(rfc3339, Utc.with_ymd_and_hms(2026, 3, 27, 12, 30, 0).unwrap());
    }

    #[test]
    fn coerce_outbox_row_decodes_insert_columns() {
        let insert = PgOutputInsert {
            relation: PgOutputRelation {
                relation_id: 1,
                namespace: "public".to_string(),
                relation_name: "domain_event_outbox".to_string(),
                columns: vec![PgOutputColumn {
                    name: "event_id".to_string(),
                    type_oid: 25,
                }],
            },
            values: HashMap::from([
                ("event_id".to_string(), Some("evt-1".to_string())),
                ("aggregate_type".to_string(), Some("flight".to_string())),
                ("aggregate_id".to_string(), Some("CA123".to_string())),
                ("event_type".to_string(), Some("flight.updated".to_string())),
                ("payload".to_string(), Some("{\"ok\":true}".to_string())),
                ("occurred_at".to_string(), Some("2026-03-27T12:30:00Z".to_string())),
                ("publish_attempts".to_string(), Some("3".to_string())),
                ("source_change_id".to_string(), Some("chg-1".to_string())),
            ]),
        };

        let row = DomainEventCdcRelayService::coerce_outbox_row(&insert).unwrap();
        assert_eq!(row.event_id, "evt-1");
        assert_eq!(row.publish_attempts, 3);
        assert_eq!(row.occurred_at, Utc.with_ymd_and_hms(2026, 3, 27, 12, 30, 0).unwrap());
    }
}
