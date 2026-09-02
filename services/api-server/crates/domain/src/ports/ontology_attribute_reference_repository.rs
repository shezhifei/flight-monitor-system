use async_trait::async_trait;

use crate::error::DomainError;
use crate::models::ontology_attribute_reference::OntologyAttributeReference;

#[async_trait]
pub trait OntologyAttributeReferenceRepository: Send + Sync {
    async fn replace_owner_references(
        &self,
        owner_object_name: &str,
        owner_object_id: &str,
        references: &[OntologyAttributeReference],
    ) -> Result<(), DomainError>;

    async fn find_by_target(
        &self,
        target_object_name: &str,
        target_key: &str,
    ) -> Result<Vec<OntologyAttributeReference>, DomainError>;
}

/// Transactional companion used when an owner write and its reference-index
/// projection must commit atomically.  The non-transactional port above stays
/// object-safe for application services and test doubles; concrete adapters
/// opt into this typed port for their UnitOfWork transaction handle.
#[async_trait]
pub trait OntologyAttributeReferenceTransactionalRepository<Tx>: Send + Sync {
    async fn replace_owner_references_in_tx(
        &self,
        tx: &mut Tx,
        owner_object_name: &str,
        owner_object_id: &str,
        references: &[OntologyAttributeReference],
    ) -> Result<(), DomainError>;

    async fn find_by_target_in_tx(
        &self,
        tx: &mut Tx,
        target_object_name: &str,
        target_key: &str,
    ) -> Result<Vec<OntologyAttributeReference>, DomainError>;
}
