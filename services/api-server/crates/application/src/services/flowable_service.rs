use std::sync::Arc;

use fms_domain::ports::flowable_gateway::{FlowableGateway, FlowableGatewayError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlowableServiceError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Upstream(String),
}

#[derive(Clone)]
pub struct FlowableService {
    client: Arc<dyn FlowableGateway>,
}

impl FlowableService {
    pub fn new(client: Arc<dyn FlowableGateway>) -> Self {
        Self { client }
    }

    pub async fn list_process_definitions(
        &self,
        key: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableServiceError> {
        self.client
            .get_process_definitions(key, tenant_id)
            .await
            .map_err(map_client_error)
    }

    pub async fn get_process_definition(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableServiceError> {
        self.client
            .get_process_definition(process_definition_id)
            .await
            .map_err(map_client_error)
    }

    pub async fn get_process_definition_xml(
        &self,
        process_definition_id: &str,
    ) -> Result<Option<String>, FlowableServiceError> {
        self.client
            .get_process_definition_xml(process_definition_id)
            .await
            .map_err(map_client_error)
    }

    pub async fn create_deployment(
        &self,
        bpmn_xml: &str,
        deployment_name: Option<&str>,
        category: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<serde_json::Value, FlowableServiceError> {
        self.client
            .deploy_process(bpmn_xml, deployment_name, category, tenant_id)
            .await
            .map_err(map_client_error)
    }

    pub async fn deploy_process_definition(
        &self,
        bpmn_content: &str,
        filename: &str,
    ) -> Result<serde_json::Value, FlowableServiceError> {
        self.create_deployment(bpmn_content, Some(filename), None, None).await
    }

    pub async fn list_deployments(
        &self,
        name: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, FlowableServiceError> {
        self.client
            .get_deployments(name, tenant_id)
            .await
            .map_err(map_client_error)
    }

    pub async fn delete_deployment(&self, deployment_id: &str, cascade: bool) -> Result<bool, FlowableServiceError> {
        self.client
            .delete_deployment(deployment_id, cascade)
            .await
            .map_err(map_client_error)
    }

    pub async fn start_process_instance(
        &self,
        process_key: &str,
        business_key: Option<&str>,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
        tenant_id: Option<&str>,
    ) -> Result<Option<String>, FlowableServiceError> {
        if process_key.trim().is_empty() {
            return Err(FlowableServiceError::Validation("process_key is required".to_string()));
        }
        self.client
            .start_process_instance(process_key, variables, business_key, tenant_id)
            .await
            .map_err(map_client_error)
    }

    pub async fn list_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableServiceError> {
        self.client
            .get_process_instances(filters)
            .await
            .map_err(map_client_error)
    }

    pub async fn get_process_instance(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableServiceError> {
        self.client
            .get_process_instance(process_instance_id)
            .await
            .map_err(map_client_error)
    }

    pub async fn delete_process_instance(
        &self,
        process_instance_id: &str,
        delete_reason: Option<&str>,
    ) -> Result<bool, FlowableServiceError> {
        self.client
            .delete_process_instance(process_instance_id, delete_reason)
            .await
            .map_err(map_client_error)
    }

    pub async fn list_tasks(&self, filters: &[(&str, String)]) -> Result<Vec<serde_json::Value>, FlowableServiceError> {
        self.client.get_tasks(filters).await.map_err(map_client_error)
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<serde_json::Value>, FlowableServiceError> {
        self.client.get_task(task_id).await.map_err(map_client_error)
    }

    pub async fn claim_task(&self, task_id: &str, user_id: &str) -> Result<bool, FlowableServiceError> {
        if user_id.trim().is_empty() {
            return Err(FlowableServiceError::Validation("user_id is required".to_string()));
        }
        self.client.claim_task(task_id, user_id).await.map_err(map_client_error)
    }

    pub async fn unclaim_task(&self, task_id: &str) -> Result<bool, FlowableServiceError> {
        self.client.unclaim_task(task_id).await.map_err(map_client_error)
    }

    pub async fn complete_task(
        &self,
        task_id: &str,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<bool, FlowableServiceError> {
        self.client
            .complete_task(task_id, variables)
            .await
            .map_err(map_client_error)
    }

    pub async fn start_process_with_subprocess(
        &self,
        process_key: &str,
        business_key: Option<&str>,
        variables: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<Option<String>, FlowableServiceError> {
        self.start_process_instance(process_key, business_key, variables, None)
            .await
    }

    pub async fn get_subprocess_executions(
        &self,
        process_instance_id: &str,
    ) -> Result<Vec<serde_json::Value>, FlowableServiceError> {
        self.client
            .get_executions(&[("processInstanceId", process_instance_id.to_string())])
            .await
            .map_err(map_client_error)
    }

    pub async fn get_subprocess_result(
        &self,
        process_instance_id: &str,
    ) -> Result<Option<serde_json::Value>, FlowableServiceError> {
        let Some(instance) = self
            .client
            .get_historic_process_instance(process_instance_id)
            .await
            .map_err(map_client_error)?
        else {
            return Ok(None);
        };
        let variables = self
            .client
            .get_historic_variable_instances(&[("processInstanceId", process_instance_id.to_string())])
            .await
            .map_err(map_client_error)?;
        let mut output_variables = serde_json::Map::new();
        for mut variable in variables {
            if let Some(name) = variable
                .get("variableName")
                .and_then(serde_json::Value::as_str)
                .or_else(|| variable.get("name").and_then(serde_json::Value::as_str))
                .map(str::to_string)
            {
                let value = variable
                    .as_object_mut()
                    .and_then(|object| object.remove("value"))
                    .unwrap_or(serde_json::Value::Null);
                output_variables.insert(name, value);
            }
        }
        Ok(Some(serde_json::json!({
            "subprocess_instance_id": process_instance_id,
            "process_definition_id": instance.get("processDefinitionId").unwrap_or(&serde_json::Value::Null),
            "process_definition_key": instance.get("processDefinitionKey").unwrap_or(&serde_json::Value::Null),
            "start_time": instance.get("startTime").unwrap_or(&serde_json::Value::Null),
            "end_time": instance.get("endTime").unwrap_or(&serde_json::Value::Null),
            "duration_in_millis": instance.get("durationInMillis").unwrap_or(&serde_json::Value::Null),
            "end_activity_id": instance.get("endActivityId").unwrap_or(&serde_json::Value::Null),
            "delete_reason": instance.get("deleteReason").unwrap_or(&serde_json::Value::Null),
            "super_process_instance_id": instance.get("superProcessInstanceId").unwrap_or(&serde_json::Value::Null),
            "output_variables": output_variables,
        })))
    }

    pub async fn get_process_instance_variables(
        &self,
        process_instance_id: &str,
    ) -> Result<serde_json::Value, FlowableServiceError> {
        self.client
            .get_process_instance_variables(process_instance_id)
            .await
            .map_err(map_client_error)
    }

    pub async fn set_process_instance_variables(
        &self,
        process_instance_id: &str,
        variables: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<bool, FlowableServiceError> {
        self.client
            .set_process_instance_variables(process_instance_id, variables)
            .await
            .map_err(map_client_error)
    }

    pub async fn list_historic_process_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableServiceError> {
        self.client
            .get_historic_process_instances(filters)
            .await
            .map_err(map_client_error)
    }

    pub async fn list_historic_tasks(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableServiceError> {
        self.client.get_historic_tasks(filters).await.map_err(map_client_error)
    }

    pub async fn list_historic_variable_instances(
        &self,
        filters: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, FlowableServiceError> {
        self.client
            .get_historic_variable_instances(filters)
            .await
            .map_err(map_client_error)
    }

    pub async fn health(&self) -> Result<serde_json::Value, FlowableServiceError> {
        let definitions = self.list_process_definitions(None, None).await?;
        Ok(serde_json::json!({
            "status": "healthy",
            "message": "Flowable REST API 正常",
            "process_definitions_count": definitions.len(),
        }))
    }
}

fn map_client_error(error: FlowableGatewayError) -> FlowableServiceError {
    match error {
        FlowableGatewayError::NotFound => FlowableServiceError::NotFound("resource not found".to_string()),
        FlowableGatewayError::Upstream(message) => FlowableServiceError::Upstream(message),
    }
}
