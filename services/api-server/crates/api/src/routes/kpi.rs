//! KPI 仪表盘路由。
//!
//! 对齐 Python `kpi_routes.py` 的参数、权限与返回结构。

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use actix_web::{web, HttpResponse};
use chrono::{NaiveDate, Utc};
use futures_core::Stream;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::sse::hub::{SseHub, SseMessage};
use fms_application::services::kpi_aggregation_service::KpiAggregationService;

fn ok_resp(data: impl serde::Serialize, message: &str) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": data,
        "message": message,
    }))
}

#[allow(dead_code)]
struct KpiSseStream {
    receiver: broadcast::Receiver<SseMessage>,
    heartbeat: tokio::time::Interval,
}

impl Stream for KpiSseStream {
    type Item = Result<actix_web::web::Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.heartbeat.poll_tick(cx).is_ready() {
            let payload = format!(
                "event: heartbeat\ndata: {{\"timestamp\":\"{}\"}}\n\n",
                Utc::now().to_rfc3339()
            );
            return Poll::Ready(Some(Ok(actix_web::web::Bytes::from(payload))));
        }

        match self.receiver.try_recv() {
            Ok(message) => {
                let event = message.event.unwrap_or(message.topic);
                let data: &str = message.serialized_data.as_ref();
                let payload = format!("event: {event}\ndata: {data}\n\n");
                Poll::Ready(Some(Ok(actix_web::web::Bytes::from(payload))))
            }
            Err(broadcast::error::TryRecvError::Empty)
            | Err(broadcast::error::TryRecvError::Lagged(_))
            | Err(broadcast::error::TryRecvError::Closed) => Poll::Pending,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SnapshotQuery {
    time_range: Option<String>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
struct TrendQuery {
    metric: Option<String>,
    days: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ServiceNodesQuery {
    #[serde(rename = "date")]
    target_date: NaiveDate,
}

#[derive(Debug, Deserialize)]
struct CompareQuery {
    base_start_date: NaiveDate,
    base_end_date: NaiveDate,
    compare_start_date: NaiveDate,
    compare_end_date: NaiveDate,
}

#[derive(Debug, Deserialize)]
struct BaselineCompareQuery {
    #[serde(rename = "date")]
    target_date: NaiveDate,
    weather: Option<String>,
}

fn normalize_snapshot_time_range(value: Option<&str>) -> Result<&str, ApiError> {
    match value.unwrap_or("today") {
        "today" | "this_week" | "this_month" | "custom" => Ok(value.unwrap_or("today")),
        _ => Err(ApiError::ValidationError(
            "time_range must be one of today, this_week, this_month, custom".into(),
        )),
    }
}

fn normalize_metric(value: Option<&str>) -> Result<&str, ApiError> {
    match value.unwrap_or("on_time_rate") {
        "on_time_rate" | "turnaround" | "abnormal_ratio" => Ok(value.unwrap_or("on_time_rate")),
        _ => Err(ApiError::ValidationError(
            "metric must be one of on_time_rate, turnaround, abnormal_ratio".into(),
        )),
    }
}

fn normalize_days(value: Option<i32>) -> Result<i32, ApiError> {
    let days = value.unwrap_or(7);
    if (1..=90).contains(&days) {
        Ok(days)
    } else {
        Err(ApiError::ValidationError("days must be between 1 and 90".into()))
    }
}

async fn get_snapshot(
    svc: web::Data<Arc<KpiAggregationService>>,
    query: web::Query<SnapshotQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_permission("flight:read")?;

    let time_range = normalize_snapshot_time_range(query.time_range.as_deref())?;
    let snapshot = svc
        .get_kpi_snapshot(time_range, query.start_date, query.end_date)
        .await?;
    Ok(ok_resp(snapshot, "KPI snapshot fetched"))
}

async fn get_trend(
    svc: web::Data<Arc<KpiAggregationService>>,
    query: web::Query<TrendQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_permission("flight:read")?;

    let metric = normalize_metric(query.metric.as_deref())?;
    let days = normalize_days(query.days)?;
    let items = svc.get_kpi_trend(metric, days).await?;

    Ok(HttpResponse::Ok().json(json!({
        "metric": metric,
        "days": days,
        "items": items,
    })))
}

async fn get_service_nodes(
    svc: web::Data<Arc<KpiAggregationService>>,
    query: web::Query<ServiceNodesQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_permission("flight:read")?;

    let items = svc.get_service_node_compliance(query.target_date).await?;
    Ok(HttpResponse::Ok().json(json!({
        "date": query.target_date.format("%Y-%m-%d").to_string(),
        "items": items,
    })))
}

async fn compare_kpi(
    svc: web::Data<Arc<KpiAggregationService>>,
    query: web::Query<CompareQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_permission("flight:read")?;

    if query.base_start_date > query.base_end_date {
        return Err(ApiError::BadRequest("base_start_date must be <= base_end_date".into()));
    }
    if query.compare_start_date > query.compare_end_date {
        return Err(ApiError::BadRequest(
            "compare_start_date must be <= compare_end_date".into(),
        ));
    }

    let payload = svc
        .compare_kpi(
            (query.base_start_date, query.base_end_date),
            (query.compare_start_date, query.compare_end_date),
        )
        .await?;
    Ok(ok_resp(payload, "KPI comparison fetched"))
}

async fn get_trend_with_anomalies(
    svc: web::Data<Arc<KpiAggregationService>>,
    query: web::Query<TrendQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_permission("flight:read")?;

    let metric = normalize_metric(query.metric.as_deref())?;
    let days = normalize_days(query.days)?;
    let payload = svc.get_trend_with_anomaly_overlay(metric, days).await?;
    Ok(ok_resp(payload, "KPI trend with anomaly overlay fetched"))
}

async fn baseline_compare(
    svc: web::Data<Arc<KpiAggregationService>>,
    query: web::Query<BaselineCompareQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_permission("flight:read")?;

    let payload = svc
        .get_baseline_compare(query.target_date, query.weather.as_deref().unwrap_or("normal"))
        .await?;
    Ok(ok_resp(payload, "KPI baseline compare fetched"))
}

#[allow(dead_code)]
async fn kpi_stream(
    svc: web::Data<Arc<KpiAggregationService>>,
    hub: web::Data<Arc<SseHub>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_authenticated()?;
    claims.ensure_permission("flight:read")?;

    let snapshot = svc.get_kpi_snapshot("today", None, None).await?;
    let initial_payload = format!(
        "event: initial\ndata: {}\n\n",
        json!({
            "type": "initial_data",
            "snapshot": snapshot,
            "timestamp": Utc::now().to_rfc3339(),
        })
    );
    let stream = futures_util::stream::iter(vec![Ok::<actix_web::web::Bytes, actix_web::Error>(
        actix_web::web::Bytes::from(initial_payload),
    )])
    .chain(KpiSseStream {
        receiver: hub.subscribe("kpi_updated").await,
        heartbeat: tokio::time::interval(Duration::from_secs(15)),
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/kpi")
            .route("/snapshot", web::get().to(get_snapshot))
            .route("/trend", web::get().to(get_trend))
            .route("/service-nodes", web::get().to(get_service_nodes))
            .route("/compare", web::get().to(compare_kpi))
            .route("/trend-with-anomalies", web::get().to(get_trend_with_anomalies))
            .route("/baseline-compare", web::get().to(baseline_compare)),
    );
}
