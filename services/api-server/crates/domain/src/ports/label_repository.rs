//! 标签仓储 trait。

use crate::error::DomainError;
use crate::models::label::{LabelDefinition, LabelScope};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CreateLabelDefinitionParams {
    pub code: String,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub scope: LabelScope,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateLabelDefinitionParams {
    pub name: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub is_active: Option<bool>,
    pub sort_order: Option<i32>,
}

#[async_trait]
pub trait LabelRepository {
    async fn get_all_definitions(&self, active_only: bool) -> Result<Vec<LabelDefinition>, DomainError>;

    async fn get_definition_by_code(&self, code: &str) -> Result<Option<LabelDefinition>, DomainError>;

    async fn create_definition(&self, params: CreateLabelDefinitionParams) -> Result<LabelDefinition, DomainError>;

    async fn update_definition(&self, label_id: &str, params: UpdateLabelDefinitionParams)
        -> Result<bool, DomainError>;

    async fn delete_definition(&self, label_id: &str) -> Result<bool, DomainError>;

    async fn attach_flight_label(&self, flight_id: &str, code: &str) -> Result<(), DomainError>;

    async fn detach_flight_label(&self, flight_id: &str, code: &str) -> Result<(), DomainError>;

    async fn attach_leg_label(&self, flight_id: &str, leg_type: &str, code: &str) -> Result<(), DomainError>;

    async fn detach_leg_label(&self, flight_id: &str, leg_type: &str, code: &str) -> Result<(), DomainError>;

    async fn get_flight_labels(&self, flight_id: &str) -> Result<Vec<String>, DomainError>;

    async fn get_leg_labels(&self, flight_id: &str, leg_type: &str) -> Result<Vec<String>, DomainError>;
}
