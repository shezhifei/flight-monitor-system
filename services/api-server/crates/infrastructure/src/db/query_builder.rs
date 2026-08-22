//! 动态 SQL 查询构建器
//!
//! 对应 Python `src/infrastructure/database/query_builder.py`。
//! 使用 sqlx 的 `$N` 参数化占位符 (PostgreSQL 标准)。

use regex::Regex;
use std::fmt;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// 枚举
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

impl fmt::Display for JoinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Inner => "INNER JOIN",
            Self::Left => "LEFT JOIN",
            Self::Right => "RIGHT JOIN",
            Self::Full => "FULL OUTER JOIN",
            Self::Cross => "CROSS JOIN",
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OrderDirection {
    Asc,
    Desc,
}

impl fmt::Display for OrderDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Like,
    ILike,
    In,
    NotIn,
    IsNull,
    IsNotNull,
    Between,
}

impl CmpOp {
    fn sql_token(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Ne => "!=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Like => "LIKE",
            Self::ILike => "ILIKE",
            Self::In => "IN",
            Self::NotIn => "NOT IN",
            Self::IsNull => "IS NULL",
            Self::IsNotNull => "IS NOT NULL",
            Self::Between => "BETWEEN",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LogicalOp {
    And,
    Or,
}

impl fmt::Display for LogicalOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::And => "AND",
            Self::Or => "OR",
        })
    }
}

// ---------------------------------------------------------------------------
// SqlValue — 支持参数化绑定的值类型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

// ---------------------------------------------------------------------------
// 内部 条件结构
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum WhereClause {
    Field {
        field: String,
        op: CmpOp,
        values: Vec<SqlValue>,
        logical_op: LogicalOp,
    },
    Raw {
        sql: String,
        params: Vec<SqlValue>,
        logical_op: LogicalOp,
    },
}

// ---------------------------------------------------------------------------
// 标识符校验
// ---------------------------------------------------------------------------

static IDENT_RE: OnceLock<Regex> = OnceLock::new();

fn ident_re() -> &'static Regex {
    IDENT_RE.get_or_init(|| {
        Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)*$").expect("IDENT_RE: invalid regex constant")
    })
}

const DANGEROUS_TOKENS: &[&str] = &[";", "--", "/*", "*/", "xp_", "sp_"];

fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let lower = s.to_lowercase();
    if DANGEROUS_TOKENS.iter().any(|t| lower.contains(t)) {
        return false;
    }
    ident_re().is_match(s)
}

fn assert_ident(s: &str) -> Result<(), String> {
    if is_valid_identifier(s) {
        Ok(())
    } else {
        Err(format!("Invalid SQL identifier: {s}"))
    }
}

fn assert_select_field(s: &str) -> Result<(), String> {
    if s == "*" || is_valid_identifier(s) {
        Ok(())
    } else {
        Err(format!("Invalid SELECT field: {s}"))
    }
}

fn assert_safe_raw_select_expression(expression: &str) -> Result<(), String> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return Err("Raw SELECT expression cannot be empty".into());
    }

    let lowered = trimmed.to_lowercase();
    if DANGEROUS_TOKENS.iter().any(|token| lowered.contains(token)) {
        return Err("Unsafe raw SELECT expression".into());
    }

    static STRUCTURAL_SQL_RE: OnceLock<Regex> = OnceLock::new();
    let structural_sql_re = STRUCTURAL_SQL_RE.get_or_init(|| {
        Regex::new(r"(?i)\b(from|where|join|union|select|insert|update|delete|drop|alter)\b")
            .expect("STRUCTURAL_SQL_RE: invalid regex constant")
    });
    if structural_sql_re.is_match(trimmed) {
        return Err("Unsafe raw SELECT expression".into());
    }

    static SAFE_SELECT_EXPR_RE: OnceLock<Regex> = OnceLock::new();
    let safe_select_expr_re = SAFE_SELECT_EXPR_RE.get_or_init(|| {
        Regex::new(r"(?i)^[a-zA-Z0-9_.,\s()*+\-/]+(\s+AS\s+[a-zA-Z_][a-zA-Z0-9_]*)?$")
            .expect("SAFE_SELECT_EXPR_RE: invalid regex constant")
    });
    if safe_select_expr_re.is_match(trimmed) {
        Ok(())
    } else {
        Err("Unsafe raw SELECT expression".into())
    }
}

