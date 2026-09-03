//! 进程内嵌入式 Flowable 引擎：直接调用 vendored flowable-rust-oss
//! 的 ProcessEngine，替代对 tomcat flowable-rest 的 HTTP 调用。
//!
//! 引擎 API 是同步的；其持久化层内部自带 sqlx 异步桥接
//! （flowable-persistence/src/adapters/sqlx_executor.rs），在任意
//! tokio 上下文调用均安全。本适配器统一用 spawn_blocking 包裹，
//! 避免阻塞 actix worker 线程。
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use flowable_engine::engine::process_engine::ProcessEngine;
use flowable_engine::error::FlowableError;
use flowable_engine::service::config::{EngineDatabaseKind, ProcessEngineConfiguration};
use fms_domain::ports::flowable_gateway::{FlowableGateway, FlowableGatewayError};
use fms_runtime::environment::{current, RuntimeEnvironment};

mod history;
mod repository;
mod runtime;
mod tasks;

#[cfg(test)]
mod tests;

pub struct EmbeddedFlowableEngine {
    engine: Arc<ProcessEngine>,
}

const ENGINE_NAME: &str = "fms-embedded-flowable";

impl EmbeddedFlowableEngine {
    /// 从环境变量构造。
    pub fn try_new_from_env() -> Result<Self, FlowableGatewayError> {
        Self::build(current(), database_url())
    }

    /// 后端裁决：环境 + 库配置 → 引擎或错误，不读进程环境，便于确定性地测出 fail-closed 分支。
    ///
    /// - 有库配置 → PostgreSQL 后端（生产），引擎构造时自动建/补 ACT_* 表
    ///   （to_persistence_config 内建 SchemaMode::True，无需额外配置）。
    /// - 缺库配置 + production → 拒绝启动。降级成内存后端会让进程「启动成功、接口正常」
    ///   而流程数据只存在内存里，重启即丢，属于必须消除的假成功路径。
    /// - 缺库配置 + 开发/测试 → 内存后端。
    fn build(environment: RuntimeEnvironment, url: Option<String>) -> Result<Self, FlowableGatewayError> {
        let engine = match url {
            None => {
                if environment.is_production() {
                    return Err(FlowableGatewayError::Upstream(
                        "FLOWABLE_DATABASE_URL 未设置：production 环境拒绝使用内存后端启动（流程数据不落库，重启即丢）"
                            .to_string(),
                    ));
                }
                tracing::warn!(
                    "FLOWABLE_DATABASE_URL 未设置，嵌入式 Flowable 引擎使用内存后端——流程数据不落库，仅限测试/开发"
                );
                // 不走 ProcessEngineConfiguration::default()：default() 会读
                // FLOWABLE_TEST_ENGINE_DATABASE_URL，new_with_memory_backend 绕开该干扰。
                ProcessEngine::new_with_memory_backend(ENGINE_NAME.to_string())
            }
            Some(url) => {
                let config = Self::postgres_config(&url);
                ProcessEngine::try_new_with_config(ENGINE_NAME.to_string(), config).map_err(|error| {
                    FlowableGatewayError::Upstream(format!("embedded flowable engine init failed: {error}"))
                })?
            }
        };
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    fn postgres_config(url: &str) -> ProcessEngineConfiguration {
        let mut config = ProcessEngineConfiguration::default();
        config.database.kind = EngineDatabaseKind::Postgres;
        config.database.url = url.to_string();
        config.database.pool_size = std::env::var("FLOWABLE_DB_POOL_SIZE")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|&value| value > 0)
            .unwrap_or(8);
        config
    }
}

