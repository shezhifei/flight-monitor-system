use std::env;

use async_trait::async_trait;
use serde_json::{Map, Value};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use fms_domain::error::DomainError;
use fms_domain::ports::system_flags_repository::SystemFlagsRepository;

const DEFAULT_SYSTEM_CONFIG_TABLE: &str = "system_config";

pub struct PgSystemFlagsRepository {
    pool: PgPool,
    table_name: String,
}

impl PgSystemFlagsRepository {
    pub fn new(pool: PgPool) -> Self {
        let table_name = env::var("SYSTEM_CONFIG_TABLE")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_SYSTEM_CONFIG_TABLE.to_string());

        Self { pool, table_name }
    }

    async fn load_raw(&self) -> Result<Map<String, Value>, sqlx::Error> {
        validate_sql_identifier(&self.table_name)?;
        let statement = format!("SELECT key, value FROM {}", self.table_name);
        let rows = sqlx::query(&statement).fetch_all(&self.pool).await?;

        let mut config = Map::new();
        for row in rows {
            let key: String = row.try_get("key")?;
            let value: Value = row.try_get("value")?;
            insert_source_entry(&mut config, &key, value);
        }

        Ok(config)
    }

    async fn replace_all_raw(&self, snapshot: &Map<String, Value>) -> Result<(), sqlx::Error> {
        validate_sql_identifier(&self.table_name)?;

        let mut tx = self.pool.begin().await?;

        let delete_statement = format!("DELETE FROM {}", self.table_name);
        sqlx::query(&delete_statement).execute(&mut *tx).await?;

        if !snapshot.is_empty() {
            let mut builder = QueryBuilder::<Postgres>::new(format!("INSERT INTO {} (key, value) ", self.table_name));
            builder.push_values(snapshot.iter(), |mut b, (key, value)| {
                b.push_bind(key).push_bind(value.clone());
            });
            builder.build().execute(&mut *tx).await?;
        }

        tx.commit().await
    }
}

#[async_trait]
impl SystemFlagsRepository for PgSystemFlagsRepository {
    async fn load(&self) -> Result<Map<String, Value>, DomainError> {
        self.load_raw()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))
    }

    async fn replace_all(&self, snapshot: &Map<String, Value>) -> Result<(), DomainError> {
        self.replace_all_raw(snapshot)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))
    }
}

fn insert_source_entry(root: &mut Map<String, Value>, key: &str, value: Value) {
    if key.contains('.') {
        insert_path_value(root, key, value);
        return;
    }

    match (root.get_mut(key), value) {
        (Some(Value::Object(existing)), Value::Object(override_object)) => {
            deep_merge_objects(existing, override_object);
        }
        (_, value) => {
            root.insert(key.to_string(), value);
        }
    }
}

fn insert_path_value(root: &mut Map<String, Value>, path: &str, value: Value) {
    let parts: Vec<&str> = path.split('.').filter(|item| !item.is_empty()).collect();
    if parts.is_empty() {
        return;
    }

    insert_path_parts(root, &parts, value);
}

fn insert_path_parts(root: &mut Map<String, Value>, parts: &[&str], value: Value) {
    if parts.len() == 1 {
        root.insert(parts[0].to_string(), value);
        return;
    }

    let entry = root
        .entry(parts[0].to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }

    if let Some(child) = entry.as_object_mut() {
        insert_path_parts(child, &parts[1..], value);
    }
}

fn deep_merge_objects(base: &mut Map<String, Value>, override_values: Map<String, Value>) {
    for (key, override_value) in override_values {
        match (base.get_mut(&key), override_value) {
            (Some(Value::Object(base_object)), Value::Object(override_object)) => {
                deep_merge_objects(base_object, override_object);
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn validate_sql_identifier(identifier: &str) -> Result<(), sqlx::Error> {
    let mut chars = identifier.chars();
    let Some(first) = chars.next() else {
        return Err(sqlx::Error::Protocol("system config table name is required".into()));
    };

    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(sqlx::Error::Protocol(format!(
            "invalid system config table name: {identifier}"
        )));
    }

    Ok(())
}