// ---------------------------------------------------------------------------
// QueryBuilder (SELECT)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct QueryBuilder {
    select_fields: Vec<String>,
    from_table: Option<String>,
    joins: Vec<(JoinType, String, String)>,
    where_clauses: Vec<WhereClause>,
    group_by: Vec<String>,
    having_clauses: Vec<WhereClause>,
    order_by: Vec<(String, OrderDirection)>,
    limit: Option<i64>,
    offset: Option<i64>,
    distinct: bool,
}

impl QueryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn select(mut self, fields: &[&str]) -> Result<Self, String> {
        for field in fields {
            assert_select_field(field)?;
        }
        self.select_fields.extend(fields.iter().map(|s| s.to_string()));
        Ok(self)
    }

    pub fn select_raw(mut self, expression: &str) -> Result<Self, String> {
        assert_safe_raw_select_expression(expression)?;
        self.select_fields.push(expression.trim().to_string());
        Ok(self)
    }

    pub fn select_all(mut self) -> Self {
        self.select_fields = vec!["*".to_string()];
        self
    }

    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    pub fn from(mut self, table: &str) -> Result<Self, String> {
        assert_ident(table)?;
        self.from_table = Some(table.to_string());
        Ok(self)
    }

    pub fn join(mut self, join_type: JoinType, table: &str, on: &str) -> Result<Self, String> {
        assert_ident(table)?;
        if DANGEROUS_TOKENS.iter().any(|t| on.contains(t)) {
            return Err(format!("Dangerous character in JOIN condition: {on}"));
        }
        self.joins.push((join_type, table.to_string(), on.to_string()));
        Ok(self)
    }

    pub fn where_field(mut self, field: &str, op: CmpOp, values: Vec<SqlValue>) -> Result<Self, String> {
        assert_ident(field)?;
        self.where_clauses.push(WhereClause::Field {
            field: field.to_string(),
            op,
            values,
            logical_op: LogicalOp::And,
        });
        Ok(self)
    }

    pub fn or_where_field(mut self, field: &str, op: CmpOp, values: Vec<SqlValue>) -> Result<Self, String> {
        assert_ident(field)?;
        self.where_clauses.push(WhereClause::Field {
            field: field.to_string(),
            op,
            values,
            logical_op: LogicalOp::Or,
        });
        Ok(self)
    }

    pub fn where_eq(self, field: &str, value: SqlValue) -> Result<Self, String> {
        self.where_field(field, CmpOp::Eq, vec![value])
    }

    pub fn where_in(self, field: &str, values: Vec<SqlValue>) -> Result<Self, String> {
        if values.is_empty() {
            // 空列表 → 不可能条件
            return self.where_field(field, CmpOp::Eq, vec![SqlValue::Null]);
        }
        self.where_field(field, CmpOp::In, values)
    }

    pub fn where_null(self, field: &str) -> Result<Self, String> {
        self.where_field(field, CmpOp::IsNull, vec![])
    }

    pub fn where_not_null(self, field: &str) -> Result<Self, String> {
        self.where_field(field, CmpOp::IsNotNull, vec![])
    }

    pub fn where_between(self, field: &str, start: SqlValue, end: SqlValue) -> Result<Self, String> {
        self.where_field(field, CmpOp::Between, vec![start, end])
    }

    pub fn where_raw(mut self, sql: &str, params: Vec<SqlValue>) -> Result<Self, String> {
        if sql.trim().is_empty() {
            return Err("Raw WHERE condition cannot be empty".into());
        }
        let lowered = sql.to_lowercase();
        if DANGEROUS_TOKENS.iter().any(|t| lowered.contains(t)) {
            return Err("Unsafe raw WHERE condition".into());
        }
        self.where_clauses.push(WhereClause::Raw {
            sql: sql.trim().to_string(),
            params,
            logical_op: LogicalOp::And,
        });
        Ok(self)
    }

    pub fn group_by(mut self, fields: &[&str]) -> Result<Self, String> {
        for f in fields {
            assert_ident(f)?;
        }
        self.group_by.extend(fields.iter().map(|s| s.to_string()));
        Ok(self)
    }

    pub fn order_by(mut self, field: &str, dir: OrderDirection) -> Result<Self, String> {
        assert_ident(field)?;
        self.order_by.push((field.to_string(), dir));
        Ok(self)
    }

    pub fn limit(mut self, n: i64) -> Result<Self, String> {
        if n <= 0 {
            return Err("Limit must be positive".into());
        }
        self.limit = Some(n);
        Ok(self)
    }

    pub fn offset(mut self, n: i64) -> Result<Self, String> {
        if n < 0 {
            return Err("Offset cannot be negative".into());
        }
        self.offset = Some(n);
        Ok(self)
    }

    /// 构建 SQL 和参数列表
    pub fn build(self) -> Result<(String, Vec<SqlValue>), String> {
        let table = self.from_table.as_deref().ok_or("FROM table is required")?;
        let mut sql = String::new();
        let mut params: Vec<SqlValue> = Vec::new();
        let mut param_idx: usize = 0;

        // SELECT
        let distinct = if self.distinct { "DISTINCT " } else { "" };
        let fields = if self.select_fields.is_empty() {
            "*".to_string()
        } else {
            self.select_fields.join(", ")
        };
        sql.push_str(&format!("SELECT {distinct}{fields} FROM {table}"));

        // JOINs
        for (jt, tbl, on) in &self.joins {
            sql.push_str(&format!(" {jt} {tbl} ON {on}"));
        }

        // WHERE
        let (where_sql, where_params, next_idx) = build_conditions(&self.where_clauses, param_idx);
        if !where_sql.is_empty() {
            sql.push_str(&format!(" WHERE {where_sql}"));
            params.extend(where_params);
            param_idx = next_idx;
        }

        // GROUP BY
        if !self.group_by.is_empty() {
            sql.push_str(&format!(" GROUP BY {}", self.group_by.join(", ")));
        }

        // HAVING
        let (having_sql, having_params, next_idx) = build_conditions(&self.having_clauses, param_idx);
        if !having_sql.is_empty() {
            sql.push_str(&format!(" HAVING {having_sql}"));
            params.extend(having_params);
            param_idx = next_idx;
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            let clauses: Vec<String> = self.order_by.iter().map(|(f, d)| format!("{f} {d}")).collect();
            sql.push_str(&format!(" ORDER BY {}", clauses.join(", ")));
        }

        // LIMIT / OFFSET
        if let Some(l) = self.limit {
            param_idx += 1;
            sql.push_str(&format!(" LIMIT ${param_idx}"));
            params.push(SqlValue::Int(l));
        }
        if let Some(o) = self.offset {
            param_idx += 1;
            sql.push_str(&format!(" OFFSET ${param_idx}"));
            params.push(SqlValue::Int(o));
        }

        let _ = param_idx; // suppress unused
        Ok((sql, params))
    }
}

