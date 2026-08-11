use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::schemas::flight_import_schemas::{
    FlightImportCommitResultSchema, FlightImportPreviewDataSchema, FlightImportPreviewRowSchema,
    FlightImportSourceFileSchema, FlightImportSummarySchema,
};
use crate::schemas::flight_schemas::{FlightCreate, FlightLegPayload, FlightUpdate, NullableUpdate};
use crate::services::flight_commands::{FlightCreateCommand, FlightUpdateCommand};
use crate::types::ConcreteFlightService;

const DEFAULT_STORAGE_ROOT: &str = "data/flight_imports";
const DEFAULT_SOURCE_SYSTEM: &str = "payload_import";
const DEFAULT_MAPPING_VERSION: &str = "rust-flight-import-v1";

#[derive(Debug)]
pub enum FlightImportError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl fmt::Display for FlightImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) | Self::NotFound(message) | Self::Conflict(message) | Self::Internal(message) => {
                write!(f, "{message}")
            }
        }
    }
}

impl std::error::Error for FlightImportError {}

#[derive(Clone)]
pub struct FlightImportService {
    flight_service: Arc<ConcreteFlightService>,
    storage_root: PathBuf,
    preview_ttl_hours: i64,
    source_system: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NormalizedFlightPayload {
    flight_id: Option<String>,
    flight_number: Option<String>,
    airline_code: Option<String>,
    registration: Option<String>,
    aircraft_type_detail: Option<String>,
    status: Option<String>,
    scheduled_departure: Option<DateTime<Utc>>,
    scheduled_arrival: Option<DateTime<Utc>>,
    estimated_departure: Option<DateTime<Utc>>,
    estimated_arrival: Option<DateTime<Utc>>,
    actual_departure: Option<DateTime<Utc>>,
    actual_arrival: Option<DateTime<Utc>>,
    stand: Option<String>,
    gate: Option<String>,
    terminal: Option<String>,
    position: Option<String>,
    baggage_carousel: Option<String>,
    has_boarding_restriction: Option<bool>,
    is_quick_turnaround: Option<bool>,
    is_commercial_signed: Option<bool>,
    inbound_leg: Option<FlightLegPayload>,
    outbound_leg: Option<FlightLegPayload>,
    flight_remarks: Option<String>,
    load_planning_remarks: Option<String>,
    aircraft_maintenance_remarks: Option<String>,
    aircraft_check_remarks: Option<String>,
}

impl FlightImportService {
    pub fn new(flight_service: Arc<ConcreteFlightService>) -> Self {
        let storage_root = std::env::var("FLIGHT_IMPORT_STORAGE_ROOT")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_STORAGE_ROOT.to_string());
        let preview_ttl_hours = std::env::var("FLIGHT_IMPORT_PREVIEW_TTL_HOURS")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(24)
            .max(1);
        let source_system = std::env::var("FLIGHT_IMPORT_SOURCE_SYSTEM")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_SOURCE_SYSTEM.to_string());

