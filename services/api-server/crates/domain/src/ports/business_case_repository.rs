//! 业务事项仓储 trait

use crate::error::DomainError;
use crate::models::business_case::{BusinessCaseAppendEntry, BusinessCaseType, FlightBusinessCase};
use async_trait::async_trait;

#[async_trait]
pub trait BusinessCaseRepository {
    async fn save(&self, case: &FlightBusinessCase) -> Result<(), DomainError>;
    async fn find_by_id(&self, case_id: &str) -> Result<Option<FlightBusinessCase>, DomainError>;
    async fn find_by_id_scoped(
        &self,
        case_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Option<FlightBusinessCase>, DomainError>;
    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_by_flight_scoped(
        &self,
        flight_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_by_flight_ids(&self, flight_ids: &[String]) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_by_copilot_batch_action(
        &self,
        batch_id: &str,
        action_id: &str,
    ) -> Result<Option<FlightBusinessCase>, DomainError>;
    async fn list_by_copilot_batch(&self, batch_id: &str) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_by_flight_ids_scoped(
        &self,
        flight_ids: &[String],
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_all(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_all_scoped(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_filtered(
        &self,
        flight_id: Option<&str>,
        case_type: Option<&str>,
        status: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn find_filtered_scoped(
        &self,
        flight_id: Option<&str>,
        case_type: Option<&str>,
        status: Option<&str>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError>;
    async fn update_case(&self, case: &FlightBusinessCase) -> Result<bool, DomainError>;
    async fn update_status(&self, case_id: &str, status: &str, actor: &str) -> Result<bool, DomainError>;
    async fn insert_append(&self, append: &BusinessCaseAppendEntry) -> Result<BusinessCaseAppendEntry, DomainError>;
    async fn insert_append_once(
        &self,
        append: &BusinessCaseAppendEntry,
    ) -> Result<(BusinessCaseAppendEntry, bool), DomainError>;
    async fn find_append_by_id(&self, append_id: &str) -> Result<Option<BusinessCaseAppendEntry>, DomainError>;
    async fn update_append_metadata(&self, append_id: &str, metadata: serde_json::Value) -> Result<bool, DomainError>;
    async fn delete(&self, case_id: &str) -> Result<bool, DomainError>;
}

#[async_trait]
pub trait BusinessCaseTransactionalRepository<Tx>: Send + Sync {
    async fn save_in_tx(&self, tx: &mut Tx, case: &FlightBusinessCase) -> Result<(), DomainError>;

    async fn update_case_in_tx(&self, tx: &mut Tx, case: &FlightBusinessCase) -> Result<bool, DomainError>;
}

#[async_trait]
pub trait BusinessCaseTypeRepository {
    async fn find_all(&self, active_only: bool) -> Result<Vec<BusinessCaseType>, DomainError>;
    async fn find_all_scoped(
        &self,
        active_only: bool,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Vec<BusinessCaseType>, DomainError>;
    async fn find_by_code(&self, code: &str) -> Result<Option<BusinessCaseType>, DomainError>;
    async fn find_by_code_scoped(
        &self,
        code: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
        include_common: bool,
    ) -> Result<Option<BusinessCaseType>, DomainError>;
    async fn save(&self, entity: &BusinessCaseType) -> Result<BusinessCaseType, DomainError>;
    async fn update_bpmn_xml(&self, code: &str, bpmn_xml: &str, description: Option<&str>)
        -> Result<bool, DomainError>;
    async fn update_status(&self, code: &str, is_active: bool) -> Result<bool, DomainError>;
    async fn update_ai_extraction_config(
        &self,
        code: &str,
        config: &serde_json::Value,
    ) -> Result<Option<BusinessCaseType>, DomainError>;
    async fn update_case_properties(
        &self,
        code: &str,
        properties: &serde_json::Value,
    ) -> Result<Option<BusinessCaseType>, DomainError>;
}
