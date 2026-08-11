//! 在线状态服务

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;

use fms_domain::error::DomainError;
use fms_domain::ports::online_history_repository::OnlineHistoryRepository;
use fms_domain::ports::session_runtime_repository::SessionRuntimeRepository;
use fms_domain::ports::user_repository::UserRepository;

pub struct OnlineStatusService {
    user_repo: Arc<dyn UserRepository + Send + Sync>,
    session_runtime: Arc<dyn SessionRuntimeRepository + Send + Sync>,
    online_history_repo: Arc<dyn OnlineHistoryRepository + Send + Sync>,
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
    ) -> Self {
        Self {
            user_repo,
            session_runtime,
            online_history_repo,
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

        let mut matched_sessions = Vec::new();
        for session in statuses {
            let Some(user) = user_map.get(&session.user_id) else {
                continue;
            };
            if !user.is_active {
                continue;
            }
            if let Some(required_department) = normalized_department {
                if user.department.as_deref() != Some(required_department) {
                    continue;
                }
            }
            if let Some(required_job_title) = normalized_job_title {
                if user.job_title.as_deref() != Some(required_job_title) {
                    continue;
                }
            }
            if !matches_status_filter(&session.status, normalized_status) {
                continue;
            }

            matched_sessions.push((user, session));
        }

        matched_sessions.sort_by(|left, right| right.1.login_time.cmp(&left.1.login_time));

        let mut items = Vec::new();
        for (user, session) in matched_sessions.into_iter().take(limit.clamp(1, 300) as usize) {
            items.push(json!({
                "id": user.id,
                "username": user.username,
                "department": user.department,
                "job_title": user.job_title,
                "status": session.status,
                "login_time": session.login_time.map(|value| value.to_rfc3339()),
                "last_heartbeat": session.last_seen.map(|value| value.to_rfc3339()),
                "ip_address": session.client_ip,
            }));
        }

        Ok(json!({
            "users": items,
            "total": items.len(),
        }))
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

    fn build_service(users: Vec<User>, statuses: Vec<OnlineSessionStatus>) -> OnlineStatusService {
        OnlineStatusService::new(
            Arc::new(MockUserRepository { users }),
            Arc::new(MockSessionRuntimeRepository { statuses }),
            Arc::new(MockOnlineHistoryRepository),
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
        );

        let payload = service
            .search_online_users(Some("ops"), Some("调度员"), Some("online"), 10)
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
        );

        let payload = service
            .search_online_users(None, None, Some("online"), 1)
            .await
            .expect("search_online_users should succeed");

        let users = payload["users"]
            .as_array()
            .expect("users should be returned as an array");

        assert_eq!(users.len(), 1);
        assert_eq!(users[0]["id"].as_str(), Some("u-2"));
        assert_eq!(payload["total"].as_u64(), Some(1));
    }
}
