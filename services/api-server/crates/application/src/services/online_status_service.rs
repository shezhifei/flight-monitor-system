//! 在线状态服务

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;

use fms_domain::error::DomainError;
use fms_domain::ports::dispatch_repository::DispatchOrderMemberRepository;
use fms_domain::ports::online_history_repository::OnlineHistoryRepository;
use fms_domain::ports::session_runtime_repository::SessionRuntimeRepository;
use fms_domain::ports::user_repository::UserRepository;

pub struct OnlineStatusService {
    user_repo: Arc<dyn UserRepository + Send + Sync>,
    session_runtime: Arc<dyn SessionRuntimeRepository + Send + Sync>,
    online_history_repo: Arc<dyn OnlineHistoryRepository + Send + Sync>,
    dispatch_member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
}

fn normalize_query_value(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn matches_status_filter(session_status: &str, required_status: Option<&str>) -> bool {
    let normalized_session_status = session_status.trim().to_ascii_lowercase();
    match required_status.map(|value| value.trim().to_ascii_lowercase()) {
        Some(required) if required == "online" => normalized_session_status != "offline",
        Some(required) => normalized_session_status == required,
        None => true,
    }
}

impl OnlineStatusService {
    pub fn new(
        user_repo: Arc<dyn UserRepository + Send + Sync>,
        session_runtime: Arc<dyn SessionRuntimeRepository + Send + Sync>,
        online_history_repo: Arc<dyn OnlineHistoryRepository + Send + Sync>,
        dispatch_member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
    ) -> Self {
        Self {
            user_repo,
            session_runtime,
            online_history_repo,
            dispatch_member_repo,
        }
    }

    pub async fn get_online_summary(&self) -> Result<serde_json::Value, DomainError> {
        let (statuses, all_users) = tokio::try_join!(
            self.session_runtime.get_all_online_status(),
            self.user_repo.find_all(10_000, 0),
        )?;
        let total_users = all_users.iter().filter(|user| user.is_active).count() as i64;

        let mut user_map = BTreeMap::new();
        for user in all_users {
            user_map.insert(user.id.clone(), user);
        }

        let total_online = statuses.len() as i64;
        let total_active = statuses.iter().filter(|status| status.status == "active").count() as i64;
        let total_idle = statuses.iter().filter(|status| status.status == "idle").count() as i64;

        let mut by_job_title: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for status in statuses {
            let job_title = user_map
                .get(&status.user_id)
                .and_then(|user| user.job_title.clone())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "未设置岗位".to_string());

            let entry = by_job_title.entry(job_title).or_insert_with(|| {
                json!({
                    "total": 0,
                    "online": 0,
                    "active": 0,
                    "idle": 0,
                })
            });

            let total = entry.get("total").and_then(|value| value.as_i64()).unwrap_or(0) + 1;
            let online = entry.get("online").and_then(|value| value.as_i64()).unwrap_or(0) + 1;
            let active = entry.get("active").and_then(|value| value.as_i64()).unwrap_or(0)
                + i64::from(status.status == "active");
            let idle =
                entry.get("idle").and_then(|value| value.as_i64()).unwrap_or(0) + i64::from(status.status == "idle");

            *entry = json!({
                "total": total,
                "online": online,
                "active": active,
                "idle": idle,
            });
        }

        Ok(json!({
            "total_online": total_online,
            "total_active": total_active,
            "total_idle": total_idle,
            "total_users": total_users,
            "online_rate": if total_users > 0 {
                ((total_online as f64) / (total_users as f64) * 100.0).round() / 100.0
            } else {
                0.0
            },
            "by_job_title": by_job_title,
            "last_updated": Utc::now().to_rfc3339(),
        }))
    }

    pub async fn force_user_offline(&self, user_id: &str) -> Result<bool, DomainError> {
        let status = self.session_runtime.get_online_status(user_id).await?;
        if status.status == "offline" {
            return Ok(false);
        }

        let revoked = self
            .session_runtime
            .revoke_session(user_id, "admin_force_offline")
            .await?;
        if let Some(revoked) = revoked {
            if let Some(session_id) = revoked.session_id.as_deref() {
                self.online_history_repo
                    .record_logout(user_id, session_id, true)
                    .await?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn search_online_users(
        &self,
        department: Option<&str>,
        job_title: Option<&str>,
        status: Option<&str>,
        limit: i64,
        current_user_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        let (statuses, all_users) = tokio::try_join!(
            self.session_runtime.get_all_online_status(),
            self.user_repo.find_all(10_000, 0),
        )?;

        let normalized_department = normalize_query_value(department);
        let normalized_job_title = normalize_query_value(job_title);
        let normalized_status = normalize_query_value(status);

        let mut user_map = BTreeMap::new();
        for user in all_users {
            user_map.insert(user.id.clone(), user);
        }

        // 收集活会话的个人用户，并按 user_id 建立会话索引。
        let mut online_personal_ids: Vec<String> = Vec::new();
        let mut session_by_user: BTreeMap<String, fms_domain::models::session_runtime::OnlineSessionStatus> =
            BTreeMap::new();
        for session in statuses {
            let Some(user) = user_map.get(&session.user_id) else {
                continue;
            };
            if !user.is_active || session.status.trim().eq_ignore_ascii_case("offline") {
                continue;
            }
            if let Some(required_job_title) = normalized_job_title {
                if user.job_title.as_deref() != Some(required_job_title) {
                    continue;
                }
            }
            if !matches_status_filter(&session.status, normalized_status) {
                continue;
            }
            online_personal_ids.push(session.user_id.clone());
            session_by_user.insert(session.user_id.clone(), session);
        }

        // 当前用户自己占用的席（排除自己，不要按人名排）。
        let self_position_id = user_map
            .values()
            .find(|user| user.is_position() && user.current_occupant_user_id.as_deref() == Some(current_user_id));

        // 一次查出在线个人用户的所有活跃工单槽，再按人聚合。
        let slots = self
            .dispatch_member_repo
            .find_active_slots_for_users(&online_personal_ids)
            .await?;
        let mut slots_by_user: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
        for slot in slots {
            let uid = slot
                .get("user_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            slots_by_user.entry(uid).or_default().push(slot);
        }

        let online_personal_set: std::collections::HashSet<&str> =
            online_personal_ids.iter().map(|value| value.as_str()).collect();

        let mut seat_rows: Vec<serde_json::Value> = Vec::new();
        let mut frontline_with_work: Vec<serde_json::Value> = Vec::new();
        let mut frontline_no_work: Vec<serde_json::Value> = Vec::new();

        for user in user_map.values() {
            if !user.is_active {
                continue;
            }
            let display_name = display_name_of(user);
            if user.is_position() {
                // 席：有占用、占用人会话活着，且不是本席。
                if self_position_id.is_some_and(|position| position.id == user.id) {
                    continue;
                }
                let Some(occupant_id) = user.current_occupant_user_id.as_deref() else {
                    continue;
                };
                if !online_personal_set.contains(occupant_id) {
                    continue;
                }
                if let Some(required_department) = normalized_department {
                    if user.department.as_deref() != Some(required_department) {
                        continue;
                    }
                }
                let Some(session) = session_by_user.get(occupant_id) else {
                    continue;
                };
                let Some(occupant) = user_map.get(occupant_id) else {
                    continue;
                };
                let occupant_name = display_name_of(occupant);
                let department = user.department.as_deref().filter(|value| !value.is_empty());
                let meta = match department {
                    Some(department) => format!("{department} · {occupant_name}"),
                    None => format!("未设置科室 · {occupant_name}"),
                };
                seat_rows.push(json!({
                    "id": user.id,
                    "username": user.username,
                    "account_type": "position",
                    "display_name": display_name,
                    "department": user.department,
                    "occupant_user_id": occupant_id,
                    "occupant_display_name": occupant_name,
                    "assignments": [],
                    "label": display_name,
                    "meta": meta,
                    "status": session.status,
                    "login_time": session.login_time.map(|value| value.to_rfc3339()),
                    "last_heartbeat": session.last_seen.map(|value| value.to_rfc3339()),
                    "ip_address": session.client_ip,
                }));
                continue;
            }

            // 一线：个人在线，且当前未占任何席。坐班的人只通过席出现。
            let Some(session) = session_by_user.get(&user.id) else {
                continue;
            };
            if let Some(required_department) = normalized_department {
                if user.department.as_deref() != Some(required_department) {
                    continue;
                }
            }
            // 个人正占着某个席 -> 只以席行出现。
            let occupying_seat = user_map.values().any(|position| {
                position.is_position() && position.current_occupant_user_id.as_deref() == Some(user.id.as_str())
            });
            if occupying_seat {
                continue;
            }

            let base = json!({
                "id": user.id,
                "username": user.username,
                "account_type": "personal",
                "display_name": display_name,
                "department": user.department,
                "occupant_user_id": serde_json::Value::Null,
                "occupant_display_name": serde_json::Value::Null,
                "status": session.status,
                "login_time": session.login_time.map(|value| value.to_rfc3339()),
                "last_heartbeat": session.last_seen.map(|value| value.to_rfc3339()),
                "ip_address": session.client_ip,
            });

            let user_slots = slots_by_user.get(&user.id).cloned().unwrap_or_default();
            if user_slots.is_empty() {
                frontline_no_work.push(base.updated_with(|| {
                    json!({
                        "assignments": Vec::<serde_json::Value>::new(),
                        "label": display_name,
                        "meta": "无在办工单",
                    })
                }));
            } else {
                frontline_with_work.push(base.updated_with(|| {
                    let label = slot_label(&user_slots);
                    let meta = format!(
                        "{display_name} · {}",
                        user.department.as_deref().unwrap_or("未设置科室")
                    );
                    json!({
                        "assignments": user_slots,
                        "label": label,
                        "meta": meta,
                    })
                }));
            }
        }

        // 排序：有在办工单的一线排最前（in_progress 优先，其次 planned_start_time），
        // 其后按登录时间排席行，最后是无在办工单的一线（按登录时间倒序）。
        frontline_with_work.sort_by(compare_frontline_order);
        seat_rows.sort_by(|left, right| {
            let left_login = left
                .get("login_time")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let right_login = right
                .get("login_time")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            right_login.cmp(left_login)
        });
        frontline_no_work.sort_by(|left, right| {
            let left_login = left
                .get("login_time")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let right_login = right
                .get("login_time")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            right_login.cmp(left_login)
        });

        let mut items = Vec::with_capacity(seat_rows.len() + frontline_with_work.len() + frontline_no_work.len());
        items.extend(frontline_with_work);
        items.extend(seat_rows);
        items.extend(frontline_no_work);
        items.truncate(limit.clamp(1, 300) as usize);

        Ok(json!({
            "users": items,
            "total": items.len(),
        }))
    }
}

fn display_name_of(user: &fms_domain::models::user::User) -> &str {
    user.display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&user.username)
}

fn slot_label(user_slots: &[serde_json::Value]) -> String {
    user_slots
        .iter()
        .map(|slot| {
            let flight = slot
                .get("flight_no")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let task = slot
                .get("task_type_name")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .or_else(|| slot.get("task_type").and_then(|value| value.as_str()))
                .unwrap_or_default();
            let slot_name = slot
                .get("slot_name")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .or_else(|| slot.get("slot_code").and_then(|value| value.as_str()))
                .unwrap_or_default();
            format!("{flight}-{task}-{slot_name}")
        })
        .collect::<Vec<_>>()
        .join("；")
}

fn compare_frontline_order(left: &serde_json::Value, right: &serde_json::Value) -> std::cmp::Ordering {
    fn key(item: &serde_json::Value) -> (u8, String) {
        let assignments = item
            .get("assignments")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let in_progress = assignments
            .iter()
            .any(|slot| slot.get("status").and_then(|value| value.as_str()) == Some("in_progress"));
        let in_progress_rank = if in_progress { 0 } else { 1 };
        let earliest = assignments
            .iter()
            .filter_map(|slot| slot.get("planned_start_time").and_then(|value| value.as_str()))
            .min()
            .unwrap_or_default()
            .to_string();
        (in_progress_rank, earliest)
    }
    key(left).cmp(&key(right))
}

trait UpdatedWith {
    fn updated_with(self, extra: impl FnOnce() -> serde_json::Value) -> serde_json::Value;
}

impl UpdatedWith for serde_json::Value {
    fn updated_with(self, extra: impl FnOnce() -> serde_json::Value) -> serde_json::Value {
        let mut value = self;
        if let Some(obj) = value.as_object_mut() {
            if let serde_json::Value::Object(extra_obj) = extra() {
                for (key, val) in extra_obj {
                    obj.insert(key, val);
                }
            }
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Duration, Utc};

    use fms_domain::error::DomainError;
    use fms_domain::models::online_history::OnlineHistoryRecord;
    use fms_domain::models::session_runtime::{OnlineSessionStatus, SessionEstablishResult, SessionRuntimeStatus};
    use fms_domain::models::user::User;
    use fms_domain::ports::dispatch_repository::DispatchOrderMemberRepository;
    use fms_domain::ports::online_history_repository::OnlineHistoryRepository;
    use fms_domain::ports::session_runtime_repository::SessionRuntimeRepository;
    use fms_domain::ports::user_repository::UserRepository;

    use super::OnlineStatusService;

    struct MockUserRepository {
        users: Vec<User>,
    }

    #[async_trait::async_trait]
    impl UserRepository for MockUserRepository {
        async fn find_by_id(&self, id: &str) -> Result<Option<User>, DomainError> {
            Ok(self.users.iter().find(|user| user.id == id).cloned())
        }

        async fn find_permission_version_by_id(&self, id: &str) -> Result<Option<i32>, DomainError> {
            Ok(self
                .users
                .iter()
                .find(|user| user.id == id)
                .map(|user| user.permission_version))
        }

        async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
            Ok(self.users.iter().find(|user| user.username == username).cloned())
        }

        async fn find_by_email(&self, email: &str) -> Result<Option<User>, DomainError> {
            Ok(self.users.iter().find(|user| user.email == email).cloned())
        }

        async fn find_all(&self, _limit: i64, _offset: i64) -> Result<Vec<User>, DomainError> {
            Ok(self.users.clone())
        }

        async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, DomainError> {
            Ok(Vec::new())
        }

        async fn has_any_user_with_department_id(&self, _department_id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn save(&self, _user: &User) -> Result<(), DomainError> {
            Ok(())
        }

        async fn update(&self, _user: &User) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn update_password(&self, _id: &str, _password_hash: &str) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn update_last_login(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(true)
        }
    }

    struct MockSessionRuntimeRepository {
        statuses: Vec<OnlineSessionStatus>,
    }

    #[async_trait::async_trait]
    impl SessionRuntimeRepository for MockSessionRuntimeRepository {
        async fn establish_session(
            &self,
            _user_id: &str,
            _client_ip: Option<&str>,
            _refresh_token: Option<&str>,
        ) -> Result<SessionEstablishResult, DomainError> {
            unreachable!("not used in online status tests")
        }

        async fn validate_refresh_token(&self, _user_id: &str, _refresh_token: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn revoke_refresh_tokens(&self, _user_id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn revoke_session(
            &self,
            _user_id: &str,
            _reason: &str,
        ) -> Result<Option<OnlineSessionStatus>, DomainError> {
            Ok(None)
        }

        async fn heartbeat(&self, _user_id: &str) -> Result<Option<OnlineSessionStatus>, DomainError> {
            Ok(None)
        }

        async fn get_online_users(&self) -> Result<Vec<String>, DomainError> {
            Ok(self.statuses.iter().map(|status| status.user_id.clone()).collect())
        }

        async fn get_online_status(&self, user_id: &str) -> Result<OnlineSessionStatus, DomainError> {
            Ok(self
                .statuses
                .iter()
                .find(|status| status.user_id == user_id)
                .cloned()
                .unwrap_or(OnlineSessionStatus {
                    user_id: user_id.to_string(),
                    session_id: None,
                    login_time: None,
                    last_seen: None,
                    status: "offline".to_string(),
                    client_ip: None,
                    username: None,
                    job_title: None,
                    department: None,
                    forced_logout: false,
                    kick_event: None,
                }))
        }

        async fn get_all_online_status(&self) -> Result<Vec<OnlineSessionStatus>, DomainError> {
            Ok(self.statuses.clone())
        }

        async fn get_runtime_status(&self) -> Result<SessionRuntimeStatus, DomainError> {
            Ok(SessionRuntimeStatus {
                mode: "memory".to_string(),
                fallback_since: None,
                fallback_duration_seconds: None,
                circuit_state: "closed".to_string(),
                redis_available: false,
            })
        }

        async fn get_permission_version(&self, _user_id: &str) -> Result<i64, DomainError> {
            Ok(1)
        }
    }

    struct MockOnlineHistoryRepository;

    #[async_trait::async_trait]
    impl OnlineHistoryRepository for MockOnlineHistoryRepository {
        async fn record_login(
            &self,
            _user_id: &str,
            _session_id: &str,
            _ip_address: Option<&str>,
            _device_info: Option<&str>,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn record_logout(&self, _user_id: &str, _session_id: &str, _forced: bool) -> Result<(), DomainError> {
            Ok(())
        }

        async fn list_history(
            &self,
            _user_id: Option<&str>,
            _start_date: Option<chrono::DateTime<chrono::Utc>>,
            _end_date: Option<chrono::DateTime<chrono::Utc>>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<OnlineHistoryRecord>, DomainError> {
            Ok(Vec::new())
        }

        async fn count_history(
            &self,
            _user_id: Option<&str>,
            _start_date: Option<chrono::DateTime<chrono::Utc>>,
            _end_date: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<i64, DomainError> {
            Ok(0)
        }
    }

    fn sample_user(id: &str, username: &str, department: &str, job_title: &str) -> User {
        let now = Utc::now();
        User {
            id: id.to_string(),
            email: format!("{username}@example.com"),
            password_hash: "hashed".to_string(),
            username: username.to_string(),
            display_name: None,
            roles: Vec::new(),
            created_at: now,
            updated_at: now,
            last_login_at: None,
            is_active: true,
            is_verified: true,
            is_admin: false,
            verification_token: None,
            verification_token_expires: None,
            verified_at: None,
            password_reset_token: None,
            password_reset_token_expires: None,
            password_changed_at: None,
            department: Some(department.to_string()),
            department_id: None,
            job_level: Some(1),
            job_title: Some(job_title.to_string()),
            permission_version: 1,
            account_type: "personal".into(),
            login_enabled: true,
            current_occupant_user_id: None,
        }
    }

    fn sample_position(id: &str, username: &str, department: &str, occupant_user_id: Option<&str>) -> User {
        let now = Utc::now();
        User {
            id: id.to_string(),
            email: format!("{username}@example.com"),
            password_hash: "hashed".to_string(),
            username: username.to_string(),
            display_name: None,
            roles: Vec::new(),
            created_at: now,
            updated_at: now,
            last_login_at: None,
            is_active: true,
            is_verified: true,
            is_admin: false,
            verification_token: None,
            verification_token_expires: None,
            verified_at: None,
            password_reset_token: None,
            password_reset_token_expires: None,
            password_changed_at: None,
            department: Some(department.to_string()),
            department_id: None,
            job_level: Some(1),
            job_title: Some("调度员".to_string()),
            permission_version: 1,
            account_type: "position".into(),
            login_enabled: false,
            current_occupant_user_id: occupant_user_id.map(str::to_string),
        }
    }

    fn sample_status(user_id: &str, status: &str, login_offset_minutes: i64, client_ip: &str) -> OnlineSessionStatus {
        let login_time = Utc::now() - Duration::minutes(login_offset_minutes);
        OnlineSessionStatus {
            user_id: user_id.to_string(),
            session_id: Some(format!("session-{user_id}")),
            login_time: Some(login_time),
            last_seen: Some(login_time + Duration::minutes(1)),
            status: status.to_string(),
            client_ip: Some(client_ip.to_string()),
            username: None,
            job_title: None,
            department: None,
            forced_logout: false,
            kick_event: None,
        }
    }

    struct MockDispatchOrderMemberRepository {
        slots: Vec<serde_json::Value>,
    }

    #[async_trait::async_trait]
    impl DispatchOrderMemberRepository for MockDispatchOrderMemberRepository {
        async fn save(&self, _member: &fms_domain::models::dispatch::DispatchOrderMember) -> Result<(), DomainError> {
            unreachable!("not used in online status tests")
        }

        async fn find_by_order(
            &self,
            _order_id: &str,
        ) -> Result<Vec<fms_domain::models::dispatch::DispatchOrderMember>, DomainError> {
            unreachable!("not used in online status tests")
        }

        async fn find_by_order_and_user(
            &self,
            _order_id: &str,
            _user_id: &str,
        ) -> Result<Option<fms_domain::models::dispatch::DispatchOrderMember>, DomainError> {
            unreachable!("not used in online status tests")
        }

        async fn find_latest_checkout_for_user(
            &self,
            _user_id: &str,
            _before: chrono::DateTime<chrono::Utc>,
        ) -> Result<Option<serde_json::Value>, DomainError> {
            Ok(None)
        }

        async fn find_active_slots_for_users(
            &self,
            _user_ids: &[String],
        ) -> Result<Vec<serde_json::Value>, DomainError> {
            Ok(self.slots.clone())
        }
    }

    fn build_service(
        users: Vec<User>,
        statuses: Vec<OnlineSessionStatus>,
        slots: Vec<serde_json::Value>,
    ) -> OnlineStatusService {
        OnlineStatusService::new(
            Arc::new(MockUserRepository { users }),
            Arc::new(MockSessionRuntimeRepository { statuses }),
            Arc::new(MockOnlineHistoryRepository),
            Arc::new(MockDispatchOrderMemberRepository { slots }),
        )
    }

    #[tokio::test]
    async fn search_online_users_treats_online_as_active_and_idle() {
        let service = build_service(
            vec![
                sample_user("u-1", "alpha", "ops", "调度员"),
                sample_user("u-2", "bravo", "ops", "调度员"),
                sample_user("u-3", "charlie", "ops", "调度员"),
            ],
            vec![
                sample_status("u-1", "active", 30, "127.0.0.1"),
                sample_status("u-2", "idle", 10, "127.0.0.2"),
                sample_status("u-3", "offline", 5, "127.0.0.3"),
            ],
            Vec::new(),
        );

        let payload = service
            .search_online_users(Some("ops"), Some("调度员"), Some("online"), 10, "self-1")
            .await
            .expect("search_online_users should succeed");

        let users = payload["users"]
            .as_array()
            .expect("users should be returned as an array");
        let ids: Vec<&str> = users
            .iter()
            .filter_map(|item| item.get("id").and_then(|value| value.as_str()))
            .collect();

        assert_eq!(ids, vec!["u-2", "u-1"]);
        assert_eq!(payload["total"].as_u64(), Some(2));
        assert_eq!(users[0]["status"].as_str(), Some("idle"));
        assert_eq!(users[0]["ip_address"].as_str(), Some("127.0.0.2"));
    }

    #[tokio::test]
    async fn search_online_users_respects_limit_after_sorting() {
        let service = build_service(
            vec![
                sample_user("u-1", "alpha", "ops", "调度员"),
                sample_user("u-2", "bravo", "ops", "调度员"),
            ],
            vec![
                sample_status("u-1", "active", 20, "127.0.0.1"),
                sample_status("u-2", "active", 5, "127.0.0.2"),
            ],
            Vec::new(),
        );

        let payload = service
            .search_online_users(None, None, Some("online"), 1, "self-1")
            .await
            .expect("search_online_users should succeed");

        let users = payload["users"]
            .as_array()
            .expect("users should be returned as an array");

        assert_eq!(users.len(), 1);
        assert_eq!(users[0]["id"].as_str(), Some("u-2"));
        assert_eq!(payload["total"].as_u64(), Some(1));
    }

    #[tokio::test]
    async fn occupied_seat_with_live_occupant_appears_with_position_id_and_excludes_self_seat() {
        // pos-1 被 u-1 占用（且 u-1 在线）；pos-2 被当前用户 self-1 占用 -> 本席被排除。
        let service = build_service(
            vec![
                sample_user("u-1", "alpha", "ops", "调度员"),
                sample_position("pos-1", "pos-alpha", "ops", Some("u-1")),
                sample_position("pos-2", "pos-self", "ops", Some("self-1")),
                sample_position("pos-empty", "pos-empty", "ops", None),
            ],
            vec![
                sample_status("u-1", "active", 10, "127.0.0.9"),
                sample_status("self-1", "active", 5, "127.0.0.8"),
            ],
            Vec::new(),
        );

        let payload = service
            .search_online_users(None, None, Some("online"), 50, "self-1")
            .await
            .expect("search_online_users should succeed");

        let users = payload["users"]
            .as_array()
            .expect("users should be returned as an array");
        let ids: Vec<&str> = users
            .iter()
            .filter_map(|item| item.get("id").and_then(|value| value.as_str()))
            .collect();
        // pos-1 出现在列表；pos-2（本席）与 pos-empty（无人）都不出现。
        assert!(ids.contains(&"pos-1"));
        assert!(!ids.contains(&"pos-2"));
        assert!(!ids.contains(&"pos-empty"));
        // 占用者在席行中不作为一线重复出现。
        assert!(!ids.contains(&"u-1"));

        let seat = users
            .iter()
            .find(|item| item.get("id").and_then(|value| value.as_str()) == Some("pos-1"))
            .expect("pos-1 seat row should exist");
        assert_eq!(seat["account_type"].as_str(), Some("position"));
        assert_eq!(seat["occupant_user_id"].as_str(), Some("u-1"));
        assert_eq!(seat["occupant_display_name"].as_str(), Some("alpha"));
        assert_eq!(seat["label"].as_str(), Some("pos-alpha"));
        assert_eq!(seat["assignments"].as_array().map(Vec::len), Some(0));
    }

    #[tokio::test]
    async fn frontline_builds_flight_task_slot_label_and_deduplicates_occupant() {
        // u-9 在线且未占席，有两条工单槽 -> 一线行聚合两槽。
        let service = build_service(
            vec![sample_user("u-9", "charlie", "ground", "装卸员")],
            vec![sample_status("u-9", "active", 5, "127.0.0.5")],
            vec![serde_json::json!({
                "user_id": "u-9",
                "order_id": "o-1",
                "flight_id": "f-1",
                "flight_no": "CA101",
                "task_type": "load",
                "task_type_name": "装载",
                "slot_code": "loader-1",
                "slot_name": "装卸一",
                "status": "in_progress",
                "planned_start_time": "2026-08-26T08:00:00Z",
            })],
        );

        let payload = service
            .search_online_users(None, None, Some("online"), 50, "self-9")
            .await
            .expect("search_online_users should succeed");

        let users = payload["users"]
            .as_array()
            .expect("users should be returned as an array");
        assert_eq!(users.len(), 1);
        let first = &users[0];
        assert_eq!(first["account_type"].as_str(), Some("personal"));
        assert_eq!(first["id"].as_str(), Some("u-9"));
        assert_eq!(first["label"].as_str(), Some("CA101-装载-装卸一"));
        assert_eq!(first["assignments"][0]["flight_no"].as_str(), Some("CA101"));
        assert_eq!(first["assignments"][0]["slot_name"].as_str(), Some("装卸一"));
    }
}
