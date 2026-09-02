//! Atomic Equipment owner + attribute-reference writer.

use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::Equipment;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::EquipmentTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

#[async_trait]
pub trait EquipmentAttributeTransactionalWriter: Send + Sync {
    async fn save_with_references(
        &self,
        equipment: &Equipment,
        references: &[OntologyAttributeReference],
    ) -> Result<Equipment, DomainError>;
}

pub struct UowEquipmentAttributeWriter<U: UnitOfWork> {
    equipment_repo: Arc<dyn EquipmentTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowEquipmentAttributeWriter<U> {
    pub fn new(
        equipment_repo: Arc<dyn EquipmentTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            equipment_repo,
            reference_repo,
            uow,
        }
    }
}

#[async_trait]
impl<U> EquipmentAttributeTransactionalWriter for UowEquipmentAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_with_references(
        &self,
        equipment: &Equipment,
        references: &[OntologyAttributeReference],
    ) -> Result<Equipment, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.equipment_repo.save_in_tx(&mut tx, equipment).await?;
        self.reference_repo
            .replace_owner_references_in_tx(&mut tx, "Equipment", &saved.id, references)
            .await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }
}
