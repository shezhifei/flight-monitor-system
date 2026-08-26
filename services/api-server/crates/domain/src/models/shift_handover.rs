//! 交接班领域模型
//!
//! 对应 Python `src/domain/models/shift_handover.py`。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// 交接班事项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftHandoverItem {
    pub item_id: String,
    pub handover_id: String,
    pub item_type: String,
    pub title: String,
    pub detail: Option<String>,
    pub owner_user_id: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_mandatory: bool,
    #[serde(default)]
    pub acknowledged: bool,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 交接班记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftHandover {
    pub handover_id: String,
    pub shift_date: NaiveDate,
    pub shift_code: String,
    pub from_user_id: String,
    pub to_user_id: String,
    /// 该交接单所属岗位（席）账号 id。`from`/`to` 必须是个人；complete 核接班人密码后
    /// 把 `position_user_id` 的占用切到接班人（OccupySeat）。岗位不是人，不能作为 from/to。
    #[serde(default)]
    pub position_user_id: Option<String>,
    pub from_operator_name: Option<String>,
    pub from_operator_job_title: Option<String>,
    pub from_operator_label: Option<String>,
    pub to_operator_name: Option<String>,
    pub to_operator_job_title: Option<String>,
    pub to_operator_label: Option<String>,
    #[serde(default = "default_draft")]
    pub status: String,
    pub summary: Option<String>,
    #[serde(default = "default_medium")]
    pub risk_level: String,
    pub signed_at: Option<DateTime<Utc>>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub items: Vec<ShiftHandoverItem>,
}

fn default_true() -> bool {
    true
}
fn default_draft() -> String {
    "draft".to_string()
}
fn default_medium() -> String {
    "medium".to_string()
}
