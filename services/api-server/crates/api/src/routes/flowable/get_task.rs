use super::*;

pub(crate) async fn get_task(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let task_id = path.into_inner();
    let Some(payload) = svc.get_task(&task_id).await.map_err(map_service_error)? else {
        return Ok(raw_detail_message(actix_web::http::StatusCode::NOT_FOUND, "任务未找到"));
    };
    Ok(ok_resp(payload, "成功获取任务"))
}

pub(crate) async fn claim_task(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<ClaimTaskRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_ACT)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let task_id = path.into_inner();
    let claimed = svc
        .claim_task(&task_id, &body.user_id)
        .await
        .map_err(map_service_error)?;
    if !claimed {
        return Ok(raw_detail_message(actix_web::http::StatusCode::NOT_FOUND, "任务未找到"));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("任务 {task_id} 认领成功")
    })))
}

pub(crate) async fn unclaim_task(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_ACT)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let task_id = path.into_inner();
    let unclaimed = svc.unclaim_task(&task_id).await.map_err(map_service_error)?;
    if !unclaimed {
        return Ok(raw_detail_message(actix_web::http::StatusCode::NOT_FOUND, "任务未找到"));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("任务 {task_id} 取消认领成功")
    })))
}

pub(crate) async fn complete_task(
    svc: Option<web::Data<Arc<FlowableService>>>,
    workflow_svc: Option<web::Data<Arc<BusinessCaseWorkflowService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<CompleteTaskRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_ACT)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let task_id = path.into_inner();
    let process_instance_id = svc
        .get_task(&task_id)
        .await
        .map_err(map_service_error)?
        .and_then(|task| {
            task.get("processInstanceId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });
    let completed = svc
        .complete_task(&task_id, body.variables.as_ref())
        .await
        .map_err(map_service_error)?;
    if !completed {
        return Ok(raw_detail_message(actix_web::http::StatusCode::NOT_FOUND, "任务未找到"));
    }
    if let Some(process_instance_id) = process_instance_id {
        if let Some(workflow_svc) = workflow_svc.as_ref() {
            if let Err(error) = workflow_svc
                .continue_dispatch_tasks(&process_instance_id, None, false)
                .await
            {
                tracing::warn!(
                    task_id,
                    process_instance_id,
                    error = %error,
                    "dispatch task continuation after manual complete failed"
                );
            }
        }
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("任务 {task_id} 完成成功")
    })))
}

pub(crate) async fn start_process_with_subprocess(
    svc: Option<web::Data<Arc<FlowableService>>>,
    claims: JwtAuth,
    body: web::Json<StartProcessWithSubprocessRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_START)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let Some(process_instance_id) = svc
        .start_process_with_subprocess(&body.process_key, body.business_key.as_deref(), body.variables.as_ref())
        .await
        .map_err(map_service_error)?
    else {
        return Ok(missing_process_instance_response(
            "启动主流程失败: Flowable 未返回流程实例ID",
        ));
    };
    Ok(ok_resp(
        serde_json::json!({ "process_instance_id": process_instance_id }),
        "包含子流程的主流程启动成功",
    ))
}

pub(crate) async fn get_executions(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let payload = svc
        .get_subprocess_executions(&path.into_inner())
        .await
        .map_err(map_service_error)?;
    Ok(ok_resp(payload, "成功获取流程执行树"))
}

pub(crate) async fn get_subprocess_result(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let process_instance_id = path.into_inner();
    let Some(payload) = svc
        .get_subprocess_result(&process_instance_id)
        .await
        .map_err(map_service_error)?
    else {
        return Ok(raw_detail_message(
            actix_web::http::StatusCode::NOT_FOUND,
            "子流程实例未找到或尚未完成",
        ));
    };
    Ok(ok_resp(payload, "成功获取子流程结果"))
}

pub(crate) async fn get_variables(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let payload = svc
        .get_process_instance_variables(&path.into_inner())
        .await
        .map_err(map_service_error)?;
    Ok(ok_resp(payload, "成功获取流程实例变量"))
}

pub(crate) async fn set_variables(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<SetProcessInstanceVariablesRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_ACT)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let process_instance_id = path.into_inner();
    let updated = svc
        .set_process_instance_variables(&process_instance_id, &body.variables)
        .await
        .map_err(map_service_error)?;
    if !updated {
        return Ok(raw_detail_message(
            actix_web::http::StatusCode::NOT_FOUND,
            "流程实例未找到",
        ));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("流程实例 {process_instance_id} 变量设置成功")
    })))
}

pub(crate) async fn history_process_instances(
    svc: Option<web::Data<Arc<FlowableService>>>,
    query: web::Query<HistoricProcessInstancesQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let filter_items = [
        ("processDefinitionKey", query.process_definition_key.clone()),
        ("businessKey", query.business_key.clone()),
        ("startedBefore", query.start_time_before.clone()),
        ("startedAfter", query.start_time_after.clone()),
        ("finishedBefore", query.end_time_before.clone()),
        ("finishedAfter", query.end_time_after.clone()),
        ("startedBy", query.started_by.clone()),
    ];
    let filters = collect_filters(&filter_items);
    let payload = svc
        .list_historic_process_instances(&filters)
        .await
        .map_err(map_service_error)?;
    Ok(ok_resp(payload, "成功获取历史流程实例列表"))
}

pub(crate) async fn history_tasks(
    svc: Option<web::Data<Arc<FlowableService>>>,
    query: web::Query<HistoricTasksQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let filter_items = [
        ("processInstanceId", query.process_instance_id.clone()),
        ("assignee", query.assignee.clone()),
        ("owner", query.owner.clone()),
        ("taskDefinitionKey", query.task_definition_key.clone()),
        ("createdBefore", query.start_time_before.clone()),
        ("createdAfter", query.start_time_after.clone()),
        ("completedBefore", query.end_time_before.clone()),
        ("completedAfter", query.end_time_after.clone()),
    ];
    let filters = collect_filters(&filter_items);
    let payload = svc.list_historic_tasks(&filters).await.map_err(map_service_error)?;
    Ok(ok_resp(payload, "成功获取历史任务列表"))
}

pub(crate) async fn flowable_health(
    svc: Option<web::Data<Arc<FlowableService>>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_DEFINITION_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    match svc.health().await {
        Ok(payload) => Ok(ok_resp(payload, "Flowable服务健康检查通过")),
        Err(_error) => Ok(flowable_health_error_response()),
    }
}
