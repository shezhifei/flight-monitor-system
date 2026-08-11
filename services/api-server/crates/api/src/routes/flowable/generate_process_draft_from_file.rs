use super::*;

/// Upload size limit is enforced by the global `PayloadConfig` in `main.rs`
/// (default 20 MB). Route-level duplicate checks are removed to avoid
/// inconsistent limits.
pub(crate) async fn generate_process_draft_from_file(
    svc: Option<web::Data<Arc<FlowableDraftService>>>,
    claims: JwtAuth,
    mut multipart: Multipart,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_DEFINITION_EDIT)?;
    ensure_grant(&claims, "ai:execute")?;
    let Some(svc) = svc else {
        return Ok(flowable_draft_service_unavailable());
    };

    let mut file_name: Option<String> = None;
    let mut file_bytes = Vec::new();
    let mut process_key: Option<String> = None;
    let mut process_name: Option<String> = None;
    let mut case_type_code: Option<String> = None;
    let mut locale: Option<String> = Some("zh-CN".to_string());

    while let Some(mut field) = multipart
        .try_next()
        .await
        .map_err(|error| ApiError::BadRequest(format!("读取上传表单失败: {error}")))?
    {
        let field_name = field
            .content_disposition()
            .and_then(|disposition| disposition.get_name())
            .map(str::to_string)
            .unwrap_or_default();

        let mut bytes = Vec::new();
        while let Some(chunk) = field.next().await {
            let chunk = chunk.map_err(|error| ApiError::BadRequest(format!("读取上传内容失败: {error}")))?;
            bytes.extend_from_slice(&chunk);
        }

        match field_name.as_str() {
            "file" => {
                file_name = field
                    .content_disposition()
                    .and_then(|disposition| disposition.get_filename())
                    .map(str::to_string)
                    .or_else(|| Some("uploaded_file".to_string()));
                file_bytes = bytes;
            }
            "process_key" => process_key = Some(String::from_utf8_lossy(&bytes).trim().to_string()),
            "process_name" => process_name = Some(String::from_utf8_lossy(&bytes).trim().to_string()),
            "case_type_code" => case_type_code = Some(String::from_utf8_lossy(&bytes).trim().to_string()),
            "locale" => locale = Some(String::from_utf8_lossy(&bytes).trim().to_string()),
            _ => {}
        }
    }

    if file_bytes.is_empty() {
        return Ok(detail_response(
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
            "FILE_REQUIRED",
            "请上传体系文件",
        ));
    }

    let filename = file_name.unwrap_or_else(|| "uploaded_file".to_string());
    match svc
        .generate_from_file(
            &filename,
            &file_bytes,
            process_key.as_deref().filter(|value| !value.is_empty()),
            process_name.as_deref().filter(|value| !value.is_empty()),
            case_type_code.as_deref().filter(|value| !value.is_empty()),
            locale.as_deref().filter(|value| !value.is_empty()),
        )
        .await
    {
        Ok(data) => Ok(ok_resp(data, "流程草案生成成功")),
        Err(error) => Ok(map_draft_error(error)),
    }
}

