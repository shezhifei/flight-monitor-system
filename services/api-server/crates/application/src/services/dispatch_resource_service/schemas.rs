#[derive(Debug, serde::Deserialize)]
pub struct PageQuery {
    pub include_inactive: Option<bool>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct QualificationLevelsQuery {
    pub qualification_code: Option<String>,
    pub include_inactive: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub struct QualificationGrantsQuery {
    pub user_ids: Option<String>,
    pub include_inactive: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TaskTypeRequirementVersionsQuery {
    pub task_type: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RuleStatusQuery {
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ScheduleTemplatesQuery {
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub enabled: Option<bool>,
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ScheduleInstancesQuery {
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ScheduleExceptionsQuery {
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ScheduleAvailabilityQuery {
    pub resource_type: String,
    pub planned_start_time: chrono::DateTime<chrono::Utc>,
    pub planned_end_time: chrono::DateTime<chrono::Utc>,
    pub terminal: Option<String>,
    pub resource_ids: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct AnalyticsWindowQuery {
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct AnalyticsBreakdownQuery {
    pub group_by: Option<String>,
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct AnalyticsTrendQuery {
    pub bucket: Option<String>,
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TeamListQuery {
    pub include_inactive: Option<bool>,
    pub team_type_id: Option<String>,
    pub terminal: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TeamDetailQuery {
    pub load_members: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TeamMembersQuery {
    pub include_inactive: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TeamStatusQuery {
    pub status: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct EquipmentListQuery {
    pub include_inactive: Option<bool>,
    pub equipment_type_id: Option<String>,
    pub terminal: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct EquipmentStatusQuery {
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct StandListQuery {
    pub terminal: Option<String>,
    pub include_inactive: Option<bool>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct StepListQuery {
    pub category: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OrderListQuery {
    pub flight_id: Option<String>,
    pub status: Option<String>,
    pub source: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TimelineQuery {
    pub view_mode: Option<String>,
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
    pub terminal: Option<String>,
    pub statuses: Option<String>,
    pub source: Option<String>,
    pub include_cancelled: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ConflictQuery {
    pub window_start: Option<chrono::DateTime<chrono::Utc>>,
    pub window_end: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CascadePreviewQuery {
    pub flight_id: String,
    pub task_type: String,
    pub delay_minutes: f64,
    pub scheduled_departure: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct MyOrdersQuery {
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OrderTimelineQuery {
    pub limit: Option<i64>,
}
