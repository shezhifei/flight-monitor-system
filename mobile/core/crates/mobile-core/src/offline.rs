//! SQLite offline action queue (plan §3.5).
//!
//! `pending_actions` DDL per plan §3.5. The DB file path is supplied at init
//! (Dart side: `getApplicationSupportDirectory`).
//!
//! Queue discipline:
//! - ONLY network-class errors enqueue ([`should_enqueue`]); HTTP 4xx/5xx
//!   (server-side rejections) never enter the queue;
//! - cap [`MAX_PENDING_ACTIONS`]: when full, the OLDEST row is dropped and a
//!   `tracing::warn` is emitted;
//! - sync replays through `POST /api/v2/dispatch-orders/mobile/sync/actions`:
//!   per-action verdict `applied`/`duplicate` → row deleted; `failed` → row
//!   kept with `retry_count + 1` and the server message in `last_error`.

use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::client::ApiClient;
use crate::dto::dispatch::{sync_status, DispatchSyncAction, DispatchSyncRequest, DispatchSyncResponse};
use crate::error::CoreError;

/// Hard cap on queued actions; oldest rows are dropped beyond this (§3.5).
pub const MAX_PENDING_ACTIONS: usize = 200;

const DDL: &str = "
CREATE TABLE IF NOT EXISTS pending_actions (
  client_action_id TEXT PRIMARY KEY,
  order_id         TEXT NOT NULL,
  action_type      TEXT NOT NULL,
  payload_json     TEXT NOT NULL,
  created_at       INTEGER NOT NULL,
  retry_count      INTEGER NOT NULL DEFAULT 0,
  last_error       TEXT
);";

/// One queued offline action (row of `pending_actions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAction {
    pub client_action_id: String,
    pub order_id: String,
    pub action_type: String,
    pub payload_json: String,
    pub created_at: i64,
    pub retry_count: i64,
    pub last_error: Option<String>,
}

/// Aggregate outcome of one [`OfflineQueue::sync_pending`] run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncSummary {
    pub total: usize,
    pub applied: usize,
    pub duplicates: usize,
    pub failed: usize,
    /// Rows still in the queue after the run.
    pub remaining: usize,
}

