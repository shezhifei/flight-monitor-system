//! 逻辑复制（CDC）管理端口。
//!
//! CDC relay 服务需要保证 publication 与 replication slot 存在。这两件事必须在
//! 数据库里做，但**做什么**是应用层的决定，**怎么做**不是。这个端口只用 `&str` 和
//! `DomainError` 表达意图，签名里没有任何数据库类型；`pg_catalog` 查询、`CREATE
//! PUBLICATION` / `pg_create_logical_replication_slot` 都留在 infrastructure。
//!
//! 它取代的是 application 直接持有 `fms_infrastructure::cdc::PgCdcAdmin`——那是分层
//! 反向，也是 `application → fms-infrastructure` 这条依赖边最后两个生产使用者之一。

use async_trait::async_trait;

use crate::error::DomainError;

#[async_trait]
pub trait CdcAdminPort: Send + Sync {
    /// 确保指定 publication 存在；已存在时是空操作。
    async fn ensure_publication_exists(&self, publication_name: &str) -> Result<(), DomainError>;

    /// 确保指定逻辑复制 slot 存在；已存在但绑定到别的库时必须报错而不是静默复用。
    async fn ensure_replication_slot(&self, slot_name: &str, expected_database: &str) -> Result<(), DomainError>;
}