        let service = Self {
            flight_service,
            storage_root: PathBuf::from(storage_root),
            preview_ttl_hours,
            source_system,
        };
        let _ = service.ensure_storage_dirs();
        service
    }

    pub async fn create_preview_from_bytes(
        &self,
        filename: &str,
        content: &[u8],
    ) -> Result<FlightImportPreviewDataSchema, FlightImportError> {
        let normalized_name = normalize_filename(filename);
        validate_suffix(&normalized_name)?;

        let source_file = FlightImportSourceFileSchema {
            filename: normalized_name.clone(),
            size: content.len(),
            checksum_sha256: sha256_hex(content),
        };
        let preview_id = ulid::Ulid::new().to_string();
        let created_at = Utc::now();
        let expires_at = created_at + Duration::hours(self.preview_ttl_hours);
        let airport_context = airport_context_payload();
        let field_mapping = field_mapping_payload();

        let parsed_rows = parse_rows(&normalized_name, content)?;
        let mut rows = Vec::with_capacity(parsed_rows.len());
        let mut top_errors = Vec::new();

        for (index, raw_row) in parsed_rows.into_iter().enumerate() {
            match self.build_preview_row(index + 1, raw_row).await {
                Ok(row) => rows.push(row),
                Err(error) => {
                    let source_row_key = format!("row-{}", index + 1);
                    rows.push(FlightImportPreviewRowSchema {
                        source_row_key,
                        match_strategy: None,
                        matched_flight_id: None,
                        action: "skip".to_string(),
                        normalized_flight: Value::Object(Map::new()),
                        timeline_events: Vec::new(),
                        warnings: Vec::new(),
                        errors: vec![error.to_string()],
                    });
                }
            }
        }

        if rows.is_empty() {
            top_errors.push("导入文件中没有可解析的航班行".to_string());
        }

        let summary = build_summary(&rows, &top_errors, 0);
        let preview = FlightImportPreviewDataSchema {
            preview_id: preview_id.clone(),
            airport_context,
            source_file,
            summary,
            rows,
            errors: top_errors,
            mapping_version: DEFAULT_MAPPING_VERSION.to_string(),
            status: "previewed".to_string(),
            field_mapping,
            created_at,
            expires_at,
            source_system: self.source_system.clone(),
        };
        self.write_json(&self.preview_path(&preview_id), &preview)?;
        Ok(preview)
    }

    pub fn get_preview(&self, preview_id: &str) -> Result<FlightImportPreviewDataSchema, FlightImportError> {
        let mut preview: FlightImportPreviewDataSchema = self.read_json(&self.preview_path(preview_id))?;
        preview.status = self.current_preview_status(preview_id, &preview)?;
        Ok(preview)
    }

    pub async fn commit_preview(
        &self,
        preview_id: &str,
        actor_id: &str,
        request_id: Option<String>,
    ) -> Result<FlightImportCommitResultSchema, FlightImportError> {
        let preview = self.get_preview(preview_id)?;
        match preview.status.as_str() {
            "expired" => {
                return Err(FlightImportError::Conflict(
                    "导入预览已过期，请重新上传文件".to_string(),
                ))
            }
            "committed" | "failed" => {
                return Err(FlightImportError::Conflict("该预览已提交，不能重复执行".to_string()))
            }
            _ => {}
        }
        if !preview.errors.is_empty() || preview.rows.iter().any(|row| !row.errors.is_empty()) {
            return Err(FlightImportError::Conflict(
                "预览包含错误，不能提交，请修正源文件后重新上传".to_string(),
            ));
        }

        let mut result_rows = Vec::with_capacity(preview.rows.len());
        let mut flight_ids = Vec::new();
        let mut errors = Vec::new();
        let mut failed_count = 0usize;

        for row in &preview.rows {
            let mut committed_row = row.clone();
            match committed_row.action.as_str() {
                "skip" => {
                    result_rows.push(committed_row);
                    continue;
                }
                "create" => {
                    let normalized = decode_normalized_flight(&committed_row.normalized_flight)?;
                    let create_payload = normalized.to_create_payload();
                    let command = FlightCreateCommand::new(create_payload, Some(actor_id.to_string()));
                    if let Err(error) = command.validate() {
                        failed_count += 1;
                        let message = error.to_string();
                        committed_row.errors.push(message.clone());
                        errors.push(message);
                    } else {
                        match self.flight_service.execute_create(command).await {
                            Ok(created) => {
                                if let Some(flight_id) = created.flight_id {
                                    flight_ids.push(flight_id.clone());
                                    committed_row.matched_flight_id = Some(flight_id);
                                }
                            }
                            Err(error) => {
                                failed_count += 1;
                                let message = error.to_string();
                                committed_row.errors.push(message.clone());
                                errors.push(message);
                            }
                        }
                    }
                }
                "update" => {
                    let flight_id = committed_row.matched_flight_id.clone().ok_or_else(|| {
                        FlightImportError::Conflict("预览缺少 matched_flight_id，无法更新航班".to_string())
                    })?;
                    let normalized = decode_normalized_flight(&committed_row.normalized_flight)?;
                    let update_payload = normalized.to_update_payload();
                    match FlightUpdateCommand::build(flight_id.clone(), update_payload, Some(actor_id.to_string())) {
                        Err(error) => {
                            failed_count += 1;
                            let message = error.to_string();
                            committed_row.errors.push(message.clone());
                            errors.push(message);
                        }
                        Ok(command) => match self.flight_service.execute_update(command).await {
                            Ok(Some(updated)) => {
                                if let Some(updated_id) = updated.flight_id {
                                    flight_ids.push(updated_id);
                                } else {
                                    flight_ids.push(flight_id);
                                }
                            }
                            Ok(None) => {
                                failed_count += 1;
                                let message = format!("导入目标航班不存在: {flight_id}");
                                committed_row.errors.push(message.clone());
                                errors.push(message);
                            }
                            Err(error) => {
                                failed_count += 1;
                                let message = error.to_string();
                                committed_row.errors.push(message.clone());
                                errors.push(message);
                            }
                        },
                    }
                }
                other => {
                    failed_count += 1;
                    let message = format!("不支持的导入动作: {other}");
                    committed_row.errors.push(message.clone());
                    errors.push(message);
                }
            }
            result_rows.push(committed_row);
        }

        let status = if failed_count == 0 { "committed" } else { "failed" };
        let summary = build_summary(&result_rows, &errors, failed_count);
        let result = FlightImportCommitResultSchema {
            preview_id: preview.preview_id.clone(),
            airport_context: preview.airport_context.clone(),
            source_file: preview.source_file.clone(),
            summary,
            rows: result_rows,
            errors,
            mapping_version: preview.mapping_version.clone(),
            status: status.to_string(),
            flight_ids,
            committed_at: Some(Utc::now()),
            field_mapping: preview.field_mapping.clone(),
            request_id,
            source_system: Some(preview.source_system.clone()),
        };

        self.write_json(&self.result_path(preview_id), &result)?;
        Ok(result)
    }

    pub fn get_result(&self, preview_id: &str) -> Result<FlightImportCommitResultSchema, FlightImportError> {
        self.read_json(&self.result_path(preview_id))
            .map_err(|error| match error {
                FlightImportError::NotFound(_) => FlightImportError::NotFound(format!("导入结果不存在: {preview_id}")),
                other => other,
            })
    }

    async fn build_preview_row(
        &self,
        row_number: usize,
        raw_row: HashMap<String, String>,
    ) -> Result<FlightImportPreviewRowSchema, FlightImportError> {
        let source_row_key = raw_row
            .get("source_row_key")
            .or_else(|| raw_row.get("row_key"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("row-{row_number}"));

        if row_is_blank(&raw_row) {
            return Ok(FlightImportPreviewRowSchema {
                source_row_key,
                match_strategy: Some("blank-row".to_string()),
                matched_flight_id: None,
                action: "skip".to_string(),
                normalized_flight: Value::Object(Map::new()),
                timeline_events: Vec::new(),
                warnings: vec!["空白行已跳过".to_string()],
                errors: Vec::new(),
            });
        }

        let forced_action = lookup(&raw_row, &["action"])
            .map(|value| value.to_ascii_lowercase())
            .filter(|value| matches!(value.as_str(), "create" | "update" | "skip"));
        if matches!(forced_action.as_deref(), Some("skip")) {
            return Ok(FlightImportPreviewRowSchema {
                source_row_key,
                match_strategy: Some("source-action".to_string()),
                matched_flight_id: None,
                action: "skip".to_string(),
                normalized_flight: Value::Object(Map::new()),
                timeline_events: Vec::new(),
                warnings: vec!["源数据标记为 skip，已跳过".to_string()],
                errors: Vec::new(),
            });
        }

        let normalized = normalize_row(raw_row)?;
        let mut warnings = Vec::new();
        if normalized.inbound_leg.is_none() && normalized.outbound_leg.is_none() {
            warnings.push("未识别到航段信息；提交时创建动作会失败".to_string());
        }

        let (matched_flight_id, match_strategy) = self.match_existing_flight(&normalized).await?;
        let action = match forced_action.as_deref() {
            Some("create") => "create".to_string(),
            Some("update") => {
                if matched_flight_id.is_none() {
                    return Ok(FlightImportPreviewRowSchema {
                        source_row_key,
                        match_strategy: Some("source-action".to_string()),
                        matched_flight_id: None,
                        action: "update".to_string(),
                        normalized_flight: serde_json::to_value(&normalized).map_err(serialize_error)?,
                        timeline_events: Vec::new(),
                        warnings,
                        errors: vec!["源数据要求 update，但未匹配到现有航班".to_string()],
                    });
                }
                "update".to_string()
            }
            _ => {
                if matched_flight_id.is_some() {
                    "update".to_string()
                } else {
                    "create".to_string()
                }
            }
        };

        Ok(FlightImportPreviewRowSchema {
            source_row_key,
            match_strategy,
            matched_flight_id,
            action,
            normalized_flight: serde_json::to_value(&normalized).map_err(serialize_error)?,
            timeline_events: Vec::new(),
            warnings,
            errors: Vec::new(),
        })
    }

    async fn match_existing_flight(
        &self,
        normalized: &NormalizedFlightPayload,
    ) -> Result<(Option<String>, Option<String>), FlightImportError> {
        if let Some(flight_id) = normalized.flight_id.as_deref() {
            if let Some(found) = self
                .flight_service
                .get_flight(flight_id)
                .await
                .map_err(map_domain_error)?
            {
                return Ok((found.flight_id, Some("flight_id".to_string())));
            }
        }

        if let Some(flight_number) = normalized.flight_number.as_deref() {
            let candidates = self
                .flight_service
                .search_flights(Some(flight_number), None, None, None, None, 1, 20)
                .await
                .map_err(map_domain_error)?;
            let mut exact_matches = candidates
                .into_iter()
                .filter(|item| {
                    item.flight_number
                        .as_deref()
                        .map(|value| value.eq_ignore_ascii_case(flight_number))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>();
            if exact_matches.len() == 1 {
                return Ok((exact_matches.remove(0).flight_id, Some("flight_number".to_string())));
            }
        }

        Ok((None, None))
    }

    fn current_preview_status(
        &self,
        preview_id: &str,
        preview: &FlightImportPreviewDataSchema,
    ) -> Result<String, FlightImportError> {
        if self.result_path(preview_id).exists() {
            let result: FlightImportCommitResultSchema = self.read_json(&self.result_path(preview_id))?;
            return Ok(result.status);
        }
        if preview.expires_at < Utc::now() {
            return Ok("expired".to_string());
        }
        Ok(preview.status.clone())
    }

    fn ensure_storage_dirs(&self) -> Result<(), FlightImportError> {
        fs::create_dir_all(self.previews_dir()).map_err(io_error)?;
        fs::create_dir_all(self.results_dir()).map_err(io_error)?;
        Ok(())
    }

    fn previews_dir(&self) -> PathBuf {
        self.storage_root.join("previews")
    }

    fn results_dir(&self) -> PathBuf {
        self.storage_root.join("results")
    }

    fn preview_path(&self, preview_id: &str) -> PathBuf {
        self.previews_dir().join(format!("{preview_id}.json"))
    }

    fn result_path(&self, preview_id: &str) -> PathBuf {
        self.results_dir().join(format!("{preview_id}.json"))
    }

    fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), FlightImportError> {
        self.ensure_storage_dirs()?;
        let bytes = serde_json::to_vec_pretty(value).map_err(serialize_error)?;
        fs::write(path, bytes).map_err(io_error)
    }

    fn read_json<T: for<'de> Deserialize<'de>>(&self, path: &Path) -> Result<T, FlightImportError> {
        let bytes = fs::read(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                FlightImportError::NotFound(format!("导入预览不存在: {}", path.display()))
            } else {
                io_error(error)
            }
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|error| FlightImportError::Internal(format!("读取导入快照失败: {error}")))
    }
}

