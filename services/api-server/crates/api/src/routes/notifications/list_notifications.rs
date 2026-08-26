use super::*;

/// GET /api/v2/notifications
pub(crate) async fn list_notifications(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    query: web::Query<NotifListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let unread_only = query.unread_only.unwrap_or(false);
    let result = svc.list_notifications(user_id, unread_only, limit, offset).await?;
    Ok(HttpResponse::Ok().json(json!({
        "items": result,
        "total": result.len(),
        "limit": limit,
        "offset": offset,
    })))
}

/// GET /api/v2/notifications/unread-count
pub(crate) async fn get_unread_count(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let count = svc.get_unread_count(user_id).await?;
    Ok(HttpResponse::Ok().json(json!({ "unread_count": count })))
}

/// POST /api/v2/notifications/{id}/read
pub(crate) async fn mark_read(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let id = path.into_inner();
    let ok = svc.mark_read(&id, user_id).await?;
    Ok(ok_resp(
        if ok {
            "Notification marked as read"
        } else {
            "Notification not found or already read"
        },
        json!({ "notification_id": id }),
    ))
}

/// POST /api/v2/notifications/{id}/ack
pub(crate) async fn ack_notification(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    path: web::Path<String>,
    body: web::Json<NotificationAckRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ack_notification_inner(svc.get_ref().as_ref(), path, body, claims).await
}

/// POST /api/v2/notifications/read-all
pub(crate) async fn mark_all_read(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let updated = svc.mark_all_read(user_id).await?;
    Ok(ok_resp(
        "All notifications marked as read",
        json!({ "updated": updated }),
    ))
}

/// GET /api/v2/notifications/preferences
pub(crate) async fn get_preferences(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let preference = svc.get_preferences(user_id).await?;
    Ok(HttpResponse::Ok().json(preference))
}

/// PATCH /api/v2/notifications/preferences
pub(crate) async fn update_preferences(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    claims: JwtAuth,
    body: web::Json<NotificationPreferenceUpdate>,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let updated = svc.update_preferences(user_id, body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(updated))
}

/// GET /api/v2/notifications/{id}/receipts
pub(crate) async fn get_receipts(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let item = svc
        .get_notification(&path.into_inner(), user_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Notification not found".into()))?;
    Ok(HttpResponse::Ok().json(notification_receipt_value(&item)))
}

/// GET /api/v2/notifications/receipt-groups/{receipt_group_id}
pub(crate) async fn get_receipt_group(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    get_receipt_group_inner(svc.get_ref().as_ref(), path, claims).await
}

/// GET /api/v2/notifications/sent-receipt-groups
pub(crate) async fn list_sent_receipt_groups(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    query: web::Query<SentReceiptGroupsQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::BadRequest("limit must be between 1 and 100".into()));
    }
    if offset < 0 {
        return Err(ApiError::BadRequest("offset must be >= 0".into()));
    }
    let payload = svc.list_sent_receipt_groups(user_id, limit, offset).await?;
    Ok(HttpResponse::Ok().json(payload))
}

/// GET /api/v2/notifications/dispatch/online-users
pub(crate) async fn list_dispatch_online_users(
    svc: web::Data<Arc<OnlineStatusService>>,
    query: web::Query<DispatchOnlineUsersQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    ensure_dispatch_view_permission(&claims)?;
    let caller_user_id = claims.0.sub.as_deref().unwrap_or_default();
    let keyword = query.keyword.as_deref().unwrap_or("").trim().to_lowercase();
    let payload = svc
        .search_online_users(
            query.department.as_deref(),
            query.job_title.as_deref(),
            Some("online"),
            query.limit.unwrap_or(120),
            caller_user_id,
        )
        .await?;
    let raw_items = payload
        .get("users")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();
    for item in raw_items {
        let Some(item_obj) = item.as_object() else {
            continue;
        };
        let user_id = item_obj
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if user_id.is_empty() {
            continue;
        }
        let account_type = item_obj
            .get("account_type")
            .and_then(|value| value.as_str())
            .unwrap_or("personal");
        let display_name = item_obj
            .get("display_name")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&user_id)
            .to_string();
        let department = item_obj
            .get("department")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        // 搜索 haystack：岗名、岗位 username、在岗人名、科室、航班号、任务名、槽位。
        let mut haystack = format!("{} {}", user_id, display_name);
        if let Some(department) = department.as_deref() {
            haystack.push(' ');
            haystack.push_str(department);
        }
        if account_type == "position" {
            if let Some(occupant_name) = item_obj
                .get("occupant_display_name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                haystack.push(' ');
                haystack.push_str(occupant_name);
            }
        } else if let Some(assignments) = item_obj.get("assignments").and_then(|value| value.as_array()) {
            for assignment in assignments {
                for key in ["flight_no", "task_type", "task_type_name", "slot_code", "slot_name"] {
                    if let Some(text) = assignment.get(key).and_then(|value| value.as_str()) {
                        haystack.push(' ');
                        haystack.push_str(text.trim());
                    }
                }
            }
        }
        let haystack = haystack.to_lowercase();
        if !(keyword.is_empty() || haystack.contains(&keyword)) {
            continue;
        }

        items.push(json!({
            "user_id": user_id,
            "account_type": account_type,
            "display_name": display_name,
            "occupant_user_id": item_obj.get("occupant_user_id").unwrap_or(&NULL_VALUE),
            "occupant_display_name": item_obj.get("occupant_display_name").unwrap_or(&NULL_VALUE),
            "assignments": item_obj.get("assignments").unwrap_or(&NULL_VALUE),
            "label": item_obj.get("label").unwrap_or(&NULL_VALUE),
            "meta": item_obj.get("meta").unwrap_or(&NULL_VALUE),
            "department": department,
            "status": item_obj.get("status").unwrap_or(&NULL_VALUE),
            "login_time": item_obj.get("login_time").unwrap_or(&NULL_VALUE),
            "last_heartbeat": item_obj.get("last_heartbeat").unwrap_or(&NULL_VALUE),
        }));
    }
    Ok(ok_resp(
        "dispatch online users loaded",
        json!({ "items": items, "total": items.len() }),
    ))
}

