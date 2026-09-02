//! Atomic Qualification catalog owner + attribute-reference writer.

use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DepartmentQualificationCatalog;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::DepartmentQualificationTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

#[async_trait]
pub trait QualificationAttributeTransactionalWriter: Send + Sync {
    async fn save_catalog_with_references(
        &self,
        catalog: &DepartmentQualificationCatalog,
        references: &[OntologyAttributeReference],
    ) -> Result<DepartmentQualificationCatalog, DomainError>;
}

pub struct UowQualificationAttributeWriter<U: UnitOfWork> {
    qualification_repo: Arc<dyn DepartmentQualificationTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowQualificationAttributeWriter<U> {
    pub fn new(
        qualification_repo: Arc<dyn DepartmentQualificationTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self { qualification_repo, reference_repo, uow }
    }
}

#[async_trait]
impl<U> QualificationAttributeTransactionalWriter for UowQualificationAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_catalog_with_references(
        &self,
        catalog: &DepartmentQualificationCatalog,
        references: &[OntologyAttributeReference],
    ) -> Result<DepartmentQualificationCatalog, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.qualification_repo.save_catalog_in_tx(&mut tx, catalog).await?;
        self.reference_repo
            .replace_owner_references_in_tx(&mut tx, "Qualification", &saved.id, references)
            .await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }
}