// ---------------------------------------------------------------------------
// InsertBuilder
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct InsertBuilder {
    table: Option<String>,
    columns: Vec<String>,
    rows: Vec<Vec<SqlValue>>,
    on_conflict: Option<ConflictAction>,
}

#[derive(Debug, Clone)]
pub enum ConflictAction {
    DoNothing,
    DoUpdate,
}

impl InsertBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_table(mut self, table: &str) -> Result<Self, String> {
        assert_ident(table)?;
        self.table = Some(table.to_string());
        Ok(self)
    }

    pub fn columns(mut self, cols: &[&str]) -> Result<Self, String> {
        for c in cols {
            assert_ident(c)?;
        }
        self.columns.extend(cols.iter().map(|s| s.to_string()));
        Ok(self)
    }

    pub fn values(mut self, vals: Vec<SqlValue>) -> Result<Self, String> {
        if vals.len() != self.columns.len() {
            return Err("Values count must match columns count".into());
        }
        self.rows.push(vals);
        Ok(self)
    }

    pub fn on_conflict_ignore(mut self) -> Self {
        self.on_conflict = Some(ConflictAction::DoNothing);
        self
    }

    pub fn on_conflict_update(mut self) -> Self {
        self.on_conflict = Some(ConflictAction::DoUpdate);
        self
    }

    pub fn build(self) -> Result<(String, Vec<SqlValue>), String> {
        let table = self.table.as_deref().ok_or("INTO table is required")?;
        if self.columns.is_empty() {
            return Err("Columns are required".into());
        }
        if self.rows.is_empty() {
            return Err("Values are required".into());
        }

        let cols_str = self.columns.join(", ");
        let mut params: Vec<SqlValue> = Vec::new();
        let mut param_idx: usize = 0;

        let row_placeholders: Vec<String> = self
            .rows
            .iter()
            .map(|row| {
                let ph: Vec<String> = row
                    .iter()
                    .map(|_| {
                        param_idx += 1;
                        format!("${param_idx}")
                    })
                    .collect();
                format!("({})", ph.join(", "))
            })
            .collect();

        for row in &self.rows {
            params.extend(row.iter().cloned());
        }

        let values_str = row_placeholders.join(", ");
        let mut sql = format!("INSERT INTO {table} ({cols_str}) VALUES {values_str}");

        match self.on_conflict {
            Some(ConflictAction::DoNothing) => {
                let pk = self.columns.first().map(|s| s.as_str()).unwrap_or("id");
                sql.push_str(&format!(" ON CONFLICT ({pk}) DO NOTHING"));
            }
            Some(ConflictAction::DoUpdate) => {
                let pk = self.columns.first().map(|s| s.as_str()).unwrap_or("id");
                let updates: Vec<String> = self
                    .columns
                    .iter()
                    .filter(|c| c.as_str() != pk)
                    .map(|c| format!("{c} = EXCLUDED.{c}"))
                    .collect();
                let update_clause = if updates.is_empty() {
                    format!("{pk} = EXCLUDED.{pk}")
                } else {
                    updates.join(", ")
                };
                sql.push_str(&format!(" ON CONFLICT ({pk}) DO UPDATE SET {update_clause}"));
            }
            None => {}
        }

        Ok((sql, params))
    }
}

