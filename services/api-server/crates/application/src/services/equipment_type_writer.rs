//! Atomic EquipmentType owner + attribute-reference writer.

use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::EquipmentType;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::EquipmentTypeTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

#[async_trait]
pub trait EquipmentTypeAttributeTransactionalWriter: Send + Sync {
    async fn save_with_references(
        &self,
        equipment_type: &EquipmentType,
        references: &[OntologyAttributeReference],
    ) -> Result<EquipmentType, DomainError>;
}

pub struct UowEquipmentTypeAttributeWriter<U: UnitOfWork> {
    equipment_type_repo: Arc<dyn EquipmentTypeTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowEquipmentTypeAttributeWriter<U> {
    pub fn new(
        equipment_type_repo: Arc<dyn EquipmentTypeTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            equipment_type_repo,
            reference_repo,
            uow,
        }
    }
}

#[async_trait]
impl<U> EquipmentTypeAttributeTransactionalWriter for UowEquipmentTypeAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_with_references(
        &self,
        equipment_type: &EquipmentType,
        references: &[OntologyAttributeReference],
    ) -> Result<EquipmentType, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.equipment_type_repo.save_in_tx(&mut tx, equipment_type).await?;
        self.reference_repo
            .replace_owner_references_in_tx(&mut tx, "EquipmentType", &saved.id, references)
            .await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }
}
