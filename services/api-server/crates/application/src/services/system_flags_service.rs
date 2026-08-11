use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use chrono::Utc;
use regex::Regex;
use serde_json::{json, Map, Number, Value};
use tracing::{error, info, warn};

use fms_domain::error::DomainError;
use fms_domain::ports::system_flags_repository::SystemFlagsRepository;

const MASKED_PLACEHOLDER: &str = "***REDACTED***";
const DEFAULT_SYSTEM_CONFIG_TABLE: &str = "system_config";

pub struct SystemFlagsService {
    repo: Arc<dyn SystemFlagsRepository + Send + Sync>,
}

impl SystemFlagsService {
    pub fn new(repo: Arc<dyn SystemFlagsRepository + Send + Sync>) -> Self {
        Self { repo }
    }

    pub async fn get_flags(&self) -> Vec<Value> {
        let manager = self.load_config_manager().await;
        let mut flags = Vec::new();
        flatten_config(&manager.snapshot, "", &mut flags, true);
        flags.sort_by(|left, right| {
            left.get("path")
                .and_then(Value::as_str)
                .cmp(&right.get("path").and_then(Value::as_str))
        });
        flags
    }

    pub async fn update_flag(&self, path: &str, value: Value) -> Result<Value, DomainError> {
        let normalized_path = path.trim();
        if normalized_path.is_empty() {
            return Err(DomainError::ValidationError("path is required".into()));
        }

        let mut manager = self.load_config_manager().await;
        let writable_sources = manager
            .writable_sources(is_distributed_mode())
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        if is_distributed_mode() && writable_sources.is_empty() {
            return Err(DomainError::Internal(
                "Distributed mode requires a centralized writable config source".into(),
            ));
        }

        manager.set(normalized_path, value);
        let snapshot = manager.snapshot.clone();

        let mut persisted_sources = Vec::new();
        for source in writable_sources {
            match source.save(&self.repo, &snapshot).await {
                Ok(()) => {
                    info!(source = %source.name, "persisted system config snapshot");
                    persisted_sources.push(source.name.clone());
                }
                Err(error) => {
                    error!(source = %source.name, error = %error, "failed to persist system config snapshot");
                }
            }
        }

        if is_distributed_mode() && persisted_sources.is_empty() {
            return Err(DomainError::Internal(
                "Distributed mode requires a centralized writable config source".into(),
            ));
        }

        let new_value = manager.get(normalized_path).unwrap_or(&Value::Null);
        let masked = is_sensitive_path(normalized_path);

        Ok(json!({
            "path": normalized_path,
            "value": if masked {
                mask_sensitive_value(normalized_path, new_value)
            } else {
                new_value.clone()
            },
            "masked": masked,
            "success": true,
        }))
    }

    pub async fn export_config(&self) -> Value {
        let manager = self.load_config_manager().await;
        json!({
            "exported_at": Utc::now().to_rfc3339(),
            "config": sanitize_config_value("", Value::Object(manager.snapshot)),
        })
    }

    pub async fn get_airport_context(&self) -> Value {
        let manager = self.load_config_manager().await;
        let code = config_string(&manager.snapshot, "site.airport.code")
            .or_else(|| env_first(&["SITE_AIRPORT_CODE", "AIRPORT_CODE"]))
            .unwrap_or_default()
            .to_ascii_uppercase();
        let display_name = config_string(&manager.snapshot, "site.airport.display_name")
            .or_else(|| env_first(&["SITE_AIRPORT_DISPLAY_NAME", "AIRPORT_DISPLAY_NAME"]))
            .unwrap_or_else(|| "本站".to_string());
        let mut aliases = config_string_list(&manager.snapshot, "site.airport.name_aliases")
            .or_else(|| {
                env_first(&["SITE_AIRPORT_NAME_ALIASES", "AIRPORT_NAME_ALIASES"]).map(|value| parse_aliases(&value))
            })
            .unwrap_or_default();

        if !display_name.is_empty() && !aliases.iter().any(|item| item == &display_name) {
            aliases.insert(0, display_name.clone());
        }

        json!({
            "code": code,
            "display_name": display_name,
            "name_aliases": aliases,
        })
    }