// ---------------------------------------------------------------------------
// UpdateBuilder
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct UpdateBuilder {
    table: Option<String>,
    set_clauses: Vec<(String, SqlValue)>,
    where_clauses: Vec<WhereClause>,
}

impl UpdateBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn table(mut self, table: &str) -> Result<Self, String> {
        assert_ident(table)?;
        self.table = Some(table.to_string());
        Ok(self)
    }

    pub fn set(mut self, field: &str, value: SqlValue) -> Result<Self, String> {
        assert_ident(field)?;
        self.set_clauses.push((field.to_string(), value));
        Ok(self)
    }

    pub fn where_field(mut self, field: &str, op: CmpOp, values: Vec<SqlValue>) -> Result<Self, String> {
        assert_ident(field)?;
        self.where_clauses.push(WhereClause::Field {
            field: field.to_string(),
            op,
            values,
            logical_op: LogicalOp::And,
        });
        Ok(self)
    }

    pub fn where_eq(self, field: &str, value: SqlValue) -> Result<Self, String> {
        self.where_field(field, CmpOp::Eq, vec![value])
    }

    pub fn build(self) -> Result<(String, Vec<SqlValue>), String> {
        let table = self.table.as_deref().ok_or("Table is required")?;
        if self.set_clauses.is_empty() {
            return Err("SET clauses are required".into());
        }

        let mut params: Vec<SqlValue> = Vec::new();
        let mut param_idx: usize = 0;

        // SET
        let set_parts: Vec<String> = self
            .set_clauses
            .iter()
            .map(|(field, _val)| {
                param_idx += 1;
                format!("{field} = ${param_idx}")
            })
            .collect();
        for (_field, val) in &self.set_clauses {
            params.push(val.clone());
        }

        let mut sql = format!("UPDATE {table} SET {}", set_parts.join(", "));

        // WHERE
        let (where_sql, where_params, _) = build_conditions(&self.where_clauses, param_idx);
        if !where_sql.is_empty() {
            sql.push_str(&format!(" WHERE {where_sql}"));
            params.extend(where_params);
        }

        Ok((sql, params))
    }
}

// ---------------------------------------------------------------------------
// DeleteBuilder
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct DeleteBuilder {
    table: Option<String>,
    where_clauses: Vec<WhereClause>,
}

impl DeleteBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_table(mut self, table: &str) -> Result<Self, String> {
        assert_ident(table)?;
        self.table = Some(table.to_string());
        Ok(self)
    }

    pub fn where_field(mut self, field: &str, op: CmpOp, values: Vec<SqlValue>) -> Result<Self, String> {
        assert_ident(field)?;
        self.where_clauses.push(WhereClause::Field {
            field: field.to_string(),
            op,
            values,
            logical_op: LogicalOp::And,
        });
        Ok(self)
    }

    pub fn where_eq(self, field: &str, value: SqlValue) -> Result<Self, String> {
        self.where_field(field, CmpOp::Eq, vec![value])
    }

    pub fn build(self) -> Result<(String, Vec<SqlValue>), String> {
        let table = self.table.as_deref().ok_or("FROM table is required")?;
        let mut params: Vec<SqlValue> = Vec::new();
        let mut sql = format!("DELETE FROM {table}");

        let (where_sql, where_params, _) = build_conditions(&self.where_clauses, 0);
        if !where_sql.is_empty() {
            sql.push_str(&format!(" WHERE {where_sql}"));
            params.extend(where_params);
        }

        Ok((sql, params))
    }
}

// ---------------------------------------------------------------------------
// 共用条件构建
// ---------------------------------------------------------------------------

