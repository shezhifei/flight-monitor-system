use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlowableGatewayError {
    #[error("resource not found")]
    NotFound,
    #[error("{0}")]
    Upstream(String),
}

#[async_trait]
pub trait FlowableGateway: Send + Sync {
    async fn get_process_definitions(
        &self,
        key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError>;

    async fn get_process_definition(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError>;

    async fn get_process_definition_xml(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<String>, FlowableGatewayError>;

    async fn get_deployments(
        &self,
        name: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError>;

    async fn deploy_process(
        &self,
        bpmn_xml: &str,
        deployment_name: Option<&str>,
        category: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<serde_json::Value, FlowableGatewayError>;

    async fn delete_deployment(&self, deployment_id: &str, cascade: bool) -> Result<bool, FlowableGatewayError>;

    async fn start_process_instance(
        &self,
        process_key: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
        business_key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Option<String>, FlowableGatewayError>;

    async fn get_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError>;

    async fn get_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError>;

    async fn delete_process_instance(
        &self,
        process_instance_id: &str,
        delete_reason: Option<&str>,
    ) -> Result<bool, FlowableGatewayError>;

    async fn get_tasks(&self, filters: &[(&str, String)]) -> Result<Vec<serde_json::Value>, FlowableGatewayError>;

    async fn get_task(&self, task_id: &str) -> Result<Option<serde_json::Value>, FlowableGatewayError>;

    async fn claim_task(&self, task_id: &str, user_id: &str) -> Result<bool, FlowableGatewayError>;

    async fn unclaim_task(&self, task_id: &str) -> Result<bool, FlowableGatewayError>;

    async fn complete_task(
        &self,
        task_id: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<bool, FlowableGatewayError>;

    async fn get_executions(&self, filters: &[(&str, String)]) -> Result<Vec<serde_json::Value>, FlowableGatewayError>;

    async fn get_process_instance_variables(
        &self,
        process_instance_id: &str,
    ) -> Result<serde_json::Value, FlowableGatewayError>;

    async fn set_process_instance_variables(
        &self,
        process_instance_id: &str,
        variables: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<bool, FlowableGatewayError>;

    async fn get_historic_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError>;

    async fn get_historic_tasks(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError>;

    async fn get_historic_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableGatewayError>;

    async fn get_historic_variable_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableGatewayError>;
}