    async fn load_config_manager(&self) -> LoadedConfigManager {
        let mut merged = Map::new();
        let sources = configured_sources();

        for source in &sources {
            match source.load(&self.repo).await {
                Ok(config) => deep_merge_objects(&mut merged, config),
                Err(error) => {
                    warn!(source = %source.name, error = %error, "failed to load config source");
                }
            }
        }

        LoadedConfigManager {
            snapshot: apply_env_substitution_to_tree(merged),
            sources,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ConfigSourcePriority {
    File = 2,
    Remote = 3,
    Environment = 4,
}

#[derive(Clone, Debug)]
struct LoadedConfigManager {
    snapshot: Map<String, Value>,
    sources: Vec<ConfigSourceDefinition>,
}

impl LoadedConfigManager {
    fn get(&self, path: &str) -> Option<&Value> {
        config_value_at(&self.snapshot, path)
    }

    fn set(&mut self, path: &str, value: Value) {
        insert_path_value(&mut self.snapshot, path, value);
    }

    fn writable_sources(&self, distributed_mode: bool) -> Vec<&ConfigSourceDefinition> {
        self.sources
            .iter()
            .filter(|source| source.is_writable())
            .filter(|source| !distributed_mode || !source.is_local_file_source())
            .collect()
    }
}

#[derive(Clone, Debug)]
struct ConfigSourceDefinition {
    name: String,
    priority: ConfigSourcePriority,
    kind: ConfigSourceKind,
}

#[derive(Clone, Debug)]
enum ConfigSourceKind {
    File { path: PathBuf },
    Database { table_name: String },
    Environment,
}

impl ConfigSourceDefinition {
    fn file(relative_path: &str) -> Self {
        Self {
            name: format!("file://{relative_path}"),
            priority: ConfigSourcePriority::File,
            kind: ConfigSourceKind::File {
                path: workspace_root().join(relative_path),
            },
        }
    }

    fn database(table_name: String) -> Self {
        Self {
            name: format!("db://{table_name}"),
            priority: ConfigSourcePriority::Remote,
            kind: ConfigSourceKind::Database { table_name },
        }
    }

    fn environment() -> Self {
        Self {
            name: "env://".into(),
            priority: ConfigSourcePriority::Environment,
            kind: ConfigSourceKind::Environment,
        }
    }

    async fn load(
        &self,
        repo: &Arc<dyn SystemFlagsRepository + Send + Sync>,
    ) -> Result<Map<String, Value>, DomainError> {
        match &self.kind {
            ConfigSourceKind::File { path } => load_file_source(path),
            ConfigSourceKind::Database { table_name } => repo
                .load()
                .await
                .map_err(|error| DomainError::Internal(format!("failed to load {table_name}: {error}"))),
            ConfigSourceKind::Environment => Ok(load_env_source()),
        }
    }

    async fn save(
        &self,
        repo: &Arc<dyn SystemFlagsRepository + Send + Sync>,
        snapshot: &Map<String, Value>,
    ) -> Result<(), DomainError> {
        match &self.kind {
            ConfigSourceKind::File { path } => save_file_source(path, snapshot),
            ConfigSourceKind::Database { table_name } => repo
                .replace_all(snapshot)
                .await
                .map_err(|error| DomainError::Internal(format!("failed to persist {table_name}: {error}"))),
            ConfigSourceKind::Environment => Ok(()),
        }
    }

    fn is_local_file_source(&self) -> bool {
        matches!(self.kind, ConfigSourceKind::File { .. })
    }

    fn is_writable(&self) -> bool {
        !matches!(self.kind, ConfigSourceKind::Environment)
    }
}

fn configured_sources() -> Vec<ConfigSourceDefinition> {
    let mut sources = vec![ConfigSourceDefinition::file("config/app_config.yaml")];

    if local_runtime_overrides_enabled() {
        let runtime_override_path = workspace_root().join("config/runtime_overrides.yaml");
        ensure_runtime_override_file(&runtime_override_path);
        sources.push(ConfigSourceDefinition::file("config/runtime_overrides.yaml"));
    }

    if system_config_backend() == "postgres" {
        sources.push(ConfigSourceDefinition::database(system_config_table()));
    }

    sources.push(ConfigSourceDefinition::environment());
    sources.sort_by_key(|source| source.priority);
    sources
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn ensure_runtime_override_file(path: &Path) {
    if path.exists() {
        return;
    }

    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            warn!(path = %path.display(), error = %error, "failed to create runtime override directory");
            return;
        }
    }

    if let Err(error) = fs::write(path, "{}\n") {
        warn!(path = %path.display(), error = %error, "failed to initialize runtime override file");
    }
}

fn load_file_source(path: &Path) -> Result<Map<String, Value>, DomainError> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => {
            return Err(DomainError::Internal(format!(
                "failed to read config file {}: {error}",
                path.display()
            )));
        }
    };

