//! Shift handover exports (plan §4 交接班).

use mobile_core::dto::handover as core;

use super::runtime;

/// Mirror of `ShiftHandoverItem`.
pub struct HandoverItem {
    pub item_id: String,
    pub handover_id: String,
    pub item_type: String,
    pub title: String,
    pub detail: Option<String>,
    pub owner_user_id: Option<String>,
    pub due_at: Option<String>,
    pub is_mandatory: bool,
    pub acknowledged: bool,
    pub acknowledged_at: Option<String>,
    pub acknowledged_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<core::ShiftHandoverItem> for HandoverItem {
    fn from(i: core::ShiftHandoverItem) -> Self {
        Self {
            item_id: i.item_id,
            handover_id: i.handover_id,
            item_type: i.item_type,
            title: i.title,
            detail: i.detail,
            owner_user_id: i.owner_user_id,
            due_at: i.due_at,
            is_mandatory: i.is_mandatory,
            acknowledged: i.acknowledged,
            acknowledged_at: i.acknowledged_at,
            acknowledged_by: i.acknowledged_by,
            created_at: i.created_at,
            updated_at: i.updated_at,
        }
    }
}

/// Mirror of `ShiftHandover`.
pub struct Handover {
    pub handover_id: String,
    pub shift_date: String,
    pub shift_code: String,
    pub from_user_id: String,
    pub to_user_id: String,
    pub from_operator_name: Option<String>,
    pub from_operator_job_title: Option<String>,
    pub from_operator_label: Option<String>,
    pub to_operator_name: Option<String>,
    pub to_operator_job_title: Option<String>,
    pub to_operator_label: Option<String>,
    pub status: String,
    pub summary: Option<String>,
    pub risk_level: String,
    pub signed_at: Option<String>,
    pub submitted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<HandoverItem>,
}

impl From<core::ShiftHandover> for Handover {
    fn from(h: core::ShiftHandover) -> Self {
        Self {
            handover_id: h.handover_id,
            shift_date: h.shift_date,
            shift_code: h.shift_code,
            from_user_id: h.from_user_id,
            to_user_id: h.to_user_id,
            from_operator_name: h.from_operator_name,
            from_operator_job_title: h.from_operator_job_title,
            from_operator_label: h.from_operator_label,
            to_operator_name: h.to_operator_name,
            to_operator_job_title: h.to_operator_job_title,
            to_operator_label: h.to_operator_label,
            status: h.status,
            summary: h.summary,
            risk_level: h.risk_level,
            signed_at: h.signed_at,
            submitted_at: h.submitted_at,
            created_at: h.created_at,
            updated_at: h.updated_at,
            items: h.items.into_iter().map(Into::into).collect(),
        }
    }
}

/// `GET /api/v2/shift-handovers`.
pub async fn shift_handovers(
    status: Option<String>,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<Handover>> {
    let rt = runtime()?;
    let list = mobile_core::api::handover::shift_handovers(
        &rt.client,
        status.as_deref(),
        limit,
        offset,
    )
    .await?;
    Ok(list.into_iter().map(Into::into).collect())
}

/// `GET /api/v2/shift-handovers/{id}`.
pub async fn shift_handover_detail(id: String) -> anyhow::Result<Handover> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::handover::shift_handover_detail(&rt.client, &id)
            .await?
            .into(),
    )
}

/// `POST .../items/{item_id}/ack`.
pub async fn ack_handover_item(
    handover_id: String,
    item_id: String,
    acknowledged: bool,
) -> anyhow::Result<HandoverItem> {
    let rt = runtime()?;
    Ok(mobile_core::api::handover::ack_handover_item(
        &rt.client,
        &handover_id,
        &item_id,
        acknowledged,
    )
    .await?
    .into())
}

/// `POST .../{id}/ack` — whole-handover acknowledge.
pub async fn ack_handover(handover_id: String) -> anyhow::Result<Handover> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::handover::ack_handover(&rt.client, &handover_id)
            .await?
            .into(),
    )
}
