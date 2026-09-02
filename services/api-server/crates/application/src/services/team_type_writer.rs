//! Atomic TeamType owner + attribute-reference writer.

use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::TeamType;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::TeamTypeTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

#[async_trait]
pub trait TeamTypeAttributeTransactionalWriter: Send + Sync {
    async fn save_with_references(
        &self,
        team_type: &TeamType,
        references: &[OntologyAttributeReference],
    ) -> Result<TeamType, DomainError>;
}

pub struct UowTeamTypeAttributeWriter<U: UnitOfWork> {
    team_type_repo: Arc<dyn TeamTypeTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowTeamTypeAttributeWriter<U> {
    pub fn new(
        team_type_repo: Arc<dyn TeamTypeTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self { team_type_repo, reference_repo, uow }
    }
}

#[async_trait]
impl<U> TeamTypeAttributeTransactionalWriter for UowTeamTypeAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_with_references(
        &self,
        team_type: &TeamType,
        references: &[OntologyAttributeReference],
    ) -> Result<TeamType, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.team_type_repo.save_in_tx(&mut tx, team_type).await?;
        self.reference_repo
            .replace_owner_references_in_tx(&mut tx, "TeamType", &saved.id, references)
            .await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }
}