    if raw.trim().is_empty() {
        return Ok(Map::new());
    }

    let value: Value = serde_yaml::from_str(&raw)
        .map_err(|error| DomainError::Internal(format!("failed to parse config file {}: {error}", path.display())))?;

    match value {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        other => Err(DomainError::Internal(format!(
            "config file {} must contain an object, got {other}",
            path.display()
        ))),
    }
}

fn save_file_source(path: &Path, snapshot: &Map<String, Value>) -> Result<(), DomainError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            DomainError::Internal(format!(
                "failed to create config directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    let serialized = serde_yaml::to_string(&Value::Object(snapshot.clone())).map_err(|error| {
        DomainError::Internal(format!("failed to serialize config file {}: {error}", path.display()))
    })?;

    fs::write(path, serialized)
        .map_err(|error| DomainError::Internal(format!("failed to write config file {}: {error}", path.display())))
}

fn load_env_source() -> Map<String, Value> {
    load_env_source_from_iter(env::vars())
}

fn load_env_source_from_iter<I>(vars: I) -> Map<String, Value>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut config = Map::new();

    for (key, value) in vars {
        let path = key.to_lowercase().replace('_', ".");
        insert_path_value(&mut config, &path, convert_env_value(&value));
    }

    config
}

fn convert_env_value(value: &str) -> Value {
    if value.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if value.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }

    if !value.contains('.') {
        if let Ok(parsed) = value.parse::<i64>() {
            return Value::Number(Number::from(parsed));
        }
        if let Ok(parsed) = value.parse::<u64>() {
            return Value::Number(Number::from(parsed));
        }
    }

    if let Ok(parsed) = value.parse::<f64>() {
        if let Some(number) = Number::from_f64(parsed) {
            return Value::Number(number);
        }
    }

    Value::String(value.to_string())
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

fn apply_env_substitution_to_tree(config: Map<String, Value>) -> Map<String, Value> {
    match substitute_env_placeholders(Value::Object(config)) {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn substitute_env_placeholders(value: Value) -> Value {
    match value {
        Value::String(text) => {
            let pattern = env_substitution_pattern();
            let replaced = pattern.replace_all(&text, |captures: &regex::Captures<'_>| {
                let env_key = captures.get(1).map(|value| value.as_str()).unwrap_or_default();
                let default_value = captures.get(2).map(|value| value.as_str());

                env::var(env_key)
                    .ok()
                    .or_else(|| default_value.map(str::to_string))
                    .unwrap_or_else(|| {
                        captures
                            .get(0)
                            .map(|value| value.as_str())
                            .unwrap_or_default()
                            .to_string()
                    })
            });
            Value::String(replaced.into_owned())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, substitute_env_placeholders(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(substitute_env_placeholders).collect()),
        other => other,
    }
}

fn env_substitution_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\$\{([^}:]+)(?::?-?([^}]*))?\}").expect("env substitution regex must compile"))
}