pub(crate) async fn chat_process_draft_assistant_stream(
    svc: Option<web::Data<Arc<FlowableDraftService>>>,
    claims: JwtAuth,
    body: web::Json<FlowableDraftAssistantChatRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_DEFINITION_READ)?;
    ensure_grant(&claims, "ai:chat")?;
    let Some(svc) = svc else {
        return Ok(flowable_draft_service_unavailable());
    };
    let request = body.into_inner();
    let request_id = request
        .request_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("req_{}", uuid::Uuid::new_v4()));
    // Use bounded channel with backpressure to prevent memory overflow
    const SSE_CHANNEL_CAPACITY: usize = 256;
    let (sender, receiver) = mpsc::channel::<String>(SSE_CHANNEL_CAPACITY);
    let (event_sender, mut event_receiver) = mpsc::channel::<FlowableDraftAssistantStreamEvent>(SSE_CHANNEL_CAPACITY);
    let connected_payload = json!({
        "request_id": request_id.clone(),
        "scene": "flowable_assistant",
        "message": "Flowable assistant stream connected",
    });
    let stream_request_id = request_id.clone();
    let user_id = claims.0.sub.as_deref().unwrap_or("unknown_user").to_string();

    spawn_tracked("flowable_assistant_stream", async move {
        let request_id_for_events = stream_request_id.clone();
        let event_forward_sender = sender.clone();
        let user_id_for_events = user_id.clone();
        let event_forwarder = spawn_tracked("flowable_event_forwarder", async move {
            while let Some(event) = event_receiver.recv().await {
                let sse = flowable_stream_event_to_sse(&request_id_for_events, &user_id_for_events, event);
                // Use try_send to apply backpressure; drop if channel is full
                let _ = event_forward_sender.try_send(sse);
            }
        });

        let result = svc
            .chat_assistant_with_stream(request, &user_id, Some(event_sender))
            .await;
        let _ = event_forwarder.await;
        match result {
            Ok(data) => {
                let _ = sender
                    .send(build_sse_event_string(
                        "final_result",
                        serde_json::to_value(&data).unwrap_or_else(|_| json!({})),
                    ))
                    .await;
            }
            Err(error) => {
                let _ = sender
                    .send(build_sse_event_string(
                        "error",
                        json!({
                            "request_id": stream_request_id,
                            "scene": "flowable_assistant",
                            "message": error.to_string(),
                        }),
                    ))
                    .await;
            }
        }
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(RouteSseStream {
            receiver,
            initial_events: VecDeque::from(vec![build_sse_event_string("connected", connected_payload)]),
        }))
}

pub(crate) async fn list_process_definitions(
    svc: Option<web::Data<Arc<FlowableService>>>,
    req: HttpRequest,
    query: web::Query<ProcessDefinitionsQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let tenant_id = resolve_requested_tenant(&req, &claims, query.tenant_id.as_deref())?;
    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_DEFINITION_READ,
        scope_from_tenant_id(&tenant_id),
    )?;
    let payload = svc
        .list_process_definitions(query.key.as_deref(), Some(&tenant_id))
        .await
        .map_err(map_service_error)?;
    Ok(ok_resp(payload, "成功获取流程定义列表"))
}

pub(crate) async fn get_process_definition(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_DEFINITION_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let process_definition_id = path.into_inner();
    let Some(payload) = svc
        .get_process_definition(&process_definition_id)
        .await
        .map_err(map_service_error)?
    else {
        return Ok(raw_detail_message(
            actix_web::http::StatusCode::NOT_FOUND,
            "流程定义未找到",
        ));
    };
    Ok(ok_resp(payload, "成功获取流程定义"))
}

pub(crate) async fn get_process_definition_xml(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_DEFINITION_READ)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let process_definition_id = path.into_inner();
    let Some(payload) = svc
        .get_process_definition_xml(&process_definition_id)
        .await
        .map_err(map_service_error)?
    else {
        return Ok(raw_detail_message(
            actix_web::http::StatusCode::NOT_FOUND,
            "流程定义XML未找到",
        ));
    };
    Ok(HttpResponse::Ok().content_type("application/xml").body(payload))
}

pub(crate) async fn create_deployment(
    svc: Option<web::Data<Arc<FlowableService>>>,
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<CreateDeploymentRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let Some(svc) = svc else {
        return Ok(flowable_service_unavailable());
    };
    let tenant_id = resolve_requested_tenant(&req, &claims, body.tenant_id.as_deref())?;
    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_DEFINITION_PUBLISH,
        scope_from_tenant_id(&tenant_id),
    )?;
    let payload = svc
        .create_deployment(
            &body.bpmn_xml,
            body.deployment_name.as_deref(),
            body.category.as_deref(),
            Some(&tenant_id),
        )
        .await
        .map_err(map_service_error)?;
    Ok(ok_resp(payload, "流程部署成功"))
}

pub(crate) async fn list_deployments(
    svc: Option<web::Data<Arc<FlowableService>>>,
    req: HttpRequest,
    query: web::Query<DeploymentsQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let Some(svc) = svc else {
        return Ok(flowable_service_unavailable());
    };
    let tenant_id = resolve_requested_tenant(&req, &claims, query.tenant_id.as_deref())?;
    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_DEFINITION_READ,
        scope_from_tenant_id(&tenant_id),
    )?;
    let payload = svc
        .list_deployments(query.name.as_deref(), Some(&tenant_id))
        .await
        .map_err(map_service_error)?;
    Ok(ok_resp(payload, "成功获取流程部署列表"))
}

