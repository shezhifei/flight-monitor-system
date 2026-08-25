//! 待办事项仓储 trait

use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::models::todo::{Todo, TodoPriority, TodoStatus};
use async_trait::async_trait;

/// 待办事项仓储接口
#[async_trait]
pub trait TodoRepository {
    async fn find_by_id(&self, todo_id: &str) -> Result<Option<Todo>, DomainError>;

    async fn find_all(
        &self,
        status: Option<TodoStatus>,
        priority: Option<TodoPriority>,
        category: Option<&str>,
        assigned_to: Option<&str>,
        source_type: Option<&str>,
        source_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Todo>, DomainError>;

    async fn find_by_ids(&self, todo_ids: &[String]) -> Result<Vec<Todo>, DomainError>;

    async fn find_by_source(&self, source_type: &str, source_id: &str) -> Result<Vec<Todo>, DomainError>;

    async fn find_overdue(&self) -> Result<Vec<Todo>, DomainError>;

    async fn find_children(&self, parent_todo_id: &str) -> Result<Vec<Todo>, DomainError>;

    async fn save(&self, todo: &Todo) -> Result<(), DomainError>;

    async fn update(&self, todo: &Todo) -> Result<bool, DomainError>;

    async fn soft_delete(&self, todo_id: &str, deleted_by: &str) -> Result<bool, DomainError>;

    async fn count_by_status(&self, status: TodoStatus) -> Result<i64, DomainError>;

    // Batch smoke-cleanup operations
    async fn count_by_source_ids(
        &self,
        source_type: &str,
        source_ids: &[String],
        cutoff: DateTime<Utc>,
    ) -> Result<i64, DomainError>;

    /// 单条 UPDATE。它原先挂在 `TodoTransactionalRepository` 上并要求调用方开事务，
    /// 但事务里只有这一条语句——单语句在 Postgres 本来就是原子的，那个事务是纯仪式。
    /// 它的兄弟方法 `count_by_source_ids` 一直就在这里。
    async fn soft_delete_by_source_ids(
        &self,
        source_type: &str,
        source_ids: &[String],
        cutoff: DateTime<Utc>,
    ) -> Result<u64, DomainError>;
}

#[async_trait]
pub trait TodoTransactionalRepository<Tx>: Send + Sync {
    async fn save_in_tx(&self, tx: &mut Tx, todo: &Todo) -> Result<(), DomainError>;

    async fn update_in_tx(&self, tx: &mut Tx, todo: &Todo) -> Result<bool, DomainError>;

}
