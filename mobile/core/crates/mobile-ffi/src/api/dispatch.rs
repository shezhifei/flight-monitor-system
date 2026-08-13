//! Dispatch exports.
//!
//! Structured data crosses the bridge as frb-mirrored structs; the two
//! parameters that are JSON by contract (`action_json`, checklist `result`)
//! stay plain strings.

use mobile_core::api::dispatch::DispatchActionOutcome;
use mobile_core::dto::{dispatch as core_dispatch, mobile as core_mobile};

use super::runtime;

// ---------------------------------------------------------------------------
// Mirrors (mobile-core stays frb-free; see SignatureHeaders in mod.rs)
// ---------------------------------------------------------------------------

/// Mirror of `MobileWorkbenchOrderItem`.
pub struct WorkbenchOrderItem {
    pub order_id: String,
    pub flight_id: String,
    pub step_code: Option<String>,
    pub status: String,
    pub terminal: Option<String>,
    pub stand_id: Option<String>,
    pub gate: Option<String>,
    pub planned_start_time: Option<String>,
    pub planned_end_time: Option<String>,
    pub actual_start_time: Option<String>,
    pub assignment_deadline: Option<String>,
    pub supervisor_notified: bool,
}

impl From<core_mobile::MobileWorkbenchOrderItem> for WorkbenchOrderItem {
    fn from(i: core_mobile::MobileWorkbenchOrderItem) -> Self {
        Self {
            order_id: i.order_id,
            flight_id: i.flight_id,
            step_code: i.step_code,
            status: i.status,
            terminal: i.terminal,
            stand_id: i.stand_id,
            gate: i.gate,
            planned_start_time: i.planned_start_time,
            planned_end_time: i.planned_end_time,
            actual_start_time: i.actual_start_time,
            assignment_deadline: i.assignment_deadline,
            supervisor_notified: i.supervisor_notified,
        }
    }
}

/// Mirror of `MobileWorkbenchCounts`.
pub struct WorkbenchCounts {
    pub pending: i64,
    pub assigned: i64,
    pub in_progress: i64,
    pub completed: i64,
    pub cancelled: i64,
    pub total: i64,
}

impl From<core_mobile::MobileWorkbenchCounts> for WorkbenchCounts {
    fn from(c: core_mobile::MobileWorkbenchCounts) -> Self {
        Self {
            pending: c.pending,
            assigned: c.assigned,
            in_progress: c.in_progress,
            completed: c.completed,
            cancelled: c.cancelled,
            total: c.total,
        }
    }
}

/// Mirror of `MobileWorkbenchResponse`. `channel_recommendation` is a plain
/// `Map<String, bool>` on the Dart side.
pub struct Workbench {
    pub user_id: String,
    pub generated_at: String,
    pub my_orders: Vec<WorkbenchOrderItem>,
    pub order_counts: WorkbenchCounts,
    pub notification_unread_count: i64,
    pub chat_unread_total: i64,
    pub pending_shift_handover_count: i64,
    pub pending_sync_action_count: i64,
    pub channel_recommendation: std::collections::HashMap<String, bool>,
}

impl From<core_mobile::MobileWorkbenchResponse> for Workbench {
    fn from(w: core_mobile::MobileWorkbenchResponse) -> Self {
        Self {
            user_id: w.user_id,
            generated_at: w.generated_at,
            my_orders: w.my_orders.into_iter().map(Into::into).collect(),
            order_counts: w.order_counts.into(),
            notification_unread_count: w.notification_unread_count,
            chat_unread_total: w.chat_unread_total,
            pending_shift_handover_count: w.pending_shift_handover_count,
            pending_sync_action_count: w.pending_sync_action_count,
            channel_recommendation: w.channel_recommendation,
        }
    }
}

/// Mirror of `DispatchOrderItem`. The receipt summary is an arbitrary JSON
/// object server-side, so it crosses as a raw JSON string.
pub struct DispatchOrder {
    pub id: String,
    pub flight_id: String,
    pub step_code: Option<String>,
    pub status: String,
    pub terminal: Option<String>,
    pub stand_id: Option<String>,
    pub gate: Option<String>,
    pub estimated_completion_time: Option<String>,
    pub origin_type: String,
    pub origin_label: String,
    pub notification_receipt_summary_json: String,
}

