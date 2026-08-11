//! PostgreSQL 类型映射
//!
//! 对应 Python `src/infrastructure/database/type_mappings.py`。
//! 定义 Rust 类型到 PostgreSQL 列定义的标准映射。

use std::collections::HashMap;

/// Rust 类型到 PG 类型映射
pub fn rust_to_pg_types() -> HashMap<&'static str, &'static str> {
    [
        // 基础类型
        ("String", "VARCHAR"),
        ("i32", "INTEGER"),
        ("i64", "BIGINT"),
        ("f64", "DECIMAL"),
        ("bool", "BOOLEAN"),
        ("DateTime<Utc>", "TIMESTAMP WITH TIME ZONE"),
        ("NaiveDate", "DATE"),
        // 特殊类型
        ("Ulid", "VARCHAR(36)"),
        ("Vec<String>", "TEXT[]"),
        ("HashMap", "JSONB"),
        ("serde_json::Value", "JSONB"),
        // 值对象
        ("FlightId", "VARCHAR(36)"),
        ("FlightNumber", "VARCHAR(7)"),
        ("AirportCode", "VARCHAR(3)"),
        ("AircraftType", "VARCHAR(10)"),
        ("UserId", "VARCHAR(36)"),
    ]
    .into_iter()
    .collect()
}

/// 字段级别的 PG 列定义
pub fn field_type_specs() -> HashMap<&'static str, &'static str> {
    [
        // 通用
        ("id", "SERIAL PRIMARY KEY"),
        ("uuid", "VARCHAR(36) NOT NULL UNIQUE"),
        ("created_at", "TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP"),
        ("updated_at", "TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP"),
        ("version", "INTEGER NOT NULL DEFAULT 1"),
        // 航班
        ("flight_id", "VARCHAR(36) NOT NULL UNIQUE"),
        ("flight_number", "VARCHAR(7) NOT NULL"),
        ("airline_code", "VARCHAR(3) NOT NULL"),
        ("airport_code", "VARCHAR(3)"),
        ("aircraft_type_detail", "VARCHAR(10)"),
        ("execution_date", "DATE"),
        ("workspace_date", "DATE"),
        ("gate", "VARCHAR(10)"),
        ("stand", "VARCHAR(10)"),
        ("terminal", "VARCHAR(10)"),
        ("position", "VARCHAR(50)"),
        // 时间
        ("scheduled_time", "TIMESTAMP WITH TIME ZONE"),
        ("estimated_time", "TIMESTAMP WITH TIME ZONE"),
        ("actual_time", "TIMESTAMP WITH TIME ZONE"),
        // 状态
        ("status", "SMALLINT NOT NULL DEFAULT 0"),
        ("leg_type", "VARCHAR(16) NOT NULL"),
        ("flight_type", "VARCHAR(16) NOT NULL"),
        ("mission", "SMALLINT"),
        ("origin_stations", "JSONB NOT NULL DEFAULT '[]'::jsonb"),
        ("destination_stations", "JSONB NOT NULL DEFAULT '[]'::jsonb"),
        ("stand_type", "VARCHAR(32)"),
        // 标记
        ("is_vip", "BOOLEAN DEFAULT FALSE"),
        ("has_boarding_restriction", "BOOLEAN DEFAULT FALSE"),
        ("is_quick_turnaround", "BOOLEAN DEFAULT FALSE"),
        // 用户
        ("username", "VARCHAR(50) NOT NULL UNIQUE"),
        ("email", "VARCHAR(255) NOT NULL UNIQUE"),
        ("password_hash", "VARCHAR(255) NOT NULL"),
        ("is_active", "BOOLEAN DEFAULT TRUE"),
        ("is_verified", "BOOLEAN DEFAULT FALSE"),
        ("is_admin", "BOOLEAN DEFAULT FALSE"),
        ("roles", "TEXT[] DEFAULT '{}'"),
        ("permissions", "TEXT[] DEFAULT '{}'"),
        ("last_login_at", "TIMESTAMP WITH TIME ZONE"),
        // 事件
        ("event_id", "VARCHAR(36) NOT NULL UNIQUE"),
        ("event_type", "VARCHAR(50) NOT NULL"),
        ("aggregate_id", "VARCHAR(36) NOT NULL"),
        ("aggregate_type", "VARCHAR(50) NOT NULL"),
        ("event_data", "JSONB NOT NULL"),
        ("event_metadata", "JSONB DEFAULT '{}'"),
        ("sequence_number", "BIGINT NOT NULL"),
        ("occurred_at", "TIMESTAMP WITH TIME ZONE NOT NULL"),
    ]
    .into_iter()
    .collect()
}

/// 获取字段的 PG 列定义
pub fn get_column_definition(field_name: &str, rust_type: &str) -> String {
    // 优先匹配字段级规格
    if let Some(spec) = field_type_specs().get(field_name) {
        return spec.to_string();
    }
    // 回退到类型映射
    if let Some(pg_type) = rust_to_pg_types().get(rust_type) {
        return pg_type.to_string();
    }
    "VARCHAR".to_string()
}

/// 获取表的索引定义
pub fn get_table_indexes(table_name: &str) -> HashMap<&'static str, &'static str> {
    match table_name {
        "flights" => [
            (
                "idx_flights_flight_id",
                "CREATE INDEX idx_flights_flight_id ON flights(flight_id)",
            ),
            (
                "idx_flights_airline_code",
                "CREATE INDEX idx_flights_airline_code ON flights(airline_code)",
            ),
            (
                "idx_flights_status",
                "CREATE INDEX idx_flights_status ON flights(status)",
            ),
            (
                "idx_flights_scheduled_departure",
                "CREATE INDEX idx_flights_scheduled_departure ON flights(scheduled_departure)",
            ),
            (
                "idx_flights_created_at",
                "CREATE INDEX idx_flights_created_at ON flights(created_at)",
            ),
            (
                "idx_flights_status_date",
                "CREATE INDEX idx_flights_status_date ON flights(status, scheduled_departure)",
            ),
        ]
        .into_iter()
        .collect(),
        "users" => [
            ("idx_users_email", "CREATE INDEX idx_users_email ON users(email)"),
            (
                "idx_users_username",
                "CREATE INDEX idx_users_username ON users(username)",
            ),
            (
                "idx_users_is_active",
                "CREATE INDEX idx_users_is_active ON users(is_active)",
            ),
        ]
        .into_iter()
        .collect(),
        _ => HashMap::new(),
    }
}