impl NormalizedFlightPayload {
    fn to_create_payload(&self) -> FlightCreate {
        FlightCreate {
            flight_id: self.flight_id.clone(),
            flight_number: self.flight_number.clone(),
            airline_code: self.airline_code.clone(),
            registration: self.registration.clone(),
            aircraft_type_detail: self.aircraft_type_detail.clone(),
            status: self.status.clone(),
            scheduled_departure: self.scheduled_departure,
            scheduled_arrival: self.scheduled_arrival,
            estimated_departure: self.estimated_departure,
            estimated_arrival: self.estimated_arrival,
            actual_departure: self.actual_departure,
            actual_arrival: self.actual_arrival,
            stand: self.stand.clone(),
            gate: self.gate.clone(),
            terminal: self.terminal.clone(),
            position: self.position.clone(),
            baggage_carousel: self.baggage_carousel.clone(),
            has_boarding_restriction: self.has_boarding_restriction.unwrap_or(false),
            is_quick_turnaround: self.is_quick_turnaround.unwrap_or(false),
            is_commercial_signed: self.is_commercial_signed.unwrap_or(true),
            inbound_leg: self.inbound_leg.clone(),
            outbound_leg: self.outbound_leg.clone(),
            flight_remarks: self.flight_remarks.clone(),
            load_planning_remarks: self.load_planning_remarks.clone(),
            aircraft_maintenance_remarks: self.aircraft_maintenance_remarks.clone(),
            aircraft_check_remarks: self.aircraft_check_remarks.clone(),
        }
    }