impl From<core_dispatch::DispatchOrderItem> for DispatchOrder {
    fn from(o: core_dispatch::DispatchOrderItem) -> Self {
        Self {
            id: o.id,
            flight_id: o.flight_id,
            step_code: o.step_code,
            status: o.status,
            terminal: o.terminal,
            stand_id: o.stand_id,
            gate: o.gate,
            estimated_completion_time: o.estimated_completion_time,
            origin_type: o.origin_type,
            origin_label: o.origin_label,
            notification_receipt_summary_json:
                serde_json::to_string(&o.notification_receipt_summary).unwrap_or_default(),
        }
    }
}

/// Outcome of [`dispatch_action`] (mirror of
/// `mobile_core::api::dispatch::DispatchActionOutcome`).
pub enum DispatchActionResult {
    Sent,
    Queued,
}

impl From<DispatchActionOutcome> for DispatchActionResult {
    fn from(o: DispatchActionOutcome) -> Self {
        match o {
            DispatchActionOutcome::Sent => Self::Sent,
            DispatchActionOutcome::Queued => Self::Queued,
        }
    }
}

/// Mirror of `mobile_core::offline::SyncSummary`.
pub struct SyncSummary {
    pub total: i64,
    pub applied: i64,
    pub duplicates: i64,
    pub failed: i64,
    pub remaining: i64,
}

impl From<mobile_core::offline::SyncSummary> for SyncSummary {
    fn from(s: mobile_core::offline::SyncSummary) -> Self {
        Self {
            total: s.total as i64,
            applied: s.applied as i64,
            duplicates: s.duplicates as i64,
            failed: s.failed as i64,
            remaining: s.remaining as i64,
        }
    }
}

/// Mirror of `DispatchSafetyChecklistItemStatus`.
pub struct ChecklistItem {
    pub item_code: String,
    pub title: String,
    pub required: bool,
    pub allow_na: bool,
    pub order: i64,
    pub result: Option<String>,
    pub checked_by: Option<String>,
    pub checked_by_username: Option<String>,
    pub checked_at: Option<String>,
    pub note: Option<String>,
    pub status: String,
}

impl From<core_dispatch::DispatchSafetyChecklistItemStatus> for ChecklistItem {
    fn from(i: core_dispatch::DispatchSafetyChecklistItemStatus) -> Self {
        Self {
            item_code: i.item_code,
            title: i.title,
            required: i.required,
            allow_na: i.allow_na,
            order: i.order,
            result: i.result,
            checked_by: i.checked_by,
            checked_by_username: i.checked_by_username,
            checked_at: i.checked_at,
            note: i.note,
            status: i.status,
        }
    }
}

/// Mirror of `DispatchSafetyChecklistStatus`.
pub struct SafetyChecklist {
    pub dispatch_order_id: String,
    pub step_code: Option<String>,
    pub template_id: Option<String>,
    pub template_version: Option<String>,
    pub enforced: bool,
    pub ready: bool,
    pub required_total: i64,
    pub completed_required: i64,
    pub pending_required_items: Vec<String>,
    pub failed_required_items: Vec<String>,
    pub items: Vec<ChecklistItem>,
}

impl From<core_dispatch::DispatchSafetyChecklistStatus> for SafetyChecklist {
    fn from(s: core_dispatch::DispatchSafetyChecklistStatus) -> Self {
        Self {
            dispatch_order_id: s.dispatch_order_id,
            step_code: s.step_code,
            template_id: s.template_id,
            template_version: s.template_version,
            enforced: s.enforced,
            ready: s.ready,
            required_total: s.required_total,
            completed_required: s.completed_required,
            pending_required_items: s.pending_required_items,
            failed_required_items: s.failed_required_items,
            items: s.items.into_iter().map(Into::into).collect(),
        }
    }
}

