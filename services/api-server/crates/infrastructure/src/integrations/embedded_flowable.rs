//! 进程内嵌入式 Flowable 引擎：直接调用 vendored flowable-rust-oss
//! 的 ProcessEngine，替代对 tomcat flowable-rest 的 HTTP 调用。
//!
//! 引擎 API 是同步的；其持久化层内部自带 sqlx 异步桥接
//! （flowable-persistence/src/adapters/sqlx_executor.rs），在任意
//! tokio 上下文调用均安全。本适配器统一用 spawn_blocking 包裹，
//! 避免阻塞 actix worker 线程。
use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::ports::flowable_gateway::{FlowableGateway, FlowableGatewayError};
use flowable_engine::engine::process_engine::ProcessEngine;
use flowable_engine::error::FlowableError;
use flowable_engine::service::config::{EngineDatabaseKind, ProcessEngineConfiguration};

mod repository;
mod runtime;

#[cfg(test)]
mod tests;

pub struct EmbeddedFlowableEngine {
    engine: Arc<ProcessEngine>,
}

const ENGINE_NAME: &str = "fms-embedded-flowable";

impl EmbeddedFlowableEngine {
    /// 从环境变量构造。
    ///
    /// - `FLOWABLE_DATABASE_URL` 非空 → PostgreSQL 后端（生产），
    ///   引擎构造时自动建/补 ACT_* 表（to_persistence_config 内建
    ///   SchemaMode::True，无需额外配置）。
    /// - 为空 → 内存后端（测试/开发，数据不落盘）。
    pub fn try_new_from_env() -> Result<Self, FlowableGatewayError> {
        let url = std::env::var("FLOWABLE_DATABASE_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let engine = match url {
            // 不用 ProcessEngineConfiguration::default() 构造内存引擎：
            // default() 会读 FLOWABLE_TEST_ENGINE_DATABASE_URL 环境变量，
            // new_with_memory_backend 完全绕开该干扰。
            None => ProcessEngine::new_with_memory_backend(ENGINE_NAME.to_string()),
            Some(url) => {
                let config = Self::postgres_config(&url);
                ProcessEngine::try_new_with_config(ENGINE_NAME.to_string(), config).map_err(
                    |error| {
                        FlowableGatewayError::Upstream(format!(
                            "embedded flowable engine init failed: {error}"
                        ))
                    },
                )?
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

/// 在阻塞线程池上执行引擎调用，统一错误映射。
///
/// 自由函数形式：`FlowableGateway` 的 impl 块必须整体位于
/// `embedded_flowable.rs`（Rust 不允许一个 trait 的实现拆散到多个
/// impl 块），各方法组实现为子模块里的自由函数并复用本辅助。
pub(crate) async fn run_on_engine<T, F>(
    engine: Arc<ProcessEngine>,
    f: F,
) -> Result<T, FlowableGatewayError>
where
    T: Send + 'static,
    F: FnOnce(&ProcessEngine) -> Result<T, FlowableError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || f(&engine))
        .await
        .map_err(|join_error| {
            FlowableGatewayError::Upstream(format!("flowable engine task panicked: {join_error}"))
        })?
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
        repository::deploy_process(&self.engine, bpmn_xml, deployment_name, category, tenant_id)
            .await
    }

    async fn delete_deployment(
        &self,
        deployment_id: &str,
        cascade: bool,
    ) -> Result<bool, FlowableGatewayError> {
        repository::delete_deployment(&self.engine, deployment_id, cascade).await
    }

    async fn start_process_instance(
        &self,
        process_key: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
        business_key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Option<String>, FlowableGatewayError> {
        runtime::start_process_instance(&self.engine, process_key, variables, business_key, tenant_id)
            .await
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

    async fn get_tasks(
        &self,
        _filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        unimplemented!("Task 6")
    }

    async fn get_task(
        &self,
        _task_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        unimplemented!("Task 6")
    }

    async fn claim_task(&self, _task_id: &str, _user_id: &str) -> Result<bool, FlowableGatewayError> {
        unimplemented!("Task 6")
    }

    async fn unclaim_task(&self, _task_id: &str) -> Result<bool, FlowableGatewayError> {
        unimplemented!("Task 6")
    }

    async fn complete_task(
        &self,
        _task_id: &str,
        _variables: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<bool, FlowableGatewayError> {
        unimplemented!("Task 6")
    }

    async fn get_executions(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
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
        _filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        unimplemented!("Task 7")
    }

    async fn get_historic_tasks(
        &self,
        _filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        unimplemented!("Task 7")
    }

    async fn get_historic_process_instance(
        &self,
        _process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError> {
        unimplemented!("Task 7")
    }

    async fn get_historic_variable_instances(
        &self,
        _filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError> {
        unimplemented!("Task 7")
    }
}
