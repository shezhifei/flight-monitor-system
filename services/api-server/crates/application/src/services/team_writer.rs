//! Atomic Team owner + attribute-reference writer.

use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::Team;
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::TeamTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

#[async_trait]
pub trait TeamAttributeTransactionalWriter: Send + Sync {
    async fn save_with_references(
        &self,
        team: &Team,
        references: &[OntologyAttributeReference],
    ) -> Result<Team, DomainError>;
}

pub struct UowTeamAttributeWriter<U: UnitOfWork> {
    team_repo: Arc<dyn TeamTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowTeamAttributeWriter<U> {
    pub fn new(
        team_repo: Arc<dyn TeamTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            team_repo,
            reference_repo,
            uow,
        }
    }
}

#[async_trait]
impl<U> TeamAttributeTransactionalWriter for UowTeamAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_with_references(
        &self,
        team: &Team,
        references: &[OntologyAttributeReference],
    ) -> Result<Team, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.team_repo.save_in_tx(&mut tx, team).await?;
        self.reference_repo
            .replace_owner_references_in_tx(&mut tx, "Team", &saved.id, references)
            .await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }
}