/// Mirror of `DispatchSafetyChecklistRecord`.
pub struct ChecklistRecord {
    pub record_id: String,
    pub dispatch_order_id: String,
    pub item_code: String,
    pub result: String,
    pub checked_by: Option<String>,
    pub checked_by_username: Option<String>,
    pub checked_at: String,
    pub note: Option<String>,
    pub template_version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<core_dispatch::DispatchSafetyChecklistRecord> for ChecklistRecord {
    fn from(r: core_dispatch::DispatchSafetyChecklistRecord) -> Self {
        Self {
            record_id: r.record_id,
            dispatch_order_id: r.dispatch_order_id,
            item_code: r.item_code,
            result: r.result,
            checked_by: r.checked_by,
            checked_by_username: r.checked_by_username,
            checked_at: r.checked_at,
            note: r.note,
            template_version: r.template_version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// Mirror of `MobileUploadAsset` (`metadata` crosses as raw JSON string).
pub struct UploadAsset {
    pub upload_id: String,
    pub original_filename: String,
    pub content_type: Option<String>,
    pub file_size: i64,
    pub checksum_sha256: Option<String>,
    pub created_at: String,
    pub attachment_url: String,
    pub metadata_json: String,
}

impl From<core_mobile::MobileUploadAsset> for UploadAsset {
    fn from(a: core_mobile::MobileUploadAsset) -> Self {
        Self {
            upload_id: a.upload_id,
            original_filename: a.original_filename,
            content_type: a.content_type,
            file_size: a.file_size,
            checksum_sha256: a.checksum_sha256,
            created_at: a.created_at,
            attachment_url: a.attachment_url,
            metadata_json: serde_json::to_string(&a.metadata).unwrap_or_default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

/// `GET /api/v2/mobile/workbench`.
pub async fn workbench(
    pending_sync_count: i64,
    max_orders: i64,
) -> anyhow::Result<Workbench> {
    let rt = runtime()?;
    Ok(mobile_core::api::mobile::workbench(&rt.client, pending_sync_count, max_orders)
        .await?
        .into())
}

/// `GET /api/v2/dispatch-orders/my/assigned` (optional status filter).
pub async fn my_assigned_orders(status: Option<String>) -> anyhow::Result<Vec<DispatchOrder>> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::dispatch::my_assigned_orders(&rt.client, status.as_deref())
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )
}

/// Send one dispatch action, queueing offline on network-class errors only
///. `action_json` is `{"action_type": "...", "payload": {...}}` where
/// `action_type` is one of `accept|checkin|checkout|start|complete|
/// eta_report|report_issue` and `payload` the action-specific request body.
pub async fn dispatch_action(
    order_id: String,
    action_json: String,
) -> anyhow::Result<DispatchActionResult> {
    let rt = runtime()?;
    let parsed: serde_json::Value = serde_json::from_str(&action_json)?;
    let action = parsed
        .get("action_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("action_json missing action_type"))?
        .to_string();
    let payload = parsed
        .get("payload")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let payload_json = serde_json::to_string(&payload)?;
    Ok(
        mobile_core::api::dispatch::dispatch_action(
            &rt.client,
            &rt.offline,
            &order_id,
            &action,
            &payload_json,
        )
        .await?
        .into(),
    )
}

/// Replay the offline queue through
/// `POST /api/v2/dispatch-orders/mobile/sync/actions`.
pub async fn sync_offline_actions() -> anyhow::Result<SyncSummary> {
    let rt = runtime()?;
    Ok(rt.offline.sync_pending(&rt.client).await?.into())
}

/// `GET /api/v2/dispatch-orders/{order_id}/safety-checklist`.
pub async fn safety_checklist(order_id: String) -> anyhow::Result<SafetyChecklist> {
    let rt = runtime()?;
    Ok(mobile_core::api::dispatch::safety_checklist(&rt.client, &order_id)
        .await?
        .into())
}

/// `POST .../safety-checklist/items/{item_code}`. `result` is the plain
/// checklist verdict string (`pass` / `fail` / `na`), not JSON.
pub async fn submit_checklist_item(
    order_id: String,
    item_code: String,
    result: String,
) -> anyhow::Result<ChecklistRecord> {
    let rt = runtime()?;
    Ok(mobile_core::api::dispatch::submit_checklist_item(
        &rt.client,
        &order_id,
        &item_code,
        &result,
        None,
    )
    .await?
    .into())
}

/// Multipart upload `POST /api/v2/mobile/uploads`. Reads the file at `path`
/// and guesses the content type from its extension.
pub async fn upload_attachment(path: String, category: String) -> anyhow::Result<UploadAsset> {
    let rt = runtime()?;
    let bytes = tokio::fs::read(&path).await?;
    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload.bin")
        .to_string();
    let content_type = guess_content_type(&filename);
    Ok(rt
        .client
        .upload(&category, &filename, content_type, &bytes)
        .await?
        .into())
}

fn guess_content_type(filename: &str) -> &'static str {
    match filename.rsplit('.').next().map(|e| e.to_ascii_lowercase()) {
        Some(ext) => match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "mp4" => "video/mp4",
            "pdf" => "application/pdf",
            "txt" => "text/plain",
            _ => "application/octet-stream",
        },
        None => "application/octet-stream",
    }
}
