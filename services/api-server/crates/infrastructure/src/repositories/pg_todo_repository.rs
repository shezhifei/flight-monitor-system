//! PostgreSQL 待办事项仓储实现

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction};

use fms_domain::error::DomainError;
use fms_domain::models::todo::{Todo, TodoCategory, TodoPriority, TodoStatus};
use fms_domain::ports::todo_repository::{TodoRepository, TodoTransactionalRepository};

pub struct PgTodoRepository {
    pool: PgPool,
}

impl PgTodoRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TodoRepository for PgTodoRepository {
    async fn find_by_id(&self, todo_id: &str) -> Result<Option<Todo>, DomainError> {
        let row = sqlx::query("SELECT * FROM todos WHERE todo_id = $1 AND is_deleted = FALSE")
            .bind(todo_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_todo(&r)))
    }

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
    ) -> Result<Vec<Todo>, DomainError> {
        let mut builder = QueryBuilder::<Postgres>::new("SELECT * FROM todos WHERE is_deleted = FALSE");

        if let Some(s) = &status {
            let status_str = match s {
                TodoStatus::Pending => "待办",
                TodoStatus::InProgress => "进行中",
                TodoStatus::Completed => "已完成",
                TodoStatus::Cancelled => "已取消",
                TodoStatus::Blocked => "待办",
            };
            builder.push(" AND status = ");
            builder.push_bind(status_str.to_string());
        }
        if let Some(p) = &priority {
            let priority_str = match p {
                TodoPriority::Critical => "紧急",
                TodoPriority::High => "高",
                TodoPriority::Medium => "中",
                TodoPriority::Low => "低",
                TodoPriority::Background => "低",
            };
            builder.push(" AND priority = ");
            builder.push_bind(priority_str.to_string());
        }
        if let Some(category) = category {
            builder.push(" AND category = ");
            builder.push_bind(normalize_category_value(category));
        }
        if let Some(a) = assigned_to {
            builder.push(" AND assigned_to = ");
            builder.push_bind(a.to_string());
        }
        if let Some(st) = source_type {
            builder.push(" AND source_type = ");
            builder.push_bind(st.to_string());
        }
        if let Some(sid) = source_id {
            builder.push(" AND source_id = ");
            builder.push_bind(sid.to_string());
        }

        builder.push(" ORDER BY created_at DESC LIMIT ");
        builder.push_bind(limit);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_todo).collect())
    }

    async fn find_by_ids(&self, todo_ids: &[String]) -> Result<Vec<Todo>, DomainError> {
        if todo_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query("SELECT * FROM todos WHERE todo_id = ANY($1) AND is_deleted = FALSE")
            .bind(todo_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.iter().map(row_to_todo).collect())
    }

    async fn find_by_source(&self, source_type: &str, source_id: &str) -> Result<Vec<Todo>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM todos WHERE source_type = $1 AND source_id = $2 AND is_deleted = FALSE ORDER BY created_at DESC",
        )
        .bind(source_type)
        .bind(source_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_todo).collect())
    }

    async fn find_overdue(&self) -> Result<Vec<Todo>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM todos WHERE is_deleted = FALSE AND status NOT IN ('completed', 'cancelled') AND due_date < NOW() ORDER BY due_date",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_todo).collect())
    }

    async fn find_children(&self, parent_todo_id: &str) -> Result<Vec<Todo>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM todos WHERE parent_todo_id = $1 AND is_deleted = FALSE ORDER BY execution_order",
        )
        .bind(parent_todo_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_todo).collect())
    }

    async fn save(&self, todo: &Todo) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"INSERT INTO todos (
                todo_id, title, description, priority, status, category,
                due_date, assigned_to, tags, estimated_duration, actual_duration,
                progress, is_recurring, recurring_pattern,
                parent_todo_id, execution_order, depends_on,
                source_type, source_id, is_deleted, deleted_at,
                created_at, updated_at, created_by, updated_by, version
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11,
                $12, $13, $14,
                $15, $16, $17,
                $18, $19, $20, $21,
                $22, $23, $24, $25, $26
            )
            ON CONFLICT (todo_id) DO UPDATE SET
                title = EXCLUDED.title, description = EXCLUDED.description,
                priority = EXCLUDED.priority, status = EXCLUDED.status,
                category = EXCLUDED.category, due_date = EXCLUDED.due_date,
                assigned_to = EXCLUDED.assigned_to, tags = EXCLUDED.tags,
                estimated_duration = EXCLUDED.estimated_duration,
                actual_duration = EXCLUDED.actual_duration,
                progress = EXCLUDED.progress,
                is_recurring = EXCLUDED.is_recurring,
                recurring_pattern = EXCLUDED.recurring_pattern,
                parent_todo_id = EXCLUDED.parent_todo_id,
                execution_order = EXCLUDED.execution_order,
                depends_on = EXCLUDED.depends_on,
                source_type = EXCLUDED.source_type,
                source_id = EXCLUDED.source_id,
                is_deleted = EXCLUDED.is_deleted,
                deleted_at = EXCLUDED.deleted_at,
                updated_at = EXCLUDED.updated_at,
                updated_by = EXCLUDED.updated_by,
                version = EXCLUDED.version
            WHERE todos.version = EXCLUDED.version - 1"#,
        )
        .bind(&todo.todo_id)
        .bind(&todo.title)
        .bind(&todo.description)
        .bind(match todo.priority {
            TodoPriority::Critical => "紧急",
            TodoPriority::High => "高",
            TodoPriority::Medium => "中",
            TodoPriority::Low => "低",
            TodoPriority::Background => "低",
        })
        .bind(match todo.status {
            TodoStatus::Pending => "待办",
            TodoStatus::InProgress => "进行中",
            TodoStatus::Completed => "已完成",
            TodoStatus::Cancelled => "已取消",
            TodoStatus::Blocked => "待办",
        })
        .bind(todo.category.map(|c| c.as_ref().to_string()))
        .bind(todo.due_date)
        .bind(&todo.assigned_to)
        .bind(&todo.tags)
        .bind(todo.estimated_duration)
        .bind(todo.actual_duration)
        .bind(todo.progress)
        .bind(todo.is_recurring)
        .bind(&todo.recurring_pattern)
        .bind(&todo.parent_todo_id)
        .bind(todo.execution_order)
        .bind(&todo.depends_on)
        .bind(&todo.source_type)
        .bind(&todo.source_id)
        .bind(todo.is_deleted)
        .bind(todo.deleted_at)
        .bind(todo.created_at)
        .bind(todo.updated_at)
        .bind(&todo.created_by)
        .bind(&todo.updated_by)
        .bind(todo.version)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::ConcurrencyConflict(
                "Todo was modified concurrently".to_string(),
            ));
        }

        Ok(())
    }

    async fn update(&self, todo: &Todo) -> Result<bool, DomainError> {
        self.save(todo).await?;
        Ok(true)
    }

    async fn soft_delete(&self, todo_id: &str, deleted_by: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
                "UPDATE todos SET is_deleted = TRUE, deleted_at = $1, updated_by = $2, version = version + 1 WHERE todo_id = $3",
            )
            .bind(Utc::now())
            .bind(deleted_by)
            .bind(todo_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_by_status(&self, status: TodoStatus) -> Result<i64, DomainError> {
        let status_str = match status {
            TodoStatus::Pending => "待办",
            TodoStatus::InProgress => "进行中",
            TodoStatus::Completed => "已完成",
            TodoStatus::Cancelled => "已取消",
            TodoStatus::Blocked => "待办",
        };
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM todos WHERE status = $1 AND is_deleted = FALSE")
            .bind(status_str)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.get::<i64, _>("cnt"))
    }

    async fn count_by_source_ids(
        &self,
        source_type: &str,
        source_ids: &[String],
        cutoff: DateTime<Utc>,
    ) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM todos WHERE source_type = $1 AND source_id = ANY($2) AND created_at < $3 AND is_deleted = FALSE",
        )
        .bind(source_type)
        .bind(source_ids)
        .bind(cutoff)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(count.0)
    }
}

