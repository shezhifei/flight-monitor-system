//! Atomic Department owner + attribute-reference writer.

use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::Department;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::DepartmentTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

#[async_trait]
pub trait DepartmentAttributeTransactionalWriter: Send + Sync {
    async fn save_with_references(
        &self,
        department: &Department,
        references: &[OntologyAttributeReference],
    ) -> Result<Department, DomainError>;
}

pub struct UowDepartmentAttributeWriter<U: UnitOfWork> {
    department_repo: Arc<dyn DepartmentTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowDepartmentAttributeWriter<U> {
    pub fn new(
        department_repo: Arc<dyn DepartmentTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            department_repo,
            reference_repo,
            uow,
        }
    }
}

#[async_trait]
impl<U> DepartmentAttributeTransactionalWriter for UowDepartmentAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_with_references(
        &self,
        department: &Department,
        references: &[OntologyAttributeReference],
    ) -> Result<Department, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.department_repo.save_in_tx(&mut tx, department).await?;
        self.reference_repo
            .replace_owner_references_in_tx(&mut tx, "Department", &saved.id, references)
            .await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }
}