    fn to_update_payload(&self) -> FlightUpdate {
        FlightUpdate {
            expected_version: None,
            status: self.status.clone(),
            gate: update_from_option(self.gate.clone()),
            terminal: update_from_option(self.terminal.clone()),
            stand: update_from_option(self.stand.clone()),
            position: update_from_option(self.position.clone()),
            baggage_carousel: update_from_option(self.baggage_carousel.clone()),
            scheduled_departure: update_from_option(self.scheduled_departure),
            scheduled_arrival: update_from_option(self.scheduled_arrival),
            estimated_departure: update_from_option(self.estimated_departure),
            estimated_arrival: update_from_option(self.estimated_arrival),
            actual_departure: update_from_option(self.actual_departure),
            actual_arrival: update_from_option(self.actual_arrival),
            cobt_time: NullableUpdate::Unset,
            aircraft_type_detail: update_from_option(self.aircraft_type_detail.clone()),
            registration: update_from_option(self.registration.clone()),
            has_boarding_restriction: self.has_boarding_restriction,
            is_quick_turnaround: self.is_quick_turnaround,
            is_commercial_signed: self.is_commercial_signed,
            inbound_leg: update_from_option(self.inbound_leg.clone()),
            outbound_leg: update_from_option(self.outbound_leg.clone()),
            flight_remarks: update_from_option(self.flight_remarks.clone()),
            load_planning_remarks: update_from_option(self.load_planning_remarks.clone()),
            aircraft_maintenance_remarks: update_from_option(self.aircraft_maintenance_remarks.clone()),
            aircraft_check_remarks: update_from_option(self.aircraft_check_remarks.clone()),
        }
    }
}