#[async_trait]
impl<'tx> TodoTransactionalRepository<Transaction<'tx, Postgres>> for PgTodoRepository {
    async fn save_in_tx(&self, tx: &mut Transaction<'tx, Postgres>, todo: &Todo) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"INSERT INTO todos (
                todo_id, title, description, priority, status, category,
                due_date, assigned_to, tags, estimated_duration, actual_duration,
                progress, is_recurring, recurring_pattern,
                parent_todo_id, execution_order, depends_on,
                source_type, source_id, is_deleted, deleted_at,
                created_at, updated_at, created_by, updated_by, version
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11,
                $12, $13, $14,
                $15, $16, $17,
                $18, $19, $20, $21,
                $22, $23, $24, $25, $26
            )
            ON CONFLICT (todo_id) DO UPDATE SET
                title = EXCLUDED.title, description = EXCLUDED.description,
                priority = EXCLUDED.priority, status = EXCLUDED.status,
                category = EXCLUDED.category, due_date = EXCLUDED.due_date,
                assigned_to = EXCLUDED.assigned_to, tags = EXCLUDED.tags,
                estimated_duration = EXCLUDED.estimated_duration,
                actual_duration = EXCLUDED.actual_duration,
                progress = EXCLUDED.progress,
                is_recurring = EXCLUDED.is_recurring,
                recurring_pattern = EXCLUDED.recurring_pattern,
                parent_todo_id = EXCLUDED.parent_todo_id,
                execution_order = EXCLUDED.execution_order,
                depends_on = EXCLUDED.depends_on,
                source_type = EXCLUDED.source_type,
                source_id = EXCLUDED.source_id,
                is_deleted = EXCLUDED.is_deleted,
                deleted_at = EXCLUDED.deleted_at,
                updated_at = EXCLUDED.updated_at,
                updated_by = EXCLUDED.updated_by,
                version = EXCLUDED.version
            WHERE todos.version = EXCLUDED.version - 1"#,
        )
        .bind(&todo.todo_id)
        .bind(&todo.title)
        .bind(&todo.description)
        .bind(match todo.priority {
            TodoPriority::Critical => "紧急",
            TodoPriority::High => "高",
            TodoPriority::Medium => "中",
            TodoPriority::Low => "低",
            TodoPriority::Background => "低",
        })
        .bind(match todo.status {
            TodoStatus::Pending => "待办",
            TodoStatus::InProgress => "进行中",
            TodoStatus::Completed => "已完成",
            TodoStatus::Cancelled => "已取消",
            TodoStatus::Blocked => "待办",
        })
        .bind(todo.category.map(|c| c.as_ref().to_string()))
        .bind(todo.due_date)
        .bind(&todo.assigned_to)
        .bind(&todo.tags)
        .bind(todo.estimated_duration)
        .bind(todo.actual_duration)
        .bind(todo.progress)
        .bind(todo.is_recurring)
        .bind(&todo.recurring_pattern)
        .bind(&todo.parent_todo_id)
        .bind(todo.execution_order)
        .bind(&todo.depends_on)
        .bind(&todo.source_type)
        .bind(&todo.source_id)
        .bind(todo.is_deleted)
        .bind(todo.deleted_at)
        .bind(todo.created_at)
        .bind(todo.updated_at)
        .bind(&todo.created_by)
        .bind(&todo.updated_by)
        .bind(todo.version)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()));

        match result {
            Ok(res) if res.rows_affected() == 0 => {
                return Err(DomainError::ConcurrencyConflict(
                    "Todo was modified concurrently".to_string(),
                ));
            }
            Err(e) => return Err(DomainError::Internal(e.to_string())),
            _ => {}
        }

        Ok(())
    }
    async fn update_in_tx(&self, tx: &mut Transaction<'tx, Postgres>, todo: &Todo) -> Result<bool, DomainError> {
        self.save_in_tx(tx, todo).await?;
        Ok(true)
    }

    async fn soft_delete_by_source_ids(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        source_type: &str,
        source_ids: &[String],
        cutoff: DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE todos
            SET is_deleted = TRUE,
                deleted_at = NOW(),
                updated_at = NOW()
            WHERE source_type = $1
              AND source_id = ANY($2)
              AND created_at < $3
              AND is_deleted = FALSE
            "#,
        )
        .bind(source_type)
        .bind(source_ids)
        .bind(cutoff)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected())
    }
}

