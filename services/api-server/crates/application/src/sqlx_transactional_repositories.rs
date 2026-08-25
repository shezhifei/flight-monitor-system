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
    FlightTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxFlightTransactionalRepository for T where
    T: FlightTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxTodoTransactionalRepository:
    TodoTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxTodoTransactionalRepository for T where
    T: TodoTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxNotificationTransactionalRepository:
    NotificationTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxNotificationTransactionalRepository for T where
    T: NotificationTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxAnomalyTransactionalRepository:
    AnomalyTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxAnomalyTransactionalRepository for T where
    T: AnomalyTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxBusinessCaseTransactionalRepository:
    BusinessCaseTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxBusinessCaseTransactionalRepository for T where
    T: BusinessCaseTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxDispatchOrderTransactionalRepository:
    DispatchOrderTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxDispatchOrderTransactionalRepository for T where
    T: DispatchOrderTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxDispatchOrderMemberTransactionalRepository:
    DispatchOrderMemberTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxDispatchOrderMemberTransactionalRepository for T where
    T: DispatchOrderMemberTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxFlightTimelineTransactionalRepository:
    FlightTimelineEventTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxFlightTimelineTransactionalRepository for T where
    T: FlightTimelineEventTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxOntologyTransactionalRepository:
    OntologyTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxOntologyTransactionalRepository for T where
    T: OntologyTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

pub trait SqlxDomainEventOutboxTransactionalRepository:
    DomainEventOutboxTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}

impl<T> SqlxDomainEventOutboxTransactionalRepository for T where
    T: DomainEventOutboxTransactionalRepository<Transaction<'static, Postgres>> + Send + Sync
{
}