fn update_from_option<T>(value: Option<T>) -> NullableUpdate<T> {
    match value {
        Some(value) => NullableUpdate::Set(value),
        None => NullableUpdate::Unset,
    }
}

fn parse_rows(filename: &str, content: &[u8]) -> Result<Vec<HashMap<String, String>>, FlightImportError> {
    let suffix = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    match suffix.as_str() {
        "json" => parse_json_rows(content),
        "csv" | "txt" => parse_csv_rows(content),
        _ => Err(FlightImportError::Validation(
            "仅支持上传 csv、txt 或 json 文件".to_string(),
        )),
    }
}

fn parse_json_rows(content: &[u8]) -> Result<Vec<HashMap<String, String>>, FlightImportError> {
    let value: Value = serde_json::from_slice(content)
        .map_err(|error| FlightImportError::Validation(format!("JSON 解析失败: {error}")))?;
    let rows = match value {
        Value::Array(items) => items,
        Value::Object(mut object) => object
            .remove("rows")
            .and_then(|value| value.as_array().cloned())
            .ok_or_else(|| FlightImportError::Validation("JSON 需要数组或 rows 字段".to_string()))?,
        _ => return Err(FlightImportError::Validation("JSON 需要数组或 rows 字段".to_string())),
    };

    rows.into_iter().map(json_object_to_map).collect()
}

fn json_object_to_map(value: Value) -> Result<HashMap<String, String>, FlightImportError> {
    let Value::Object(object) = value else {
        return Err(FlightImportError::Validation("JSON 行必须是对象".to_string()));
    };
    let mut row = HashMap::new();
    for (key, value) in object {
        row.insert(
            key,
            match value {
                Value::Null => String::new(),
                Value::String(value) => value,
                other => other.to_string(),
            },
        );
    }
    Ok(row)
}

fn parse_csv_rows(content: &[u8]) -> Result<Vec<HashMap<String, String>>, FlightImportError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(content);
    let headers = reader
        .headers()
        .map_err(|error| FlightImportError::Validation(format!("CSV 表头解析失败: {error}")))?
        .clone();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| FlightImportError::Validation(format!("CSV 行解析失败: {error}")))?;
        let mut row = HashMap::new();
        for (index, header) in headers.iter().enumerate() {
            row.insert(header.to_string(), record.get(index).unwrap_or("").trim().to_string());
        }
        rows.push(row);
        // Hard cap on row count to prevent unbounded memory growth
        const MAX_IMPORT_ROWS: usize = 2000;
        if rows.len() > MAX_IMPORT_ROWS {
            return Err(FlightImportError::Validation(format!(
                "导入行数超出上限（{} 行），请分批导入",
                MAX_IMPORT_ROWS
            )));
        }
    }
    Ok(rows)
}