fn parse_status(s: &str) -> TodoStatus {
    match s {
        "pending" | "待办" => TodoStatus::Pending,
        "in_progress" | "进行中" => TodoStatus::InProgress,
        "completed" | "已完成" => TodoStatus::Completed,
        "cancelled" | "已取消" => TodoStatus::Cancelled,
        "blocked" | "阻塞中" => TodoStatus::Blocked,
        _ => TodoStatus::Pending,
    }
}

fn parse_priority(s: &str) -> TodoPriority {
    match s {
        "critical" | "紧急" | "关键" => TodoPriority::Critical,
        "high" | "高" => TodoPriority::High,
        "medium" | "中" => TodoPriority::Medium,
        "low" | "低" => TodoPriority::Low,
        "background" | "后台" => TodoPriority::Background,
        _ => TodoPriority::Medium,
    }
}

fn parse_category(s: Option<String>) -> Option<TodoCategory> {
    s.as_deref().and_then(|s| match s {
        "work" | "工作" => Some(TodoCategory::Work),
        "personal" | "个人" => Some(TodoCategory::Personal),
        "meeting" | "会议" => Some(TodoCategory::Meeting),
        "deadline" | "截止日期" => Some(TodoCategory::Deadline),
        "recurring" | "重复任务" => Some(TodoCategory::Recurring),
        _ => None,
    })
}

