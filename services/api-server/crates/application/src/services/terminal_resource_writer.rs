//! Atomic directory owner + attribute-reference writers for spatial resources.

use std::sync::Arc;

use async_trait::async_trait;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{BaggageCarousel, Gate, Stand, Terminal};
use fms_domain::models::ontology_attribute_reference::OntologyAttributeReference;
use fms_domain::ports::dispatch_repository::TerminalResourceTransactionalRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceTransactionalRepository;
use fms_domain::ports::unit_of_work::UnitOfWork;

#[async_trait]
pub trait TerminalResourceAttributeTransactionalWriter: Send + Sync {
    async fn save_terminal_with_references(
        &self,
        terminal: &Terminal,
        references: &[OntologyAttributeReference],
    ) -> Result<Terminal, DomainError>;
    async fn save_gate_with_references(
        &self,
        gate: &Gate,
        references: &[OntologyAttributeReference],
    ) -> Result<Gate, DomainError>;
    async fn save_gate_with_terminal_and_references(
        &self,
        terminal_id: &str,
        gate: &Gate,
        references: &[OntologyAttributeReference],
    ) -> Result<Gate, DomainError>;
    async fn save_carousel_with_references(
        &self,
        carousel: &BaggageCarousel,
        references: &[OntologyAttributeReference],
    ) -> Result<BaggageCarousel, DomainError>;
    async fn save_carousel_with_terminal_and_references(
        &self,
        terminal_id: &str,
        carousel: &BaggageCarousel,
        references: &[OntologyAttributeReference],
    ) -> Result<BaggageCarousel, DomainError>;
    async fn save_stand_with_references(
        &self,
        stand: &Stand,
        references: &[OntologyAttributeReference],
    ) -> Result<Stand, DomainError>;
    async fn save_stand_with_terminal_and_references(
        &self,
        terminal_id: &str,
        stand: &Stand,
        references: &[OntologyAttributeReference],
    ) -> Result<Stand, DomainError>;
}

pub struct UowTerminalResourceAttributeWriter<U: UnitOfWork> {
    resource_repo: Arc<dyn TerminalResourceTransactionalRepository<U::Tx> + Send + Sync>,
    reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> UowTerminalResourceAttributeWriter<U> {
    pub fn new(
        resource_repo: Arc<dyn TerminalResourceTransactionalRepository<U::Tx> + Send + Sync>,
        reference_repo: Arc<dyn OntologyAttributeReferenceTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self { resource_repo, reference_repo, uow }
    }
}

#[async_trait]
impl<U> TerminalResourceAttributeTransactionalWriter for UowTerminalResourceAttributeWriter<U>
where
    U: UnitOfWork,
    U::Tx: Send,
{
    async fn save_terminal_with_references(
        &self,
        terminal: &Terminal,
        references: &[OntologyAttributeReference],
    ) -> Result<Terminal, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.resource_repo.save_terminal_in_tx(&mut tx, terminal).await?;
        self.reference_repo.replace_owner_references_in_tx(&mut tx, "Terminal", &saved.terminal_id, references).await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }

    async fn save_gate_with_references(
        &self,
        gate: &Gate,
        references: &[OntologyAttributeReference],
    ) -> Result<Gate, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.resource_repo.save_gate_in_tx(&mut tx, gate).await?;
        self.reference_repo.replace_owner_references_in_tx(&mut tx, "Gate", &saved.gate_id, references).await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }

    async fn save_gate_with_terminal_and_references(
        &self,
        terminal_id: &str,
        gate: &Gate,
        references: &[OntologyAttributeReference],
    ) -> Result<Gate, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.resource_repo.save_gate_with_terminal_in_tx(&mut tx, terminal_id, gate).await?;
        self.reference_repo.replace_owner_references_in_tx(&mut tx, "Gate", &saved.gate_id, references).await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }

    async fn save_carousel_with_references(
        &self,
        carousel: &BaggageCarousel,
        references: &[OntologyAttributeReference],
    ) -> Result<BaggageCarousel, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.resource_repo.save_carousel_in_tx(&mut tx, carousel).await?;
        self.reference_repo.replace_owner_references_in_tx(&mut tx, "BaggageCarousel", &saved.carousel_id, references).await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }

    async fn save_carousel_with_terminal_and_references(
        &self,
        terminal_id: &str,
        carousel: &BaggageCarousel,
        references: &[OntologyAttributeReference],
    ) -> Result<BaggageCarousel, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.resource_repo.save_carousel_with_terminal_in_tx(&mut tx, terminal_id, carousel).await?;
        self.reference_repo.replace_owner_references_in_tx(&mut tx, "BaggageCarousel", &saved.carousel_id, references).await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }

    async fn save_stand_with_references(
        &self,
        stand: &Stand,
        references: &[OntologyAttributeReference],
    ) -> Result<Stand, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.resource_repo.save_stand_in_tx(&mut tx, stand).await?;
        self.reference_repo.replace_owner_references_in_tx(&mut tx, "Stand", &saved.id, references).await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }

    async fn save_stand_with_terminal_and_references(
        &self,
        terminal_id: &str,
        stand: &Stand,
        references: &[OntologyAttributeReference],
    ) -> Result<Stand, DomainError> {
        let mut tx = self.uow.begin().await?;
        let saved = self.resource_repo.save_stand_with_terminal_in_tx(&mut tx, terminal_id, stand).await?;
        self.reference_repo.replace_owner_references_in_tx(&mut tx, "Stand", &saved.id, references).await?;
        self.uow.commit(tx).await?;
        Ok(saved)
    }
}