/// Only network-class errors are enqueueable (§3.5); HTTP 4xx/5xx and
/// serialization/auth failures are final and must not be queued.
pub fn should_enqueue(error: &CoreError) -> bool {
    matches!(error, CoreError::Network(_))
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// SQLite-backed offline queue. `rusqlite::Connection` is `!Sync`, so all
/// access is serialized through a mutex; operations are short.
pub struct OfflineQueue {
    conn: Mutex<Connection>,
}

impl OfflineQueue {
    /// Open (creating if needed) the queue database at `path` and ensure the
    /// schema exists.
    pub fn open(path: &str) -> Result<Self, CoreError> {
        let conn = Connection::open(path)
            .map_err(|e| CoreError::OfflineStore(format!("open {path}: {e}")))?;
        conn.execute_batch(DDL)
            .map_err(|e| CoreError::OfflineStore(format!("migrate: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// In-memory variant for tests.
    #[cfg(test)]
    fn open_in_memory() -> Result<Self, CoreError> {
        let conn =
            Connection::open_in_memory().map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        conn.execute_batch(DDL)
            .map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, CoreError> {
        self.conn
            .lock()
            .map_err(|_| CoreError::OfflineStore("queue mutex poisoned".to_string()))
    }

    /// Enqueue one action. When the queue is at [`MAX_PENDING_ACTIONS`], the
    /// oldest row is dropped first (with a warning).
    pub fn enqueue(
        &self,
        client_action_id: &str,
        order_id: &str,
        action_type: &str,
        payload_json: &str,
    ) -> Result<(), CoreError> {
        self.enqueue_at(client_action_id, order_id, action_type, payload_json, now_epoch())
    }

    fn enqueue_at(
        &self,
        client_action_id: &str,
        order_id: &str,
        action_type: &str,
        payload_json: &str,
        created_at: i64,
    ) -> Result<(), CoreError> {
        let conn = self.lock()?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM pending_actions", [], |r| r.get(0))
            .map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        if count as usize >= MAX_PENDING_ACTIONS {
            // Drop the oldest row (lowest created_at, tie-broken by rowid).
            let evicted: Option<String> = conn
                .query_row(
                    "DELETE FROM pending_actions WHERE client_action_id = (
                       SELECT client_action_id FROM pending_actions
                       ORDER BY created_at ASC, rowid ASC LIMIT 1
                     ) RETURNING client_action_id",
                    [],
                    |r| r.get(0),
                )
                .ok();
            tracing::warn!(
                evicted = evicted.as_deref().unwrap_or("<none>"),
                "offline queue full ({MAX_PENDING_ACTIONS}); dropped oldest action"
            );
        }
        conn.execute(
            "INSERT OR REPLACE INTO pending_actions
             (client_action_id, order_id, action_type, payload_json, created_at, retry_count, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL)",
            params![client_action_id, order_id, action_type, payload_json, created_at],
        )
        .map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        Ok(())
    }

    /// All queued actions, oldest first (replay order).
    pub fn pending(&self) -> Result<Vec<PendingAction>, CoreError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT client_action_id, order_id, action_type, payload_json, created_at, retry_count, last_error
                 FROM pending_actions ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PendingAction {
                    client_action_id: r.get(0)?,
                    order_id: r.get(1)?,
                    action_type: r.get(2)?,
                    payload_json: r.get(3)?,
                    created_at: r.get(4)?,
                    retry_count: r.get(5)?,
                    last_error: r.get(6)?,
                })
            })
            .map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| CoreError::OfflineStore(e.to_string()))?);
        }
        Ok(out)
    }

    pub fn len(&self) -> Result<usize, CoreError> {
        let conn = self.lock()?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM pending_actions", [], |r| r.get(0))
            .map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        Ok(count as usize)
    }

    pub fn is_empty(&self) -> Result<bool, CoreError> {
        Ok(self.len()? == 0)
    }

    /// Delete one row (after `applied`/`duplicate` verdicts).
    pub fn remove(&self, client_action_id: &str) -> Result<(), CoreError> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM pending_actions WHERE client_action_id = ?1",
            params![client_action_id],
        )
        .map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        Ok(())
    }

    /// Keep the row but record a failed attempt (`failed` verdict, §3.5).
    pub fn mark_failed(&self, client_action_id: &str, last_error: &str) -> Result<(), CoreError> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE pending_actions SET retry_count = retry_count + 1, last_error = ?2
             WHERE client_action_id = ?1",
            params![client_action_id, last_error],
        )
        .map_err(|e| CoreError::OfflineStore(e.to_string()))?;
        Ok(())
    }

    /// Replay the queue through `POST
    /// /api/v2/dispatch-orders/mobile/sync/actions` (§3.5). Verdicts:
    /// `applied`/`duplicate` → row deleted; `failed` → kept, `retry_count+1`.
    pub async fn sync_pending(&self, client: &ApiClient) -> Result<SyncSummary, CoreError> {
        let pending = self.pending()?;
        if pending.is_empty() {
            return Ok(SyncSummary::default());
        }
        let actions: Vec<DispatchSyncAction> = pending
            .iter()
            .map(|p| DispatchSyncAction {
                client_action_id: p.client_action_id.clone(),
                action_type: p.action_type.clone(),
                dispatch_order_id: p.order_id.clone(),
                action_timestamp: Some(p.created_at.to_string()),
                payload: serde_json::from_str(&p.payload_json).ok(),
            })
            .collect();
        let total = actions.len();
        let response: DispatchSyncResponse = client
            .call_with_envelope(
                "POST",
                "/api/v2/dispatch-orders/mobile/sync/actions",
                Some(&DispatchSyncRequest { actions }),
            )
            .await?;

        let mut summary = SyncSummary {
            total,
            ..SyncSummary::default()
        };
        for result in &response.results {
            match result.status.as_str() {
                sync_status::APPLIED => {
                    self.remove(&result.client_action_id)?;
                    summary.applied += 1;
                }
                sync_status::DUPLICATE => {
                    self.remove(&result.client_action_id)?;
                    summary.duplicates += 1;
                }
                _ => {
                    // failed (or unknown): keep the row, count the retry.
                    self.mark_failed(&result.client_action_id, &result.message)?;
                    summary.failed += 1;
                }
            }
        }
        summary.remaining = self.len()?;
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ApiClient;
    use crate::config::ApiConfig;
    use crate::session::{SessionManager, TokenBundle};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    #[test]
    fn enqueue_and_remove_roundtrip() {
        let queue = OfflineQueue::open_in_memory().unwrap();
        queue
            .enqueue_at("a1", "order-1", "accept", r#"{"note":null}"#, 100)
            .unwrap();
        queue
            .enqueue_at("a2", "order-1", "checkin", r#"{"lat":1.0}"#, 200)
            .unwrap();
        let pending = queue.pending().unwrap();
        assert_eq!(pending.len(), 2);
        // Oldest first.
        assert_eq!(pending[0].client_action_id, "a1");
        assert_eq!(pending[0].retry_count, 0);
        assert_eq!(pending[0].last_error, None);
        queue.remove("a1").unwrap();
        let pending = queue.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].client_action_id, "a2");
    }

    #[test]
    fn mark_failed_increments_retry_and_keeps_row() {
        let queue = OfflineQueue::open_in_memory().unwrap();
        queue.enqueue("a1", "o1", "accept", "{}").unwrap();
        queue.mark_failed("a1", "server said no").unwrap();
        queue.mark_failed("a1", "again").unwrap();
        let pending = queue.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].retry_count, 2);
        assert_eq!(pending[0].last_error.as_deref(), Some("again"));
    }

    #[test]
    fn cap_evicts_oldest_beyond_200() {
        let queue = OfflineQueue::open_in_memory().unwrap();
        for i in 0..MAX_PENDING_ACTIONS {
            queue
                .enqueue_at(&format!("a{i:03}"), "o", "accept", "{}", i as i64)
                .unwrap();
        }
        assert_eq!(queue.len().unwrap(), MAX_PENDING_ACTIONS);
        // One more → oldest (a000) evicted.
        queue
            .enqueue_at("a-new", "o", "accept", "{}", 10_000)
            .unwrap();
        assert_eq!(queue.len().unwrap(), MAX_PENDING_ACTIONS);
        let pending = queue.pending().unwrap();
        assert!(pending.iter().all(|p| p.client_action_id != "a000"));
        assert!(pending.iter().any(|p| p.client_action_id == "a-new"));
        assert_eq!(pending[0].client_action_id, "a001");
    }

    #[test]
    fn should_enqueue_only_network_errors() {
        assert!(should_enqueue(&CoreError::Network("x".into())));
        assert!(!should_enqueue(&CoreError::Api {
            message: "x".into(),
            request_id: None
        }));
        assert!(!should_enqueue(&CoreError::Auth("x".into())));
        assert!(!should_enqueue(&CoreError::Serialization("x".into())));
    }

    /// Mock sync server answering with a fixed envelope.
    async fn spawn_sync_mock(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let (read_half, mut write_half) = socket.split();
                    let mut reader = BufReader::new(read_half);
                    let mut content_length = 0usize;
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                            return;
                        }
                        let trimmed = line.trim_end();
                        if trimmed.is_empty() {
                            break;
                        }
                        if let Some((name, value)) = trimmed.split_once(':') {
                            if name.trim().eq_ignore_ascii_case("content-length") {
                                content_length = value.trim().parse().unwrap_or(0);
                            }
                        }
                    }
                    let mut buf = vec![0u8; content_length];
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut buf).await;
                    }
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = write_half.write_all(response.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn sync_handles_applied_duplicate_failed_branches() {
        // One verdict per branch (§3.5).
        let body = r#"{"success":true,"data":{"total":3,"applied":1,"duplicates":1,"failed":1,"results":[
            {"client_action_id":"act-applied","dispatch_order_id":"o1","action_type":"accept","status":"applied","message":"ok","server_timestamp":"2024-01-01T00:00:00Z"},
            {"client_action_id":"act-dup","dispatch_order_id":"o1","action_type":"checkin","status":"duplicate","message":"already","server_timestamp":"2024-01-01T00:00:00Z"},
            {"client_action_id":"act-fail","dispatch_order_id":"o2","action_type":"complete","status":"failed","message":"conflict","server_timestamp":"2024-01-01T00:00:00Z"}
        ]},"message":"ok","error":null,"request_id":"r1"}"#;
        let base = spawn_sync_mock(body).await;

        let queue = OfflineQueue::open_in_memory().unwrap();
        queue.enqueue("act-applied", "o1", "accept", "{}").unwrap();
        queue.enqueue("act-dup", "o1", "checkin", "{}").unwrap();
        queue.enqueue("act-fail", "o2", "complete", "{}").unwrap();

        let session = SessionManager::new();
        session
            .restore_tokens(TokenBundle {
                access_token: "a".into(),
                refresh_token: "r".into(),
                session_secret: "s".into(),
                access_expire_at: now_epoch() + 3600,
            })
            .await;
        let client = ApiClient::new(ApiConfig::new(base, true).unwrap(), session, "dev");

        let summary = queue.sync_pending(&client).await.unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.applied, 1);
        assert_eq!(summary.duplicates, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.remaining, 1);

        // applied/duplicate rows gone; failed row kept with retry_count+1.
        let pending = queue.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].client_action_id, "act-fail");
        assert_eq!(pending[0].retry_count, 1);
        assert_eq!(pending[0].last_error.as_deref(), Some("conflict"));
    }

    #[tokio::test]
    async fn sync_on_empty_queue_is_noop() {
        let queue = OfflineQueue::open_in_memory().unwrap();
        let session = SessionManager::new();
        let client = ApiClient::new(
            ApiConfig::new("http://127.0.0.1:1", true).unwrap(),
            session,
            "dev",
        );
        let summary = queue.sync_pending(&client).await.unwrap();
        assert_eq!(summary, SyncSummary::default());
    }

    #[test]
    fn file_backed_open_creates_schema() {
        let dir = std::env::temp_dir().join(format!("fms-offline-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("queue.db");
        std::fs::create_dir_all(&dir).unwrap();
        let queue = OfflineQueue::open(path.to_str().unwrap()).unwrap();
        queue.enqueue("a1", "o1", "accept", "{}").unwrap();
        assert_eq!(queue.len().unwrap(), 1);
        drop(queue);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
