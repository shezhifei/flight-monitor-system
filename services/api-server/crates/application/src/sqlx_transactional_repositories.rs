//! SQLx transaction-aware repository ports used by application services.
//!
//! Domain repository traits stay storage-agnostic. Application services that
//! orchestrate a SQL transaction depend on these local composite traits, while
//! infrastructure repositories implement the generic domain transactional
//! traits for `sqlx::Transaction`.

use fms_domain::ports::anomaly_repository::AnomalyTransactionalRepository;
use fms_domain::ports::business_case_repository::BusinessCaseTransactionalRepository;
use fms_domain::ports::dispatch_repository::{
    DispatchOrderMemberTransactionalRepository, DispatchOrderTransactionalRepository,
};
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
use fms_domain::ports::flight_repository::FlightTransactionalRepository;
use fms_domain::ports::flight_timeline_event_repository::FlightTimelineEventTransactionalRepository;
use fms_domain::ports::notification_repository::NotificationTransactionalRepository;
use fms_domain::ports::ontology_repository::OntologyTransactionalRepository;
use fms_domain::ports::todo_repository::TodoTransactionalRepository;
use sqlx::{Postgres, Transaction};

pub trait SqlxFlightTransactionalRepository:
    for<'tx> FlightTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxFlightTransactionalRepository for T where
    T: for<'tx> FlightTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxTodoTransactionalRepository:
    for<'tx> TodoTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxTodoTransactionalRepository for T where
    T: for<'tx> TodoTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxNotificationTransactionalRepository:
    for<'tx> NotificationTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxNotificationTransactionalRepository for T where
    T: for<'tx> NotificationTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxAnomalyTransactionalRepository:
    for<'tx> AnomalyTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxAnomalyTransactionalRepository for T where
    T: for<'tx> AnomalyTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxBusinessCaseTransactionalRepository:
    for<'tx> BusinessCaseTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxBusinessCaseTransactionalRepository for T where
    T: for<'tx> BusinessCaseTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxDispatchOrderTransactionalRepository:
    for<'tx> DispatchOrderTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxDispatchOrderTransactionalRepository for T where
    T: for<'tx> DispatchOrderTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxDispatchOrderMemberTransactionalRepository:
    for<'tx> DispatchOrderMemberTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxDispatchOrderMemberTransactionalRepository for T where
    T: for<'tx> DispatchOrderMemberTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxFlightTimelineTransactionalRepository:
    for<'tx> FlightTimelineEventTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxFlightTimelineTransactionalRepository for T where
    T: for<'tx> FlightTimelineEventTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxOntologyTransactionalRepository:
    for<'tx> OntologyTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxOntologyTransactionalRepository for T where
    T: for<'tx> OntologyTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

pub trait SqlxDomainEventOutboxTransactionalRepository:
    for<'tx> DomainEventOutboxTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}

impl<T> SqlxDomainEventOutboxTransactionalRepository for T where
    T: for<'tx> DomainEventOutboxTransactionalRepository<Transaction<'tx, Postgres>> + Send + Sync
{
}