fn normalize_row(raw_row: HashMap<String, String>) -> Result<NormalizedFlightPayload, FlightImportError> {
    let flight_id = lookup(&raw_row, &["flight_id", "id"]);
    let flight_number =
        lookup(&raw_row, &["flight_number", "flight_no", "flightNo"]).map(|value| value.to_ascii_uppercase());
    let airline_code =
        lookup(&raw_row, &["airline_code", "airline"]).or_else(|| flight_number.as_deref().map(infer_airline_code));
    let inbound_leg = build_leg_payload(&raw_row, "inbound")?;
    let outbound_leg = build_leg_payload(&raw_row, "outbound")?;
    let generic_leg = if inbound_leg.is_none() && outbound_leg.is_none() {
        build_generic_leg_payload(&raw_row)?
    } else {
        None
    };

    let (inbound_leg, outbound_leg) = if let Some(leg) = generic_leg {
        if leg.leg_type == "inbound" {
            (Some(leg), None)
        } else {
            (None, Some(leg))
        }
    } else {
        (inbound_leg, outbound_leg)
    };

    Ok(NormalizedFlightPayload {
        flight_id,
        flight_number,
        airline_code,
        registration: lookup(&raw_row, &["registration"]),
        aircraft_type_detail: lookup(&raw_row, &["aircraft_type_detail", "aircraft_type"]),
        status: lookup(&raw_row, &["status"]).map(map_status),
        scheduled_departure: parse_datetime_field(&raw_row, &["scheduled_departure", "std"])?,
        scheduled_arrival: parse_datetime_field(&raw_row, &["scheduled_arrival", "sta"])?,
        estimated_departure: parse_datetime_field(&raw_row, &["estimated_departure", "etd"])?,
        estimated_arrival: parse_datetime_field(&raw_row, &["estimated_arrival", "eta"])?,
        actual_departure: parse_datetime_field(&raw_row, &["actual_departure", "atd"])?,
        actual_arrival: parse_datetime_field(&raw_row, &["actual_arrival", "ata"])?,
        stand: lookup(&raw_row, &["stand"]),
        gate: lookup(&raw_row, &["gate"]),
        terminal: lookup(&raw_row, &["terminal"]),
        position: lookup(&raw_row, &["position"]),
        baggage_carousel: lookup(&raw_row, &["baggage_carousel"]),
        has_boarding_restriction: parse_bool_field(&raw_row, &["has_boarding_restriction"])?,
        is_quick_turnaround: parse_bool_field(&raw_row, &["is_quick_turnaround"])?,
        is_commercial_signed: parse_bool_field(&raw_row, &["is_commercial_signed"])?,
        inbound_leg,
        outbound_leg,
        flight_remarks: lookup(&raw_row, &["flight_remarks", "remarks"]),
        load_planning_remarks: lookup(&raw_row, &["load_planning_remarks"]),
        aircraft_maintenance_remarks: lookup(&raw_row, &["aircraft_maintenance_remarks"]),
        aircraft_check_remarks: lookup(&raw_row, &["aircraft_check_remarks"]),
    })
}

fn build_leg_payload(
    raw_row: &HashMap<String, String>,
    prefix: &str,
) -> Result<Option<FlightLegPayload>, FlightImportError> {
    let flight_no = lookup(
        raw_row,
        &[&format!("{prefix}_flight_no"), &format!("{prefix}_flight_number")],
    )
    .or_else(|| lookup(raw_row, &["flight_number", "flight_no"]));
    let origin_code = lookup(raw_row, &[&format!("{prefix}_origin_code")]);
    let destination_code = lookup(raw_row, &[&format!("{prefix}_destination_code")]);
    let origin_name = lookup(raw_row, &[&format!("{prefix}_origin_name")]);
    let destination_name = lookup(raw_row, &[&format!("{prefix}_destination_name")]);

    if flight_no.is_none()
        && origin_code.is_none()
        && destination_code.is_none()
        && origin_name.is_none()
        && destination_name.is_none()
    {
        return Ok(None);
    }

    Ok(Some(FlightLegPayload {
        leg_type: prefix.to_string(),
        flight_no: flight_no.unwrap_or_default().to_ascii_uppercase(),
        flight_type: lookup(raw_row, &[&format!("{prefix}_flight_type")]).unwrap_or_else(|| "domestic".to_string()),
        mission: parse_i32_field(raw_row, &[&format!("{prefix}_mission")])?,
        origin_stations: station_payloads(origin_code.clone(), origin_name.clone()),
        destination_stations: station_payloads(destination_code.clone(), destination_name.clone()),
        origin_code,
        destination_code,
        origin_name,
        destination_name,
        is_vip: parse_bool_field(raw_row, &[&format!("{prefix}_is_vip")])?.unwrap_or(false),
        stand_type: lookup(raw_row, &[&format!("{prefix}_stand_type")]),
        scheduled_time: parse_datetime_field(raw_row, &[&format!("{prefix}_scheduled_time")])?,
    }))
}

