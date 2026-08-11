//! 航班 SSE/WS 广播辅助函数。

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use actix_web::web;
use futures_core::Stream;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::sse::hub::{SseHub, SseMessage};

pub async fn broadcast_flight_event(hub: &Arc<SseHub>, event: &str, payload: Value, status_changed: bool) {
    if status_changed {
        let _ = hub.broadcast_event("flights", Some(event), payload.clone()).await;
        let _ = hub
            .broadcast_event("flight_status_changes", Some("flight_status_changed"), payload)
            .await;
    } else {
        let _ = hub.broadcast_event("flights", Some(event), payload).await;
    }
}

#[allow(dead_code)]
pub struct FlightSseStream {
    receivers: Vec<broadcast::Receiver<SseMessage>>,
    heartbeat: tokio::time::Interval,
}

impl Stream for FlightSseStream {
    type Item = Result<actix_web::web::Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.heartbeat.poll_tick(cx).is_ready() {
            return Poll::Ready(Some(Ok(heartbeat_sse_payload_bytes())));
        }

        for receiver in self.receivers.iter_mut() {
            match receiver.try_recv() {
                Ok(message) => {
                    let event = message.event.unwrap_or(message.topic);
                    let data: &str = message.serialized_data.as_ref();
                    return Poll::Ready(Some(Ok(sse_payload_bytes(&event, data))));
                }
                Err(broadcast::error::TryRecvError::Empty)
                | Err(broadcast::error::TryRecvError::Lagged(_))
                | Err(broadcast::error::TryRecvError::Closed) => continue,
            }
        }

        Poll::Pending
    }
}

pub fn unix_timestamp_value() -> serde_json::Value {
    serde_json::Number::from_f64(chrono::Utc::now().timestamp_millis() as f64 / 1000.0)
        .map(serde_json::Value::Number)
        .unwrap_or_else(|| serde_json::Value::from(0))
}

#[allow(dead_code)]
pub fn normalize_ws_timestamp(payload: &mut serde_json::Value) {
    let Some(timestamp) = payload.get("timestamp").cloned() else {
        return;
    };

    if timestamp.is_number() {
        return;
    }

    let normalized = timestamp
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .and_then(|value| {
            serde_json::Number::from_f64(value.timestamp_millis() as f64 / 1000.0).map(serde_json::Value::Number)
        })
        .unwrap_or_else(unix_timestamp_value);
    payload["timestamp"] = normalized;
}

#[allow(dead_code)]
pub fn websocket_payload(message: &SseMessage, fallback_event: &str) -> Option<(String, serde_json::Value, String)> {
    let event_type = message
        .event
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_event)
        .to_string();
    let mut payload = match serde_json::from_str::<serde_json::Value>(message.serialized_data.as_ref()) {
        Ok(value) if value.is_object() => value,
        Ok(value) => json!({
            "type": event_type.as_str(),
            "data": value,
            "timestamp": unix_timestamp_value(),
        }),
        Err(_) => json!({
            "type": event_type.as_str(),
            "data": message.serialized_data.as_ref(),
            "timestamp": unix_timestamp_value(),
        }),
    };
    if payload.get("type").is_none() {
        payload["type"] = serde_json::Value::String(event_type.clone());
    }
    normalize_ws_timestamp(&mut payload);
    let payload_json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    Some((event_type, payload, payload_json))
}

pub fn sse_payload_bytes(event: &str, data: &str) -> web::Bytes {
    let mut payload = Vec::with_capacity("event: \ndata: \n\n".len() + event.len() + data.len());
    payload.extend_from_slice(b"event: ");
    payload.extend_from_slice(event.as_bytes());
    payload.extend_from_slice(b"\ndata: ");
    payload.extend_from_slice(data.as_bytes());
    payload.extend_from_slice(b"\n\n");
    web::Bytes::from(payload)
}

pub fn heartbeat_sse_payload_bytes() -> web::Bytes {
    let timestamp = unix_timestamp_value();
    let timestamp = timestamp.to_string();
    let mut data = String::with_capacity("{\"timestamp\":}".len() + timestamp.len());
    data.push_str("{\"timestamp\":");
    data.push_str(&timestamp);
    data.push('}');
    sse_payload_bytes("heartbeat", &data)
}
