//! Atomic TaskType owner + attribute-reference writer.

use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::TaskType;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::TaskTypeTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

#[async_trait]
pub trait TaskTypeAttributeTransactionalWriter: Send + Sync {
    async fn save_with_references(
        &self,
        task_type: &TaskType,
        references: &[OntologyAttributeReference],
    ) -> Result<TaskType, DomainError>;
}

pub struct UowTaskTypeAttributeWriter<U: UnitOfWork> {
    task_type_repo: Arc<dyn TaskTypeTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowTaskTypeAttributeWriter<U> {
    pub fn new(
        task_type_repo: Arc<dyn TaskTypeTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            task_type_repo,
            reference_repo,
            uow,
        }
    }
}

#[async_trait]
impl<U> TaskTypeAttributeTransactionalWriter for UowTaskTypeAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_with_references(
        &self,
        task_type: &TaskType,
        references: &[OntologyAttributeReference],
    ) -> Result<TaskType, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.task_type_repo.save_in_tx(&mut tx, task_type).await?;
        self.reference_repo
            .replace_owner_references_in_tx(&mut tx, "TaskType", &saved.id, references)
            .await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }
}