fn database_url() -> Option<String> {
    std::env::var("FLOWABLE_DATABASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

const DEFAULT_ENGINE_TIMEOUT_SECS: u64 = 30;

fn parse_engine_timeout(raw: Option<&str>) -> Duration {
    Duration::from_secs(
        raw.and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|&value| value > 0)
            .unwrap_or(DEFAULT_ENGINE_TIMEOUT_SECS),
    )
}

fn engine_timeout() -> Duration {
    static TIMEOUT: OnceLock<Duration> = OnceLock::new();
    *TIMEOUT.get_or_init(|| parse_engine_timeout(std::env::var("FLOWABLE_ENGINE_TIMEOUT_SECS").ok().as_deref()))
}

/// 在阻塞线程池上执行引擎调用，统一错误映射。
///
/// 自由函数形式：`FlowableGateway` 的 impl 块必须整体位于
/// `embedded_flowable.rs`（Rust 不允许一个 trait 的实现拆散到多个
/// impl 块），各方法组实现为子模块里的自由函数并复用本辅助。
///
/// 超时只解除调用方的等待并给出明确错误，不会回收阻塞线程本身：
/// `spawn_blocking` 里的引擎调用仍会跑到结束，因此预算要按「最慢正常
/// 查询 + 余量」设置，不能压到毫秒级。
pub(crate) async fn run_on_engine<T, F>(engine: Arc<ProcessEngine>, f: F) -> Result<T, FlowableGatewayError>
where
    T: Send + 'static,
    F: FnOnce(&ProcessEngine) -> Result<T, FlowableError> + Send + 'static,
{
    let budget = engine_timeout();
    let joined = tokio::time::timeout(budget, tokio::task::spawn_blocking(move || f(&engine)))
        .await
        .map_err(|_| {
            FlowableGatewayError::Upstream(format!("flowable engine call timed out after {}s", budget.as_secs()))
        })?;
    joined
        .map_err(|join_error| FlowableGatewayError::Upstream(format!("flowable engine task panicked: {join_error}")))?
        .map_err(|error: FlowableError| FlowableGatewayError::Upstream(error.to_string()))
}

#[async_trait]
impl FlowableGateway for EmbeddedFlowableEngine {
    async fn get_process_definitions(
        &self,
        key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        repository::get_process_definitions(&self.engine, key, tenant_id).await
    }

    async fn get_process_definition(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        repository::get_process_definition(&self.engine, process_definition_id).await
    }

    async fn get_process_definition_xml(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<String>, FlowableGatewayError> {
        repository::get_process_definition_xml(&self.engine, process_definition_id).await
    }

    async fn get_deployments(
        &self,
        name: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        repository::get_deployments(&self.engine, name, tenant_id).await
    }

    async fn deploy_process(
        &self,
        bpmn_xml: &str,
        deployment_name: Option<&str>,
        category: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<serde_json::Value, FlowableGatewayError> {
        repository::deploy_process(&self.engine, bpmn_xml, deployment_name, category, tenant_id).await
    }

    async fn delete_deployment(&self, deployment_id: &str, cascade: bool) -> Result<bool, FlowableGatewayError> {
        repository::delete_deployment(&self.engine, deployment_id, cascade).await
    }

    async fn start_process_instance(
        &self,
        process_key: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
        business_key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Option<String>, FlowableGatewayError> {
        runtime::start_process_instance(&self.engine, process_key, variables, business_key, tenant_id).await
    }

    async fn get_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        runtime::get_process_instances(&self.engine, filters).await
    }

    async fn get_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        runtime::get_process_instance(&self.engine, process_instance_id).await
    }

    async fn delete_process_instance(
        &self,
        process_instance_id: &str,
        delete_reason: Option<&str>,
    ) -> Result<bool, FlowableGatewayError> {
        runtime::delete_process_instance(&self.engine, process_instance_id, delete_reason).await
    }

    async fn get_tasks(&self, filters: &[(&str, String)]) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        tasks::get_tasks(&self.engine, filters).await
    }

    async fn get_task(&self, task_id: &str) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        tasks::get_task(&self.engine, task_id).await
    }

    async fn claim_task(&self, task_id: &str, user_id: &str) -> Result<bool, FlowableGatewayError> {
        tasks::claim_task(&self.engine, task_id, user_id).await
    }

    async fn unclaim_task(&self, task_id: &str) -> Result<bool, FlowableGatewayError> {
        tasks::unclaim_task(&self.engine, task_id).await
    }

    async fn complete_task(
        &self,
        task_id: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<bool, FlowableGatewayError> {
        tasks::complete_task(&self.engine, task_id, variables).await
    }

    async fn get_executions(&self, filters: &[(&str, String)]) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        runtime::get_executions(&self.engine, filters).await
    }

    async fn get_process_instance_variables(
        &self,
        process_instance_id: &str,
    ) -> Result<serde_json::Value, FlowableGatewayError> {
        runtime::get_process_instance_variables(&self.engine, process_instance_id).await
    }

    async fn set_process_instance_variables(
        &self,
        process_instance_id: &str,
        variables: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<bool, FlowableGatewayError> {
        runtime::set_process_instance_variables(&self.engine, process_instance_id, variables).await
    }

    async fn get_historic_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        history::get_historic_process_instances(&self.engine, filters).await
    }

    async fn get_historic_tasks(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        history::get_historic_tasks(&self.engine, filters).await
    }

    async fn get_historic_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        history::get_historic_process_instance(&self.engine, process_instance_id).await
    }

    async fn get_historic_variable_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        history::get_historic_variable_instances(&self.engine, filters).await
    }
}