fn build_generic_leg_payload(raw_row: &HashMap<String, String>) -> Result<Option<FlightLegPayload>, FlightImportError> {
    let flight_no = lookup(raw_row, &["flight_no", "flight_number"]);
    let origin_code = lookup(raw_row, &["origin_code"]);
    let destination_code = lookup(raw_row, &["destination_code"]);
    let origin_name = lookup(raw_row, &["origin_name"]);
    let destination_name = lookup(raw_row, &["destination_name"]);
    if flight_no.is_none()
        && origin_code.is_none()
        && destination_code.is_none()
        && origin_name.is_none()
        && destination_name.is_none()
    {
        return Ok(None);
    }

    let leg_type = lookup(raw_row, &["leg_type"])
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| value == "inbound" || value == "outbound")
        .unwrap_or_else(|| {
            if destination_code.is_some() {
                "outbound".to_string()
            } else {
                "inbound".to_string()
            }
        });

    Ok(Some(FlightLegPayload {
        leg_type,
        flight_no: flight_no.unwrap_or_default().to_ascii_uppercase(),
        flight_type: lookup(raw_row, &["flight_type"]).unwrap_or_else(|| "domestic".to_string()),
        mission: parse_i32_field(raw_row, &["mission"])?,
        origin_stations: station_payloads(origin_code.clone(), origin_name.clone()),
        destination_stations: station_payloads(destination_code.clone(), destination_name.clone()),
        origin_code,
        destination_code,
        origin_name,
        destination_name,
        is_vip: parse_bool_field(raw_row, &["is_vip"])?.unwrap_or(false),
        stand_type: lookup(raw_row, &["stand_type"]),
        scheduled_time: parse_datetime_field(raw_row, &["scheduled_time"])?,
    }))
}

fn station_payloads(
    code: Option<String>,
    name: Option<String>,
) -> Vec<crate::schemas::flight_schemas::RouteStationPayload> {
    match code {
        Some(code) => vec![crate::schemas::flight_schemas::RouteStationPayload { code, name }],
        None => Vec::new(),
    }
}

fn parse_datetime_field(
    raw_row: &HashMap<String, String>,
    keys: &[&str],
) -> Result<Option<DateTime<Utc>>, FlightImportError> {
    let Some(value) = lookup(raw_row, keys) else {
        return Ok(None);
    };
    parse_datetime(&value).map(Some)
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, FlightImportError> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Ok(parsed.with_timezone(&Utc));
    }
    for pattern in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, pattern) {
            return Ok(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc));
        }
    }
    Err(FlightImportError::Validation(format!("无效时间格式: {value}")))
}

fn parse_bool_field(raw_row: &HashMap<String, String>, keys: &[&str]) -> Result<Option<bool>, FlightImportError> {
    let Some(value) = lookup(raw_row, keys) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "y" => Ok(Some(true)),
        "0" | "false" | "no" | "n" => Ok(Some(false)),
        _ => Err(FlightImportError::Validation(format!("无效布尔值: {value}"))),
    }
}

fn parse_i32_field(raw_row: &HashMap<String, String>, keys: &[&str]) -> Result<Option<i32>, FlightImportError> {
    let Some(value) = lookup(raw_row, keys) else {
        return Ok(None);
    };
    value
        .parse::<i32>()
        .map(Some)
        .map_err(|_| FlightImportError::Validation(format!("无效整数值: {value}")))
}