pub(crate) async fn delete_deployment(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    query: web::Query<DeleteDeploymentQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_DEFINITION_DEPRECATE)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let deployment_id = path.into_inner();
    let deleted = svc
        .delete_deployment(&deployment_id, query.cascade.unwrap_or(false))
        .await
        .map_err(map_service_error)?;
    if !deleted {
        return Err(ApiError::NotFound("流程部署未找到".into()));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("流程部署 {deployment_id} 删除成功")
    })))
}

pub(crate) async fn start_process_instance(
    svc: Option<web::Data<Arc<FlowableService>>>,
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<StartProcessInstanceRequest>,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let tenant_id = resolve_requested_tenant(&req, &claims, body.tenant_id.as_deref())?;
    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_RUN_START,
        scope_from_tenant_id(&tenant_id),
    )?;
    let Some(process_instance_id) = svc
        .start_process_instance(
            &body.process_key,
            body.business_key.as_deref(),
            body.variables.as_ref(),
            Some(&tenant_id),
        )
        .await
        .map_err(map_service_error)?
    else {
        return Ok(missing_process_instance_response(
            "启动流程实例失败: Flowable 未返回流程实例ID",
        ));
    };
    Ok(ok_resp(
        serde_json::json!({ "process_instance_id": process_instance_id }),
        "流程实例启动成功",
    ))
}

pub(crate) async fn list_process_instances(
    svc: Option<web::Data<Arc<FlowableService>>>,
    req: HttpRequest,
    query: web::Query<ProcessInstancesQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let Some(svc) = svc else {
        return Ok(flowable_service_unavailable());
    };
    let tenant_id = resolve_requested_tenant(&req, &claims, query.tenant_id.as_deref())?;
    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_RUN_READ,
        scope_from_tenant_id(&tenant_id),
    )?;
    let filter_items = [
        ("processDefinitionKey", query.process_key.clone()),
        ("businessKey", query.business_key.clone()),
        ("tenantId", Some(tenant_id)),
    ];
    let filters = collect_filters(&filter_items);
    let payload = svc.list_process_instances(&filters).await.map_err(map_service_error)?;
    Ok(ok_resp(payload, "成功获取流程实例列表"))
}

pub(crate) async fn get_process_instance(
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
        .get_process_instance(&process_instance_id)
        .await
        .map_err(map_service_error)?
    else {
        return Ok(raw_detail_message(
            actix_web::http::StatusCode::NOT_FOUND,
            "流程实例未找到",
        ));
    };
    Ok(ok_resp(payload, "成功获取流程实例"))
}

pub(crate) async fn delete_process_instance(
    svc: Option<web::Data<Arc<FlowableService>>>,
    path: web::Path<String>,
    query: web::Query<DeleteProcessInstanceQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_grant(&claims, PermissionCatalog::WORKFLOW_RUN_ACT)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let process_instance_id = path.into_inner();
    let deleted = svc
        .delete_process_instance(&process_instance_id, query.delete_reason.as_deref())
        .await
        .map_err(map_service_error)?;
    if !deleted {
        return Ok(raw_detail_message(
            actix_web::http::StatusCode::NOT_FOUND,
            "流程实例未找到",
        ));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("流程实例 {process_instance_id} 删除成功")
    })))
}

pub(crate) async fn list_tasks(
    svc: Option<web::Data<Arc<FlowableService>>>,
    req: HttpRequest,
    query: web::Query<TasksQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_authenticated(&claims)?;
    let Some(svc) = svc else {
        return Ok(flowable_client_unavailable());
    };
    let tenant_id = resolve_requested_tenant(&req, &claims, query.tenant_id.as_deref())?;
    ensure_scope_grant(
        &claims,
        PermissionCatalog::WORKFLOW_RUN_READ,
        scope_from_tenant_id(&tenant_id),
    )?;
    let filter_items = [
        ("assignee", query.assignee.clone()),
        ("owner", query.owner.clone()),
        ("processInstanceId", query.process_instance_id.clone()),
        ("processDefinitionKey", query.process_definition_key.clone()),
        ("tenantId", Some(tenant_id)),
    ];
    let filters = collect_filters(&filter_items);
    let payload = svc.list_tasks(&filters).await.map_err(map_service_error)?;
    Ok(ok_resp(payload, "成功获取任务列表"))
}
