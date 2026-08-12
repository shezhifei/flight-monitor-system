//! Business case exports (plan §4 事项).

use mobile_core::dto::business_case as core;

use super::runtime;

pub struct BusinessCaseAppend {
    pub append_id: String,
    pub case_id: String,
    pub content: String,
    pub submitted_by: String,
    pub submitted_operator_name: Option<String>,
    pub appended_at: String,
}

impl From<core::BusinessCaseAppendEntry> for BusinessCaseAppend {
    fn from(a: core::BusinessCaseAppendEntry) -> Self {
        Self {
            append_id: a.append_id,
            case_id: a.case_id,
            content: a.content,
            submitted_by: a.submitted_by,
            submitted_operator_name: a.submitted_operator_name,
            appended_at: a.appended_at,
        }
    }
}

pub struct BusinessCase {
    pub case_id: String,
    pub case_type: String,
    pub case_type_name: Option<String>,
    pub flight_id: String,
    pub flight_no: String,
    pub created_at: String,
    pub created_by: String,
    pub description: String,
    pub status: String,
    pub stand: Option<String>,
    pub gate: Option<String>,
    pub visibility_scope: String,
    pub department_name_snapshot: Option<String>,
    pub finished_at: Option<String>,
    pub append_count: i64,
    pub latest_append: Option<BusinessCaseAppend>,
    pub append_entries: Vec<BusinessCaseAppend>,
}

impl From<core::BusinessCase> for BusinessCase {
    fn from(c: core::BusinessCase) -> Self {
        Self {
            case_id: c.case_id,
            case_type: c.case_type,
            case_type_name: c.case_type_name,
            flight_id: c.flight_id,
            flight_no: c.flight_no,
            created_at: c.created_at,
            created_by: c.created_by,
            description: c.description,
            status: c.status,
            stand: c.stand,
            gate: c.gate,
            visibility_scope: c.visibility_scope,
            department_name_snapshot: c.department_name_snapshot,
            finished_at: c.finished_at,
            append_count: c.append_count,
            latest_append: c.latest_append.map(Into::into),
            append_entries: c.append_entries.into_iter().map(Into::into).collect(),
        }
    }
}

pub struct BusinessCaseType {
    pub id: String,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub visibility_scope: String,
}

impl From<core::BusinessCaseType> for BusinessCaseType {
    fn from(t: core::BusinessCaseType) -> Self {
        Self {
            id: t.id,
            code: t.code,
            name: t.name,
            description: t.description,
            is_active: t.is_active,
            visibility_scope: t.visibility_scope,
        }
    }
}

pub struct WorkflowRun {
    pub run_id: String,
    pub template_code: String,
    pub case_id: String,
    pub flight_id: String,
    pub process_instance_id: String,
    pub status: String,
    pub outcome: Option<String>,
    pub started_by: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<core::BusinessCaseWorkflowRun> for WorkflowRun {
    fn from(r: core::BusinessCaseWorkflowRun) -> Self {
        Self {
            run_id: r.run_id,
            template_code: r.template_code,
            case_id: r.case_id,
            flight_id: r.flight_id,
            process_instance_id: r.process_instance_id,
            status: r.status,
            outcome: r.outcome,
            started_by: r.started_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub struct WorkflowStartResult {
    pub process_instance_id: Option<String>,
    pub workflow_triggered: bool,
    pub receipt_group_id: Option<String>,
    pub business_case: Option<BusinessCase>,
    pub run: Option<WorkflowRun>,
}

impl From<core::BusinessCaseWorkflowStartData> for WorkflowStartResult {
    fn from(d: core::BusinessCaseWorkflowStartData) -> Self {
        Self {
            process_instance_id: d.process_instance_id,
            workflow_triggered: d.workflow_triggered,
            receipt_group_id: d.receipt_group_id,
            business_case: d.business_case.map(Into::into),
            run: d.run.map(Into::into),
        }
    }
}

pub struct WorkflowDetail {
    pub run: Option<WorkflowRun>,
    pub business_case: Option<BusinessCase>,
}

impl From<core::BusinessCaseWorkflowRunDetail> for WorkflowDetail {
    fn from(d: core::BusinessCaseWorkflowRunDetail) -> Self {
        Self {
            run: d.run.map(Into::into),
            business_case: d.business_case.map(Into::into),
        }
    }
}

pub async fn business_cases(
    status: Option<String>,
    case_type: Option<String>,
    flight_id: Option<String>,
) -> anyhow::Result<Vec<BusinessCase>> {
    let rt = runtime()?;
    let list = mobile_core::api::business_case::business_cases(
        &rt.client,
        status.as_deref(),
        case_type.as_deref(),
        flight_id.as_deref(),
    )
    .await?;
    Ok(list.into_iter().map(Into::into).collect())
}

pub async fn business_case_detail(id: String) -> anyhow::Result<BusinessCase> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::business_case::business_case_detail(&rt.client, &id)
            .await?
            .into(),
    )
}

pub async fn create_business_case(
    case_type: String,
    flight_id: String,
    description: String,
    visibility_scope: String,
) -> anyhow::Result<BusinessCase> {
    let rt = runtime()?;
    Ok(mobile_core::api::business_case::create_business_case(
        &rt.client,
        &case_type,
        &flight_id,
        &description,
        &visibility_scope,
    )
    .await?
    .into())
}

pub async fn append_business_case(
    case_id: String,
    content: String,
) -> anyhow::Result<BusinessCase> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::business_case::append_business_case(&rt.client, &case_id, &content)
            .await?
            .into(),
    )
}

pub async fn ack_append(case_id: String, append_id: String) -> anyhow::Result<()> {
    let rt = runtime()?;
    mobile_core::api::business_case::ack_append(&rt.client, &case_id, &append_id).await?;
    Ok(())
}

pub async fn business_case_types(active_only: bool) -> anyhow::Result<Vec<BusinessCaseType>> {
    let rt = runtime()?;
    let list =
        mobile_core::api::business_case::business_case_types(&rt.client, active_only).await?;
    Ok(list.into_iter().map(Into::into).collect())
}

pub async fn start_case_workflow(
    template_code: String,
    flight_id: String,
    description: String,
) -> anyhow::Result<WorkflowStartResult> {
    let rt = runtime()?;
    Ok(mobile_core::api::business_case::start_case_workflow(
        &rt.client,
        &template_code,
        &flight_id,
        &description,
    )
    .await?
    .into())
}

pub async fn case_workflow(case_id: String) -> anyhow::Result<WorkflowDetail> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::business_case::case_workflow(&rt.client, &case_id)
            .await?
            .into(),
    )
}