fn lookup(raw_row: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        raw_row
            .get(*key)
            .or_else(|| raw_row.get(&key.to_ascii_lowercase()))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn row_is_blank(raw_row: &HashMap<String, String>) -> bool {
    raw_row.values().all(|value| value.trim().is_empty())
}

fn infer_airline_code(flight_number: &str) -> String {
    flight_number
        .chars()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .take(3)
        .collect::<String>()
        .to_ascii_uppercase()
}

fn map_status(status: String) -> String {
    match status.trim() {
        "计划" | "计划中" => "SCHEDULED".to_string(),
        "登机" | "登机中" => "BOARDING".to_string(),
        "到达" | "到达本站" => "ARRIVED".to_string(),
        "起飞" | "离港" | "已起飞" => "DEPARTED".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

fn validate_suffix(filename: &str) -> Result<(), FlightImportError> {
    let extension = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(extension.as_str(), "csv" | "txt" | "json") {
        Ok(())
    } else {
        Err(FlightImportError::Validation(
            "仅支持上传 csv、txt 或 json 文件".to_string(),
        ))
    }
}

fn normalize_filename(filename: &str) -> String {
    let trimmed = filename.trim();
    if trimmed.is_empty() {
        "flight-import.csv".to_string()
    } else {
        trimmed.to_string()
    }
}

fn sha256_hex(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

fn airport_context_payload() -> Value {
    let code = env_first(&["SITE_AIRPORT_CODE", "AIRPORT_CODE"]).unwrap_or_default();
    let display_name =
        env_first(&["SITE_AIRPORT_DISPLAY_NAME", "AIRPORT_DISPLAY_NAME"]).unwrap_or_else(|| "本站".to_string());
    let aliases = env_first(&["SITE_AIRPORT_NAME_ALIASES", "AIRPORT_NAME_ALIASES"])
        .map(|value| value.split(',').map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();

    build_airport_context_payload(&code, &display_name, aliases)
}

fn build_airport_context_payload(code: &str, display_name: &str, aliases: Vec<String>) -> Value {
    json!({
        "code": code.trim().to_ascii_uppercase(),
        "display_name": normalize_airport_display_name(display_name),
        "name_aliases": normalize_airport_aliases(display_name, aliases),
    })
}

fn normalize_airport_display_name(display_name: &str) -> String {
    let normalized = display_name.trim();
    if normalized.is_empty() {
        "本站".to_string()
    } else {
        normalized.to_string()
    }
}

fn normalize_airport_aliases(display_name: &str, aliases: Vec<String>) -> Vec<String> {
    let display_name = normalize_airport_display_name(display_name);
    let mut normalized = vec![display_name.clone()];

    for alias in aliases {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !normalized.iter().any(|existing| existing == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }

    normalized
}

fn field_mapping_payload() -> Value {
    json!({
        "accepted_fields": [
            "flight_id", "flight_number", "flight_no", "airline_code", "status",
            "scheduled_departure", "scheduled_arrival", "estimated_departure", "estimated_arrival",
            "actual_departure", "actual_arrival", "gate", "stand", "terminal", "position",
            "baggage_carousel", "registration", "aircraft_type_detail", "origin_code",
            "origin_name", "destination_code", "destination_name", "leg_type",
            "inbound_*", "outbound_*"
        ],
        "match_strategies": ["flight_id", "flight_number"],
        "notes": [
            "csv/txt 按表头解析，json 支持数组或 { rows: [...] }",
            "未匹配现有航班时默认走 create，唯一匹配 flight_number 时走 update"
        ]
    })
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{build_airport_context_payload, normalize_airport_aliases};

    #[test]
    fn airport_context_payload_inserts_display_name_first_and_dedupes_aliases() {
        let payload = build_airport_context_payload(
            "szx",
            "深圳",
            vec!["深圳机场".into(), "深圳".into(), " 深圳机场 ".into()],
        );

        assert_eq!(payload["code"], "SZX");
        assert_eq!(payload["display_name"], "深圳");
        assert_eq!(payload["name_aliases"], serde_json::json!(["深圳", "深圳机场"]));
    }

    #[test]
    fn airport_context_aliases_fall_back_to_site_placeholder() {
        let aliases = normalize_airport_aliases("  ", vec![" ".into(), "广州机场".into()]);

        assert_eq!(aliases, vec!["本站".to_string(), "广州机场".to_string()]);
    }
}

fn build_summary(
    rows: &[FlightImportPreviewRowSchema],
    top_errors: &[String],
    failed_count: usize,
) -> FlightImportSummarySchema {
    let total_rows = rows.len();
    let invalid_rows = rows.iter().filter(|row| !row.errors.is_empty()).count();
    let valid_rows = total_rows.saturating_sub(invalid_rows);
    let create_count = rows.iter().filter(|row| row.action == "create").count();
    let update_count = rows.iter().filter(|row| row.action == "update").count();
    let skip_count = rows.iter().filter(|row| row.action == "skip").count();
    let warning_count = rows.iter().map(|row| row.warnings.len()).sum();
    let error_count = top_errors.len() + rows.iter().map(|row| row.errors.len()).sum::<usize>();

    FlightImportSummarySchema {
        total_rows,
        valid_rows,
        invalid_rows,
        create_count,
        update_count,
        skip_count,
        failed_count,
        warning_count,
        error_count,
    }
}

fn decode_normalized_flight(value: &Value) -> Result<NormalizedFlightPayload, FlightImportError> {
    serde_json::from_value(value.clone())
        .map_err(|error| FlightImportError::Internal(format!("导入预览数据损坏: {error}")))
}

fn serialize_error(error: serde_json::Error) -> FlightImportError {
    FlightImportError::Internal(format!("导入快照序列化失败: {error}"))
}

fn io_error(error: std::io::Error) -> FlightImportError {
    FlightImportError::Internal(format!("导入文件存储失败: {error}"))
}

fn map_domain_error(error: fms_domain::error::DomainError) -> FlightImportError {
    FlightImportError::Internal(error.to_string())
}
