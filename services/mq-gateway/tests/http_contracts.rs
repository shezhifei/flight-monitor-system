use actix_web::{http::StatusCode, test};
use fms_mq_gateway::api::{PublishRequest, ReceiveRequest};
use fms_mq_gateway::http;
use fms_mq_gateway::memory::InMemoryTransport;

#[actix_rt::test]
async fn publish_receive_ack_round_trip_over_http() {
    let transport = InMemoryTransport::default();
    let app = test::init_service(http::app_with_token(transport.clone(), None)).await;

    let publish = PublishRequest {
        topic: "fms.domain-events".to_string(),
        tag: Some("flight.status_updated_v2".to_string()),
        key: Some("evt-1".to_string()),
        body: serde_json::json!({"event_id": "evt-1"}),
        properties: Default::default(),
    };
    let req = test::TestRequest::post()
        .uri("/messages/publish")
        .set_json(&publish)
        .to_request();
    let publish_resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
    assert!(publish_resp["message_id"]
        .as_str()
        .unwrap_or("")
        .starts_with("mem-"));

    let receive = ReceiveRequest {
        topic: "fms.domain-events".to_string(),
        consumer_group: "domain_event_processors".to_string(),
        filter_tag: Some("flight.status_updated_v2".to_string()),
        batch_size: Some(10),
        wait_ms: Some(1),
    };
    let req = test::TestRequest::post()
        .uri("/messages/receive")
        .set_json(&receive)
        .to_request();
    let receive_resp: serde_json::Value = test::call_and_read_body_json(&app, req).await;
    let receipt_handle = receive_resp["messages"][0]["receipt_handle"]
        .as_str()
        .expect("received message has receipt")
        .to_string();

    let req = test::TestRequest::post()
        .uri("/messages/ack")
        .set_json(serde_json::json!({ "receipt_handle": receipt_handle }))
        .to_request();
    let ack_resp = test::call_service(&app, req).await;
    assert_eq!(ack_resp.status(), StatusCode::NO_CONTENT);
}

#[actix_rt::test]
async fn invalid_publish_returns_bad_request() {
    let app = test::init_service(http::app_with_token(InMemoryTransport::default(), None)).await;

    let req = test::TestRequest::post()
        .uri("/messages/publish")
        .set_json(serde_json::json!({
            "topic": "",
            "body": {}
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_rt::test]
async fn write_endpoints_require_token_when_configured() {
    let app = test::init_service(http::app_with_token(
        InMemoryTransport::default(),
        Some("internal-token".to_string()),
    ))
    .await;

    let req = test::TestRequest::post()
        .uri("/messages/publish")
        .set_json(serde_json::json!({
            "topic": "fms.domain-events",
            "body": {}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let req = test::TestRequest::post()
        .uri("/messages/receive")
        .insert_header(("Authorization", "Bearer internal-token"))
        .set_json(serde_json::json!({
            "topic": "fms.domain-events",
            "consumer_group": "domain_event_processors"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_rt::test]
async fn write_endpoints_accept_x_token_header() {
    let app = test::init_service(http::app_with_token(
        InMemoryTransport::default(),
        Some("internal-token".to_string()),
    ))
    .await;

    let req = test::TestRequest::post()
        .uri("/messages/publish")
        .insert_header(("x-mq-gateway-token", "internal-token"))
        .set_json(serde_json::json!({
            "topic": "fms.domain-events",
            "body": {}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_rt::test]
async fn production_without_token_rejects_write_endpoints() {
    let app = test::init_service(http::app_with_token_and_env(
        InMemoryTransport::default(),
        None,
        true,
    ))
    .await;

    for uri in ["/messages/publish", "/messages/receive", "/messages/ack"] {
        let req = test::TestRequest::post()
            .uri(uri)
            .set_json(serde_json::json!({
                "topic": "fms.domain-events",
                "body": {},
                "consumer_group": "cg",
                "receipt_handle": "rh",
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{} should be rejected in production without token",
            uri
        );
    }
}

#[actix_rt::test]
async fn production_without_token_allows_health() {
    let app = test::init_service(http::app_with_token_and_env(
        InMemoryTransport::default(),
        None,
        true,
    ))
    .await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_rt::test]
async fn production_with_valid_token_allows_access() {
    let app = test::init_service(http::app_with_token_and_env(
        InMemoryTransport::default(),
        Some("prod-token".to_string()),
        true,
    ))
    .await;

    let req = test::TestRequest::post()
        .uri("/messages/publish")
        .insert_header(("x-mq-gateway-token", "prod-token"))
        .set_json(serde_json::json!({
            "topic": "fms.domain-events",
            "body": {}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_rt::test]
async fn dev_without_token_allows_write_endpoints() {
    let app = test::init_service(http::app_with_token_and_env(
        InMemoryTransport::default(),
        None,
        false,
    ))
    .await;

    let req = test::TestRequest::post()
        .uri("/messages/publish")
        .set_json(serde_json::json!({
            "topic": "fms.domain-events",
            "body": {}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
