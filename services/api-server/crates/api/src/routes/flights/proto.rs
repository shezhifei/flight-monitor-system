//! 航班列表 Protobuf 响应格式转换。

use prost::Message;
use serde_json::Value;

use fms_application::schemas::flight_schemas::FlightResponse;

static NULL_VALUE: serde_json::Value = serde_json::Value::Null;

#[derive(Clone, PartialEq, Message)]
struct ProtoBusinessCase {
    #[prost(string, tag = "1")]
    case_id: String,
    #[prost(string, tag = "2")]
    case_type: String,
    #[prost(string, tag = "3")]
    description: String,
    #[prost(string, tag = "4")]
    flight_id: String,
    #[prost(string, tag = "5")]
    flight_no: String,
    #[prost(string, tag = "6")]
    status: String,
    #[prost(string, tag = "7")]
    created_at: String,
    #[prost(string, tag = "8")]
    created_by: String,
    #[prost(string, tag = "9")]
    updated_by: String,
    #[prost(string, tag = "10")]
    finished_at: String,
    #[prost(string, tag = "11")]
    cancelled_at: String,
    #[prost(string, tag = "12")]
    stand: String,
    #[prost(string, tag = "13")]
    gate: String,
    #[prost(string, repeated, tag = "14")]
    log: Vec<String>,
    #[prost(string, tag = "15")]
    context_json: String,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoFlight {
    #[prost(string, tag = "1")]
    flight_id: String,
    #[prost(string, tag = "2")]
    flight_number: String,
    #[prost(string, tag = "3")]
    airline_code: String,
    #[prost(string, tag = "4")]
    registration: String,
    #[prost(string, tag = "13")]
    aircraft_type_detail: String,
    #[prost(string, tag = "15")]
    status: String,
    #[prost(string, tag = "16")]
    scheduled_departure: String,
    #[prost(string, tag = "17")]
    scheduled_arrival: String,
    #[prost(string, tag = "18")]
    estimated_departure: String,
    #[prost(string, tag = "19")]
    estimated_arrival: String,
    #[prost(string, tag = "20")]
    actual_departure: String,
    #[prost(string, tag = "21")]
    actual_arrival: String,
    #[prost(string, tag = "22")]
    stand: String,
    #[prost(string, tag = "23")]
    gate: String,
    #[prost(string, tag = "24")]
    terminal: String,
    #[prost(string, tag = "25")]
    position: String,
    #[prost(string, tag = "26")]
    baggage_carousel: String,
    #[prost(bool, tag = "28")]
    has_boarding_restriction: bool,
    #[prost(bool, tag = "29")]
    is_quick_turnaround: bool,
    #[prost(bool, tag = "30")]
    is_commercial_signed: bool,
    #[prost(string, tag = "35")]
    created_at: String,
    #[prost(string, tag = "36")]
    updated_at: String,
    #[prost(int32, tag = "37")]
    version: i32,
    #[prost(string, tag = "55")]
    flight_remarks: String,
    #[prost(string, tag = "56")]
    load_planning_remarks: String,
    #[prost(string, tag = "57")]
    aircraft_maintenance_remarks: String,
    #[prost(string, tag = "58")]
    aircraft_check_remarks: String,
    #[prost(message, repeated, tag = "59")]
    business_cases: Vec<ProtoBusinessCase>,
    #[prost(string, tag = "60")]
    created_by: String,
    #[prost(string, tag = "61")]
    updated_by: String,
    #[prost(string, tag = "62")]
    inbound_leg_json: String,
    #[prost(string, tag = "63")]
    outbound_leg_json: String,
    #[prost(string, tag = "64")]
    anomaly_summary_json: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct FlightsListEnvelope {
    #[prost(bool, tag = "1")]
    success: bool,
    #[prost(string, tag = "2")]
    message: String,
    #[prost(message, repeated, tag = "3")]
    flights: Vec<ProtoFlight>,
    #[prost(int32, tag = "4")]
    page: i32,
    #[prost(int32, tag = "5")]
    page_size: i32,
}

#[derive(Clone, PartialEq, Message)]
#[allow(dead_code)]
struct FlightStreamFrame {
    #[prost(string, tag = "1")]
    frame_type: String,
    #[prost(string, tag = "2")]
    flight_id: String,
    #[prost(string, repeated, tag = "3")]
    changed_fields: Vec<String>,
    #[prost(message, repeated, tag = "4")]
    flights: Vec<ProtoFlight>,
    #[prost(message, optional, tag = "5")]
    flight: Option<ProtoFlight>,
    #[prost(string, tag = "6")]
    timestamp: String,
    #[prost(string, tag = "7")]
    new_status: String,
}

fn opt_string_or_default(value: &Option<String>) -> String {
    value.as_deref().unwrap_or_default().to_owned()
}

fn json_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn json_json_string(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn to_proto_business_case(value: &serde_json::Value) -> ProtoBusinessCase {
    let get = |key: &str| value.get(key).unwrap_or(&NULL_VALUE);
    ProtoBusinessCase {
        case_id: json_string(get("case_id")),
        case_type: json_string(get("case_type")),
        description: json_string(get("description")),
        flight_id: json_string(get("flight_id")),
        flight_no: json_string(get("flight_no")),
        status: json_string(get("status")),
        created_at: json_string(get("created_at")),
        created_by: json_string(get("created_by")),
        updated_by: json_string(get("updated_by")),
        finished_at: json_string(get("finished_at")),
        cancelled_at: json_string(get("cancelled_at")),
        stand: json_string(get("stand")),
        gate: json_string(get("gate")),
        log: get("log")
            .as_array()
            .map(|items| items.iter().map(json_string).filter(|item| !item.is_empty()).collect())
            .unwrap_or_default(),
        context_json: json_json_string(value.get("context")),
    }
}

fn to_proto_flight(flight: &FlightResponse) -> ProtoFlight {
    ProtoFlight {
        flight_id: opt_string_or_default(&flight.flight_id),
        flight_number: opt_string_or_default(&flight.flight_number),
        airline_code: opt_string_or_default(&flight.airline_code),
        registration: opt_string_or_default(&flight.registration),
        aircraft_type_detail: opt_string_or_default(&flight.aircraft_type_detail),
        status: opt_string_or_default(&flight.status),
        scheduled_departure: flight
            .scheduled_departure
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        scheduled_arrival: flight
            .scheduled_arrival
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        estimated_departure: flight
            .estimated_departure
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        estimated_arrival: flight
            .estimated_arrival
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        actual_departure: flight
            .actual_departure
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        actual_arrival: flight
            .actual_arrival
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        stand: opt_string_or_default(&flight.stand),
        gate: opt_string_or_default(&flight.gate),
        terminal: opt_string_or_default(&flight.terminal),
        position: opt_string_or_default(&flight.position),
        baggage_carousel: opt_string_or_default(&flight.baggage_carousel),
        has_boarding_restriction: flight.has_boarding_restriction,
        is_quick_turnaround: flight.is_quick_turnaround,
        is_commercial_signed: flight.is_commercial_signed,
        created_at: flight.created_at.map(|value| value.to_rfc3339()).unwrap_or_default(),
        updated_at: flight.updated_at.map(|value| value.to_rfc3339()).unwrap_or_default(),
        version: flight.version,
        flight_remarks: opt_string_or_default(&flight.flight_remarks),
        load_planning_remarks: opt_string_or_default(&flight.load_planning_remarks),
        aircraft_maintenance_remarks: opt_string_or_default(&flight.aircraft_maintenance_remarks),
        aircraft_check_remarks: opt_string_or_default(&flight.aircraft_check_remarks),
        business_cases: flight.business_cases.iter().map(to_proto_business_case).collect(),
        created_by: opt_string_or_default(&flight.created_by),
        updated_by: opt_string_or_default(&flight.updated_by),
        inbound_leg_json: flight
            .inbound_leg
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok())
            .unwrap_or_default(),
        outbound_leg_json: flight
            .outbound_leg
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok())
            .unwrap_or_default(),
        anomaly_summary_json: serde_json::to_string(&flight.anomaly_summary).unwrap_or_else(|_| "{}".to_string()),
    }
}

pub fn build_flights_list_envelope_bytes(
    flights: &[FlightResponse],
    message: &str,
    page: i64,
    page_size: i64,
) -> Vec<u8> {
    FlightsListEnvelope {
        success: true,
        message: message.to_string(),
        flights: flights.iter().map(to_proto_flight).collect(),
        page: page as i32,
        page_size: page_size as i32,
    }
    .encode_to_vec()
}

#[allow(dead_code)]
pub fn build_flight_stream_frame_bytes(payload: &Value, event_type: Option<&str>) -> Vec<u8> {
    let resolved_type = event_type
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            payload
                .get("type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "message".to_string());

    let flights = payload
        .get("flights")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .map(|flight: FlightResponse| to_proto_flight(&flight))
                .collect()
        })
        .unwrap_or_default();
    let flight = payload
        .get("flight")
        .or_else(|| payload.get("flight_data"))
        .or_else(|| payload.get("data"))
        .and_then(|item| serde_json::from_value(item.clone()).ok())
        .map(|flight: FlightResponse| to_proto_flight(&flight));

    FlightStreamFrame {
        frame_type: resolved_type,
        flight_id: payload.get("flight_id").map(json_string).unwrap_or_default(),
        changed_fields: payload
            .get("changed_fields")
            .and_then(serde_json::Value::as_array)
            .map(|items| items.iter().map(json_string).filter(|item| !item.is_empty()).collect())
            .unwrap_or_default(),
        flights,
        flight,
        timestamp: payload.get("timestamp").map(json_string).unwrap_or_default(),
        new_status: payload.get("new_status").map(json_string).unwrap_or_default(),
    }
    .encode_to_vec()
}
