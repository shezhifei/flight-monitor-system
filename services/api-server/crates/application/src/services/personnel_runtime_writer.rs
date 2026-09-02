//! Atomic personnel runtime + object-reference index writer.

use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::PersonnelRuntime;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::PersonnelRuntimeTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

use async_trait::async_trait;

#[async_trait]
pub trait PersonnelRuntimeAttributeTransactionalWriter: Send + Sync {
    async fn save_with_references(
        &self,
        runtime: &PersonnelRuntime,
        references: &[OntologyAttributeReference],
    ) -> Result<(), DomainError>;
}

pub struct UowPersonnelRuntimeAttributeWriter<U: UnitOfWork> {
    runtime_repo: Arc<dyn PersonnelRuntimeTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowPersonnelRuntimeAttributeWriter<U> {
    pub fn new(
        runtime_repo: Arc<dyn PersonnelRuntimeTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            runtime_repo,
            reference_repo,
            uow,
        }
    }
}

#[async_trait]
impl<U> PersonnelRuntimeAttributeTransactionalWriter for UowPersonnelRuntimeAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_with_references(
        &self,
        runtime: &PersonnelRuntime,
        references: &[OntologyAttributeReference],
    ) -> Result<(), DomainError> {
        let mut tx = self.uow.begin().await?;
        self.runtime_repo.save_in_tx(&mut tx, runtime).await?;
        self.reference_repo
            .replace_owner_references_in_tx(
                &mut tx,
                "Personnel",
                &runtime.user_id,
                references,
            )
            .await?;
        self.uow.commit(tx).await
    }
}