fn normalize_category_value(value: &str) -> String {
    match value.trim() {
        "work" | "工作" => "work".to_string(),
        "personal" | "个人" => "personal".to_string(),
        "meeting" | "会议" => "meeting".to_string(),
        "deadline" | "截止日期" => "deadline".to_string(),
        "recurring" | "重复任务" => "recurring".to_string(),
        other => other.to_string(),
    }
}

fn row_to_todo(r: &sqlx::postgres::PgRow) -> Todo {
    Todo {
        todo_id: r.get("todo_id"),
        title: r.get("title"),
        description: r.get("description"),
        priority: parse_priority(r.get::<String, _>("priority").as_str()),
        status: parse_status(r.get::<String, _>("status").as_str()),
        category: parse_category(r.get("category")),
        due_date: r.get("due_date"),
        assigned_to: r.get("assigned_to"),
        tags: r.get::<Vec<String>, _>("tags"),
        estimated_duration: r.get("estimated_duration"),
        actual_duration: r.get("actual_duration"),
        progress: r.get("progress"),
        is_recurring: r.get("is_recurring"),
        recurring_pattern: r.get("recurring_pattern"),
        parent_todo_id: r.get("parent_todo_id"),
        execution_order: r.get("execution_order"),
        depends_on: r.get::<Vec<String>, _>("depends_on"),
        source_type: r.get("source_type"),
        source_id: r.get("source_id"),
        is_deleted: r.get("is_deleted"),
        deleted_at: r.get("deleted_at"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        created_by: r.get("created_by"),
        updated_by: r.get("updated_by"),
        version: r.get("version"),
    }
}
