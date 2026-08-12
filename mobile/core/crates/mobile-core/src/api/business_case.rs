//! Business case API wrappers (plan §0.5 BusinessCase).

use crate::client::ApiClient;
use crate::dto::business_case::{
    BusinessCase, BusinessCaseAppendAcknowledgement, BusinessCaseAppendRequest,
    BusinessCaseCreateRequest, BusinessCaseListItemEnvelope, BusinessCaseType,
    BusinessCaseWorkflowRunDetail, BusinessCaseWorkflowStartData,
    BusinessCaseWorkflowStartRequest,
};
use crate::error::CoreError;

/// `GET /api/v2/business-cases` — array of envelopes; unwraps `data`.
pub async fn business_cases(
    client: &ApiClient,
    status: Option<&str>,
    case_type: Option<&str>,
    flight_id: Option<&str>,
) -> Result<Vec<BusinessCase>, CoreError> {
    let mut q = vec![];
    if let Some(s) = status {
        q.push(format!("status={s}"));
    }
    if let Some(t) = case_type {
        q.push(format!("case_type={t}"));
    }
    if let Some(f) = flight_id {
        q.push(format!("flight_id={f}"));
    }
    let path = if q.is_empty() {
        "/api/v2/business-cases".to_string()
    } else {
        format!("/api/v2/business-cases?{}", q.join("&"))
    };
    let envelopes: Vec<BusinessCaseListItemEnvelope> = client
        .call_raw("GET", &path, Option::<&()>::None)
        .await?;
    Ok(envelopes.into_iter().filter_map(|e| e.data).collect())
}

/// `GET /api/v2/business-cases/{id}`.
pub async fn business_case_detail(
    client: &ApiClient,
    id: &str,
) -> Result<BusinessCase, CoreError> {
    client
        .call_with_envelope(
            "GET",
            &format!("/api/v2/business-cases/{id}"),
            Option::<&()>::None,
        )
        .await
}

/// `POST /api/v2/business-cases`.
pub async fn create_business_case(
    client: &ApiClient,
    case_type: &str,
    flight_id: &str,
    description: &str,
    visibility_scope: &str,
) -> Result<BusinessCase, CoreError> {
    client
        .call_with_envelope(
            "POST",
            "/api/v2/business-cases",
            Some(&BusinessCaseCreateRequest {
                case_type: case_type.to_string(),
                flight_id: flight_id.to_string(),
                description: description.to_string(),
                visibility_scope: visibility_scope.to_string(),
            }),
        )
        .await
}

/// `POST /api/v2/business-cases/{id}/appends`.
pub async fn append_business_case(
    client: &ApiClient,
    case_id: &str,
    content: &str,
) -> Result<BusinessCase, CoreError> {
    client
        .call_with_envelope(
            "POST",
            &format!("/api/v2/business-cases/{case_id}/appends"),
            Some(&BusinessCaseAppendRequest {
                content: content.to_string(),
                mention_user_ids: vec![],
            }),
        )
        .await
}

/// `POST .../appends/{append_id}/acknowledge`.
pub async fn ack_append(
    client: &ApiClient,
    case_id: &str,
    append_id: &str,
) -> Result<BusinessCaseAppendAcknowledgement, CoreError> {
    client
        .call_with_envelope(
            "POST",
            &format!(
                "/api/v2/business-cases/{case_id}/appends/{append_id}/acknowledge"
            ),
            Option::<&()>::None,
        )
        .await
}

/// `GET /api/v2/business-case-types`.
pub async fn business_case_types(
    client: &ApiClient,
    active_only: bool,
) -> Result<Vec<BusinessCaseType>, CoreError> {
    client
        .call_with_envelope(
            "GET",
            &format!("/api/v2/business-case-types?active_only={active_only}"),
            Option::<&()>::None,
        )
        .await
}

/// `POST /api/v2/business-case-workflows/{template_code}/start`.
pub async fn start_case_workflow(
    client: &ApiClient,
    template_code: &str,
    flight_id: &str,
    description: &str,
) -> Result<BusinessCaseWorkflowStartData, CoreError> {
    client
        .call_with_envelope(
            "POST",
            &format!("/api/v2/business-case-workflows/{template_code}/start"),
            Some(&BusinessCaseWorkflowStartRequest {
                flight_id: flight_id.to_string(),
                description: description.to_string(),
            }),
        )
        .await
}

/// `GET /api/v2/business_cases/{case_id}/workflow` (underscore path — backend).
pub async fn case_workflow(
    client: &ApiClient,
    case_id: &str,
) -> Result<BusinessCaseWorkflowRunDetail, CoreError> {
    client
        .call_with_envelope(
            "GET",
            &format!("/api/v2/business_cases/{case_id}/workflow"),
            Option::<&()>::None,
        )
        .await
}
