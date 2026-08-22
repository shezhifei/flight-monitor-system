//! 航班监控系统 — 基础设施层
//!
//! 包含所有 I/O 实现：数据库、缓存、外部 API、安全、配置等。

pub mod ai_context_snapshot;
pub mod cache;
pub mod cdc;
pub mod config;
pub mod db;
pub mod error;
pub mod events;
pub mod http_client;
pub mod integrations;
pub mod logging;
pub mod messaging;
pub mod observability;
pub mod repositories;
pub mod security;

pub use repositories::pg_database_metadata_adapter::PgDatabaseMetadataAdapter;
pub use repositories::pg_domain_event_outbox_repository::PgDomainEventOutboxRepository;