/// POST /api/v2/notifications/dispatch/send
pub(crate) async fn send_dispatch_manual_notification(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    body: web::Json<DispatchManualNotificationRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission(PermissionCatalog::NOTIFICATION_SEND)?;
    let payload = body.into_inner();
    let recipient_user_ids = payload
        .recipient_user_ids
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let mut deduped_recipient_user_ids = Vec::with_capacity(recipient_user_ids.len());
    let mut seen_recipient_user_ids = std::collections::HashSet::new();
    for user_id in recipient_user_ids {
        if seen_recipient_user_ids.insert(user_id.clone()) {
            deduped_recipient_user_ids.push(user_id);
        }
    }
    let recipient_user_ids = deduped_recipient_user_ids;
    if recipient_user_ids.is_empty() {
        return Err(ApiError::BadRequest("recipient_user_ids cannot be empty".into()));
    }
    if recipient_user_ids.len() > 50 {
        return Err(ApiError::BadRequest("recipient_user_ids exceeds limit (50)".into()));
    }
    let title = payload.title.trim().to_string();
    let body_text = payload.body.trim().to_string();
    if title.is_empty() {
        return Err(ApiError::BadRequest("title cannot be empty".into()));
    }
    if title.len() > 120 {
        return Err(ApiError::BadRequest("title exceeds limit (120)".into()));
    }
    if body_text.is_empty() {
        return Err(ApiError::BadRequest("body cannot be empty".into()));
    }
    if body_text.len() > 1200 {
        return Err(ApiError::BadRequest("body exceeds limit (1200)".into()));
    }

    let severity = payload.severity.unwrap_or_else(|| "warning".to_string());
    let severity_normalized = severity.trim().to_ascii_lowercase();
    if !matches!(severity_normalized.as_str(), "info" | "warning" | "critical") {
        return Err(ApiError::BadRequest(
            "severity must be info, warning, or critical".into(),
        ));
    }

    let flight_id = match payload.flight_id {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else if trimmed.len() > 128 {
                return Err(ApiError::BadRequest("flight_id exceeds limit (128)".into()));
            } else {
                Some(trimmed)
            }
        }
        None => None,
    };
    let flight_no = match payload.flight_no {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else if trimmed.len() > 64 {
                return Err(ApiError::BadRequest("flight_no exceeds limit (64)".into()));
            } else {
                Some(trimmed)
            }
        }
        None => None,
    };

    let sender_id = current_user_id(&claims)?.to_string();
    let sender_name = current_username(&claims).to_string();
    let context_text = match &flight_no {
        Some(no) => format!("（航班 {no}）"),
        None => String::new(),
    };
    let full_body = format!("{body_text}\n\n发送人: {sender_name} ({sender_id}){context_text}");

    let result = svc
        .send_batch(DispatchBatchNotificationCreate {
            user_ids: recipient_user_ids.clone(),
            title: title.clone(),
            body: full_body,
            category: "dispatch".to_string(),
            severity: severity_normalized,
            flight_id: flight_id.clone(),
            related_entity_type: flight_id.as_ref().map(|_| "flight".to_string()),
            related_entity_id: flight_id,
            dispatch_order_id: None,
            group_id: None,
            sender_user_id: Some(sender_id.clone()),
            sender_username_snapshot: Some(sender_name.clone()),
            origin_type: "manual".to_string(),
            receipt_required: payload.receipt_required.unwrap_or(true),
        })
        .await?;

    let sent_count = result
        .get("items")
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0);
    if sent_count == 0 {
        return Err(ApiError::Internal("failed to send dispatch notifications".into()));
    }
    Ok(ok_resp(
        "dispatch notifications sent",
        json!({
            "sent_count": sent_count,
            "failed_count": recipient_user_ids.len().saturating_sub(sent_count),
            "failed_user_ids": [],
            "target_user_ids": recipient_user_ids,
            "receipt_group_id": result.get("receipt_group_id").unwrap_or(&NULL_VALUE),
        }),
    ))
}