fn build_conditions(clauses: &[WhereClause], mut param_idx: usize) -> (String, Vec<SqlValue>, usize) {
    if clauses.is_empty() {
        return (String::new(), Vec::new(), param_idx);
    }

    let mut sql_parts: Vec<String> = Vec::new();
    let mut params: Vec<SqlValue> = Vec::new();

    for (i, clause) in clauses.iter().enumerate() {
        let logical_op = match clause {
            WhereClause::Field { logical_op, .. } => logical_op,
            WhereClause::Raw { logical_op, .. } => logical_op,
        };

        if i > 0 {
            sql_parts.push(logical_op.to_string());
        }

        match clause {
            WhereClause::Field { field, op, values, .. } => match op {
                CmpOp::IsNull | CmpOp::IsNotNull => {
                    sql_parts.push(format!("({field} {})", op.sql_token()));
                }
                CmpOp::In | CmpOp::NotIn => {
                    let placeholders: Vec<String> = values
                        .iter()
                        .map(|_| {
                            param_idx += 1;
                            format!("${param_idx}")
                        })
                        .collect();
                    sql_parts.push(format!("({field} {} ({}))", op.sql_token(), placeholders.join(", ")));
                    params.extend(values.iter().cloned());
                }
                CmpOp::Between => {
                    if values.len() == 2 {
                        param_idx += 1;
                        let p1 = param_idx;
                        param_idx += 1;
                        let p2 = param_idx;
                        sql_parts.push(format!("({field} BETWEEN ${p1} AND ${p2})"));
                        params.extend(values.iter().cloned());
                    }
                }
                _ => {
                    param_idx += 1;
                    sql_parts.push(format!("({field} {} ${param_idx})", op.sql_token()));
                    if let Some(v) = values.first() {
                        params.push(v.clone());
                    }
                }
            },
            WhereClause::Raw {
                sql,
                params: raw_params,
                ..
            } => {
                // 替换 raw SQL 中的 $N 占位符，重新编号
                let mut replaced = sql.clone();
                for rp in raw_params {
                    param_idx += 1;
                    // 简单替换第一个出现的占位符标记
                    if let Some(pos) = replaced.find("$?") {
                        replaced.replace_range(pos..pos + 2, &format!("${param_idx}"));
                    }
                    params.push(rp.clone());
                }
                sql_parts.push(format!("({replaced})"));
            }
        }
    }

    (sql_parts.join(" "), params, param_idx)
}

#[cfg(test)]
mod tests {
    use super::QueryBuilder;

    #[test]
    fn select_accepts_identifiers_and_star() {
        let (sql, params) = QueryBuilder::new()
            .select(&["id", "flights.flight_no"])
            .expect("valid select fields")
            .from("flights")
            .expect("valid table")
            .build()
            .expect("query builds");

        assert_eq!(sql, "SELECT id, flights.flight_no FROM flights");
        assert!(params.is_empty());

        let (sql, _) = QueryBuilder::new()
            .select(&["*"])
            .expect("star is valid")
            .from("flights")
            .expect("valid table")
            .build()
            .expect("query builds");
        assert_eq!(sql, "SELECT * FROM flights");
    }

    #[test]
    fn select_rejects_unsafe_field_by_default() {
        let result = QueryBuilder::new().select(&["id; DROP TABLE flights"]);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid SELECT field"));
    }

    #[test]
    fn select_raw_is_explicit_and_rejects_dangerous_expressions() {
        let (sql, params) = QueryBuilder::new()
            .select(&["id"])
            .expect("valid select field")
            .select_raw("COUNT(*) AS total")
            .expect("safe expression")
            .from("flights")
            .expect("valid table")
            .build()
            .expect("query builds");

        assert_eq!(sql, "SELECT id, COUNT(*) AS total FROM flights");
        assert!(params.is_empty());

        let result = QueryBuilder::new().select_raw("COUNT(*); DROP TABLE flights");
        assert!(result.is_err());

        let result = QueryBuilder::new().select_raw("id FROM flights");
        assert!(result.is_err());
    }

    #[test]
    fn where_eq_binds_user_values_instead_of_interpolating() {
        use super::SqlValue;

        let injected = "abc'; DROP TABLE business_cases; --";
        let (sql, params) = QueryBuilder::new()
            .select(&["id"])
            .expect("valid select")
            .from("business_cases")
            .expect("valid table")
            .where_eq("id", SqlValue::Text(injected.to_string()))
            .expect("where")
            .build()
            .expect("query builds");

        assert!(sql.contains("id = $1"), "expected bind placeholder, got {sql}");
        assert!(!sql.contains(injected), "user value leaked into SQL: {sql}");
        assert_eq!(params, vec![SqlValue::Text(injected.to_string())]);
    }
}
