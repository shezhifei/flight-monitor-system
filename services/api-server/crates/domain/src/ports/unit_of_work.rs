//! 工作单元端口。
//!
//! 应用层用它开启并提交事务，而不知道事务背后是哪种数据库。
//!
//! `Tx` 是**关联类型**而不是 trait 的泛型参数，这是本端口能表达「不含数据库类型的事务」的关键：
//! 句柄的具体类型由适配层选定（Postgres 适配器里是 `sqlx::Transaction<'static, Postgres>`），
//! 应用层只把 `&mut Self::Tx` 转交给仓储端口——而 `XxxTransactionalRepository<Tx>`
//! 本来就已经对 `Tx` 泛型，所以不需要为此改动任何既有仓储端口。
//!
//! 另一条路是把句柄擦成 `Box<dyn Any>` 再在适配层向下转型，那样应用层能继续用
//! `Arc<dyn UnitOfWork>`、改动面小得多。这里**故意不选**：它把编译期保证换成了运行期
//! panic 路径，而本轮重构的整个目的就是反过来——让错误的接线无法通过编译。
//!
//! 没有 `rollback`：`sqlx::Transaction` 在 drop 时回滚，既有代码正是依赖这一点
//! （失败路径不显式回滚）。加一个没有调用者的方法只会立刻变成死代码。

use async_trait::async_trait;

use crate::error::DomainError;

#[async_trait]
pub trait UnitOfWork: Send + Sync + 'static {
    /// 事务句柄。应用层不检查它，只转交给仓储端口。
    type Tx: Send;

    async fn begin(&self) -> Result<Self::Tx, DomainError>;

    async fn commit(&self, tx: Self::Tx) -> Result<(), DomainError>;
}