fn flatten_config(config: &Map<String, Value>, parent_key: &str, items: &mut Vec<Value>, mask_sensitive: bool) {
    for (key, value) in config {
        let current_key = if parent_key.is_empty() {
            key.clone()
        } else {
            format!("{parent_key}.{key}")
        };

        if let Value::Object(children) = value {
            flatten_config(children, &current_key, items, mask_sensitive);
            continue;
        }

        let masked = mask_sensitive && is_sensitive_path(&current_key);
        items.push(json!({
            "path": current_key,
            "value": if masked {
                mask_sensitive_value(&current_key, value)
            } else {
                value.clone()
            },
            "type": infer_type(value),
            "category": parent_key.split('.').next().unwrap_or("general"),
            "label": format_label(key),
            "description": format!("Configuration for {current_key}"),
            "masked": masked,
        }));
    }
}

fn infer_type(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "float",
        Value::Array(_) => "list",
        Value::Object(_) => "object",
        _ => "string",
    }
}

fn format_label(key: &str) -> String {
    key.replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut titled = first.to_uppercase().collect::<String>();
                    titled.push_str(&chars.as_str().to_lowercase());
                    titled
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_sensitive_path(path: &str) -> bool {
    let leaf = path
        .split('.')
        .next_back()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
        .replace('-', "_");
    if leaf.is_empty() {
        return false;
    }

    let explicit_sensitive = [
        "password",
        "passphrase",
        "secret",
        "secret_key",
        "private_key",
        "api_key",
        "apikey",
        "access_key",
        "client_secret",
        "token",
        "access_token",
        "refresh_token",
        "sse_token",
        "auth_token",
        "jwt_secret",
        "jwt_secret_key",
    ];
    if explicit_sensitive.iter().any(|item| *item == leaf) {
        return true;
    }

    leaf.contains("password")
        || leaf.contains("secret")
        || leaf.ends_with("_token")
        || leaf.ends_with("_api_key")
        || leaf.ends_with("_private_key")
}

fn mask_sensitive_value(path: &str, value: &Value) -> Value {
    if !is_sensitive_path(path) {
        return value.clone();
    }
    match value {
        Value::Null => Value::Null,
        Value::String(item) if item.is_empty() => value.clone(),
        Value::Array(items) if items.is_empty() => value.clone(),
        Value::Object(items) if items.is_empty() => value.clone(),
        _ => Value::String(MASKED_PLACEHOLDER.into()),
    }
}

fn sanitize_config_value(path: &str, value: Value) -> Value {
    if is_sensitive_path(path) {
        return mask_sensitive_value(path, &value);
    }

    match value {
        Value::Object(map) => {
            let mut sanitized = Map::new();
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                sanitized.insert(key, sanitize_config_value(&child_path, child));
            }
            Value::Object(sanitized)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| sanitize_config_value(path, item))
                .collect(),
        ),
        other => other,
    }
}

fn config_value_at<'a>(config: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.').filter(|part| !part.is_empty());
    let first = parts.next()?;
    let mut current = config.get(first)?;

    for part in parts {
        current = current.as_object()?.get(part)?;
    }

    Some(current)
}

fn config_string(config: &Map<String, Value>, path: &str) -> Option<String> {
    config_value_at(config, path).and_then(value_to_string)
}

fn config_string_list(config: &Map<String, Value>, path: &str) -> Option<Vec<String>> {
    let value = config_value_at(config, path)?;
    match value {
        Value::Array(items) => {
            let values = items
                .iter()
                .filter_map(value_to_string)
                .fold(Vec::<String>::new(), |mut acc, item| {
                    if !acc.iter().any(|existing| existing == &item) {
                        acc.push(item);
                    }
                    acc
                });
            Some(values)
        }
        Value::String(text) => Some(parse_aliases(text)),
        _ => value_to_string(value).map(|text| vec![text]),
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => {
            let normalized = text.trim();
            if normalized.is_empty() {
                None
            } else {
                Some(normalized.to_string())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        _ => None,
    }
}

fn parse_aliases(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .fold(Vec::<String>::new(), |mut acc, item| {
            if !acc.iter().any(|existing| existing == item) {
                acc.push(item.to_string());
            }
            acc
        })
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_bool_env(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(value) => matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}

fn runtime_role() -> String {
    let mut raw_role = env::var("APP_RUNTIME_ROLE")
        .unwrap_or_else(|_| "all".to_string())
        .trim()
        .to_ascii_lowercase();

    if matches!(raw_role.as_str(), "combined" | "standalone") {
        raw_role = "all".into();
    }

    match raw_role.as_str() {
        "all" | "api" | "worker" => raw_role,
        _ => "all".into(),
    }
}

fn is_distributed_mode() -> bool {
    if env::var_os("APP_DISTRIBUTED_MODE").is_some() {
        return read_bool_env("APP_DISTRIBUTED_MODE", false);
    }

    matches!(runtime_role().as_str(), "api" | "worker")
}

fn local_runtime_overrides_enabled() -> bool {
    if env::var_os("LOCAL_RUNTIME_OVERRIDES_ENABLED").is_some() {
        return read_bool_env("LOCAL_RUNTIME_OVERRIDES_ENABLED", true);
    }

    !is_distributed_mode()
}

fn system_config_backend() -> String {
    env::var("SYSTEM_CONFIG_BACKEND")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn system_config_table() -> String {
    let table = env::var("SYSTEM_CONFIG_TABLE")
        .unwrap_or_else(|_| DEFAULT_SYSTEM_CONFIG_TABLE.to_string())
        .trim()
        .to_string();
    if table.is_empty() {
        DEFAULT_SYSTEM_CONFIG_TABLE.to_string()
    } else {
        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            other => panic!("expected object, got {other}"),
        }
    }

    #[test]
    fn flatten_config_descends_full_tree_and_masks_sensitive_values() {
        let config = object(json!({
            "database": {
                "host": "localhost",
                "password": "super-secret"
            },
            "site": {
                "airport": {
                    "code": "SZX"
                }
            },
            "auth": {
                "secret_key": "jwt-secret",
                "access_token_expire_minutes": 30
            }
        }));

        let mut flags = Vec::new();
        flatten_config(&config, "", &mut flags, true);

        let by_path = flags
            .into_iter()
            .map(|item| {
                let path = item.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
                (path, item)
            })
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(by_path["database.host"]["value"], json!("localhost"));
        assert_eq!(by_path["database.password"]["value"], json!(MASKED_PLACEHOLDER));
        assert_eq!(by_path["database.password"]["masked"], json!(true));
        assert_eq!(by_path["auth.access_token_expire_minutes"]["value"], json!(30));
        assert_eq!(by_path["site.airport.code"]["category"], json!("site"));
    }

    #[test]
    fn flatten_config_matches_python_flag_metadata_shape() {
        let config = object(json!({
            "feature_flags": {
                "dispatch_chat_v1": {
                    "enabled": true,
                    "api_key": "demo-secret"
                }
            }
        }));

        let mut flags = Vec::new();
        flatten_config(&config, "", &mut flags, true);

        let by_path = flags
            .into_iter()
            .map(|item| {
                let path = item.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
                (path, item)
            })
            .collect::<std::collections::BTreeMap<_, _>>();

        let flag = by_path
            .get("feature_flags.dispatch_chat_v1.enabled")
            .expect("feature flag entry should exist");

        assert_eq!(flag["value"], json!(true));
        assert_eq!(flag["type"], json!("boolean"));
        assert_eq!(flag["category"], json!("feature_flags"));
        assert_eq!(flag["label"], json!("Enabled"));
        assert_eq!(
            flag["description"],
            json!("Configuration for feature_flags.dispatch_chat_v1.enabled")
        );
        assert_eq!(flag["masked"], json!(false));
        println!("system flags metadata payload: {}", flag);

        let api_key_flag = by_path
            .get("feature_flags.dispatch_chat_v1.api_key")
            .expect("api key flag entry should exist");
        assert_eq!(api_key_flag["label"], json!("Api Key"));
        assert_eq!(api_key_flag["masked"], json!(true));
    }

    #[test]
    fn insert_path_value_supports_root_sections_and_legacy_dotted_rows() {
        let mut config = Map::new();

        insert_path_value(
            &mut config,
            "feature_flags",
            json!({
                "dispatch_chat_v1": {
                    "enabled": false,
                    "rollout": 0.25
                }
            }),
        );
        insert_path_value(&mut config, "feature_flags.dispatch_chat_v1.enabled", json!(true));

        assert_eq!(
            config_value_at(&config, "feature_flags.dispatch_chat_v1.enabled"),
            Some(&json!(true))
        );
        assert_eq!(
            config_value_at(&config, "feature_flags.dispatch_chat_v1.rollout"),
            Some(&json!(0.25))
        );
    }

    #[test]
    fn load_env_source_builds_nested_tree_and_converts_scalars() {
        let config = load_env_source_from_iter(vec![
            ("APP_DISTRIBUTED_MODE".to_string(), "true".to_string()),
            ("API_PORT".to_string(), "8088".to_string()),
            ("NOTIFY_EXTERNAL_TIMEOUT_SECONDS".to_string(), "3.5".to_string()),
            ("SITE_AIRPORT_DISPLAY_NAME".to_string(), "深圳".to_string()),
        ]);

        assert_eq!(config_value_at(&config, "app.distributed.mode"), Some(&json!(true)));
        assert_eq!(config_value_at(&config, "api.port"), Some(&json!(8088)));
        assert_eq!(
            config_value_at(&config, "notify.external.timeout.seconds"),
            Some(&json!(3.5))
        );
        assert_eq!(
            config_value_at(&config, "site.airport.display.name"),
            Some(&json!("深圳"))
        );
    }

    #[test]
    fn writable_sources_exclude_local_files_in_distributed_mode() {
        let manager = LoadedConfigManager {
            snapshot: Map::new(),
            sources: vec![
                ConfigSourceDefinition::file("config/app_config.yaml"),
                ConfigSourceDefinition::file("config/runtime_overrides.yaml"),
                ConfigSourceDefinition::database("system_config".to_string()),
                ConfigSourceDefinition::environment(),
            ],
        };

        let local_names = manager
            .writable_sources(false)
            .into_iter()
            .map(|source| source.name.clone())
            .collect::<Vec<_>>();
        let distributed_names = manager
            .writable_sources(true)
            .into_iter()
            .map(|source| source.name.clone())
            .collect::<Vec<_>>();

        assert_eq!(
            local_names,
            vec![
                "file://config/app_config.yaml".to_string(),
                "file://config/runtime_overrides.yaml".to_string(),
                "db://system_config".to_string(),
            ]
        );
        assert_eq!(distributed_names, vec!["db://system_config".to_string()]);
    }

    #[test]
    fn sanitize_config_value_masks_nested_sensitive_export_fields() {
        let exported = sanitize_config_value(
            "",
            json!({
                "database": {
                    "password": "db-pass",
                    "host": "localhost"
                },
                "auth": {
                    "secret_key": "jwt-secret"
                },
                "gemini": {
                    "cli": {
                        "ide": {
                            "auth": {
                                "token": "abc-123"
                            }
                        }
                    }
                }
            }),
        );

        assert_eq!(exported["database"]["password"], json!(MASKED_PLACEHOLDER));
        assert_eq!(exported["auth"]["secret_key"], json!(MASKED_PLACEHOLDER));
        assert_eq!(
            exported["gemini"]["cli"]["ide"]["auth"]["token"],
            json!(MASKED_PLACEHOLDER)
        );
        assert_eq!(exported["database"]["host"], json!("localhost"));
        println!("system flags export payload: {}", exported);
    }
}
