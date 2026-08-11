use async_trait::async_trait;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;

pub use fms_domain::ports::message_queue::{
    MessageHandler, MessageQueue, MessageQueueError, PublishMessage, PushConsumer, ReceiveMessages, ReceivedMessage,
    SubscriberMessage,
};

pub mod memory_push_consumer;
pub use memory_push_consumer::MemoryPushConsumer;

pub mod rocketmq_push_consumer;
pub use rocketmq_push_consumer::RocketMqPushConsumer;

#[derive(Debug, Deserialize)]
struct PublishResponse {
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct ReceiveResponse {
    messages: Vec<ReceivedMessage>,
}

#[derive(Debug, Clone)]
pub struct MessageQueueRetryPolicy {
    max_attempts: usize,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl MessageQueueRetryPolicy {
    pub fn new(max_attempts: usize, initial_backoff: Duration, max_backoff: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            initial_backoff,
            max_backoff,
        }
    }

    fn delay_after_attempt(&self, failed_attempts: usize) -> Duration {
        let exponent = failed_attempts.saturating_sub(1).min(31) as u32;
        let multiplier = 1_u32.checked_shl(exponent).unwrap_or(u32::MAX);
        self.initial_backoff
            .checked_mul(multiplier)
            .unwrap_or(self.max_backoff)
            .min(self.max_backoff)
    }
}

impl Default for MessageQueueRetryPolicy {
    fn default() -> Self {
        Self::new(3, Duration::from_millis(50), Duration::from_millis(500))
    }
}

#[async_trait]
trait RetrySleeper: Send + Sync {
    async fn sleep(&self, duration: Duration);
}

struct TokioRetrySleeper;

#[async_trait]
impl RetrySleeper for TokioRetrySleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[derive(Clone)]
pub struct MessageQueueGatewayClient {
    base_url: String,
    http: reqwest::Client,
    retry_policy: MessageQueueRetryPolicy,
    retry_sleeper: Arc<dyn RetrySleeper>,
}

impl fmt::Debug for MessageQueueGatewayClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MessageQueueGatewayClient")
            .field("base_url", &self.base_url)
            .field("http", &self.http)
            .field("retry_policy", &self.retry_policy)
            .finish_non_exhaustive()
    }
}

impl MessageQueueGatewayClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = crate::http_client::mq_http_client();
        Self {
            base_url: trim_trailing_slash(base_url.into()),
            http,
            retry_policy: MessageQueueRetryPolicy::default(),
            retry_sleeper: Arc::new(TokioRetrySleeper),
        }
    }

    pub fn with_retry_policy(mut self, retry_policy: MessageQueueRetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    #[cfg(test)]
    fn with_retry_sleeper(mut self, retry_sleeper: Arc<dyn RetrySleeper>) -> Self {
        self.retry_sleeper = retry_sleeper;
        self
    }

    async fn send_with_retry(
        &self,
        mut build_request: impl FnMut() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, MessageQueueError> {
        let mut attempt = 1;

        loop {
            match build_request().send().await {
                Ok(response) if is_retryable_status(response.status()) && attempt < self.retry_policy.max_attempts => {
                    let _ = response_text(response).await;
                    self.sleep_before_retry(attempt).await;
                    attempt += 1;
                }
                Ok(response) => return Ok(response),
                Err(error) if is_retryable_transport_error(&error) && attempt < self.retry_policy.max_attempts => {
                    self.sleep_before_retry(attempt).await;
                    attempt += 1;
                }
                Err(error) => {
                    return Err(MessageQueueError::Unavailable(error.to_string()));
                }
            }
        }
    }

    async fn sleep_before_retry(&self, failed_attempts: usize) {
        let delay = self.retry_policy.delay_after_attempt(failed_attempts);
        if !delay.is_zero() {
            self.retry_sleeper.sleep(delay).await;
        }
    }
}

#[async_trait]
impl MessageQueue for MessageQueueGatewayClient {
    async fn publish(&self, message: PublishMessage) -> Result<String, MessageQueueError> {
        let url = format!("{}/messages/publish", self.base_url);
        let topic = message.topic.clone();
        let result = self
            .send_with_retry(|| self.http.post(url.clone()).json(&message))
            .await;
        let status = if result.is_ok() { "success" } else { "error" };
        metrics::counter!(
            "fms_mq_publish_total",
            "topic" => topic,
            "status" => status
        )
        .increment(1);
        let response = result?;
        let response = expect_success(response).await?;
        let body = response
            .json::<PublishResponse>()
            .await
            .map_err(|error| MessageQueueError::Gateway(error.to_string()))?;
        Ok(body.message_id)
    }

    async fn receive(&self, request: ReceiveMessages) -> Result<Vec<ReceivedMessage>, MessageQueueError> {
        // Per-request timeout overrides the default 10s client timeout so that
        // long-polling consumers can block for up to wait_ms + 5s margin.
        let wait_ms = request.wait_ms.unwrap_or(200);
        let timeout = Duration::from_millis(wait_ms.saturating_add(5000).min(60000));
        let url = format!("{}/messages/receive", self.base_url);
        let response = self
            .send_with_retry(|| self.http.post(url.clone()).json(&request).timeout(timeout))
            .await?;
        let response = expect_success(response).await?;
        let body = response
            .json::<ReceiveResponse>()
            .await
            .map_err(|error| MessageQueueError::Gateway(error.to_string()))?;
        Ok(body.messages)
    }

    async fn ack(&self, receipt_handle: &str) -> Result<(), MessageQueueError> {
        let url = format!("{}/messages/ack", self.base_url);
        let response = self
            .send_with_retry(|| {
                self.http
                    .post(url.clone())
                    .json(&serde_json::json!({ "receipt_handle": receipt_handle }))
            })
            .await?;

        match response.status() {
            StatusCode::NO_CONTENT | StatusCode::OK => Ok(()),
            StatusCode::NOT_FOUND => Err(MessageQueueError::UnknownReceipt(response_text(response).await)),
            StatusCode::BAD_REQUEST => Err(MessageQueueError::BadRequest(response_text(response).await)),
            StatusCode::SERVICE_UNAVAILABLE => Err(MessageQueueError::Unavailable(response_text(response).await)),
            _ => Err(MessageQueueError::Gateway(response_text(response).await)),
        }
    }
}

fn is_retryable_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn is_retryable_transport_error(error: &reqwest::Error) -> bool {
    !error.is_builder() && !error.is_request()
}

async fn expect_success(response: reqwest::Response) -> Result<reqwest::Response, MessageQueueError> {
    match response.status() {
        StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED => Ok(response),
        StatusCode::BAD_REQUEST => Err(MessageQueueError::BadRequest(response_text(response).await)),
        StatusCode::NOT_FOUND => Err(MessageQueueError::UnknownReceipt(response_text(response).await)),
        StatusCode::SERVICE_UNAVAILABLE => Err(MessageQueueError::Unavailable(response_text(response).await)),
        _ => Err(MessageQueueError::Gateway(response_text(response).await)),
    }
}

async fn response_text(response: reqwest::Response) -> String {
    response.text().await.unwrap_or_else(|error| error.to_string())
}

fn trim_trailing_slash(value: String) -> String {
    value.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct RecordingSleeper {
        delays: Arc<Mutex<Vec<Duration>>>,
    }

    #[async_trait]
    impl RetrySleeper for RecordingSleeper {
        async fn sleep(&self, duration: Duration) {
            self.delays
                .lock()
                .expect("RecordingSleeper: delays lock poisoned")
                .push(duration);
        }
    }

    enum StubAction {
        Respond { status: StatusCode, body: &'static str },
    }

    struct StubGateway {
        base_url: String,
        attempts: Arc<AtomicUsize>,
        paths: Arc<Mutex<Vec<String>>>,
    }

    impl StubGateway {
        fn attempts(&self) -> usize {
            self.attempts.load(Ordering::SeqCst)
        }

        fn paths(&self) -> Vec<String> {
            self.paths.lock().expect("StubGateway: paths lock poisoned").clone()
        }
    }

    fn retrying_client(base_url: String, delays: Arc<Mutex<Vec<Duration>>>) -> MessageQueueGatewayClient {
        MessageQueueGatewayClient::new(base_url)
            .with_retry_policy(MessageQueueRetryPolicy::new(
                3,
                Duration::from_millis(10),
                Duration::from_millis(100),
            ))
            .with_retry_sleeper(Arc::new(RecordingSleeper { delays }))
    }

    fn sample_publish_message() -> PublishMessage {
        PublishMessage {
            topic: "flight.events".to_string(),
            tag: Some("created".to_string()),
            key: Some("flight-1".to_string()),
            body: serde_json::json!({ "flight_id": "flight-1" }),
            properties: BTreeMap::new(),
        }
    }

    fn sample_receive_request() -> ReceiveMessages {
        ReceiveMessages {
            topic: "flight.events".to_string(),
            consumer_group: "dispatch".to_string(),
            filter_tag: None,
            batch_size: Some(1),
            wait_ms: Some(1),
        }
    }

    #[tokio::test]
    async fn publish_retries_transient_gateway_failures_with_exponential_backoff() {
        let gateway = start_stub_gateway(vec![
            StubAction::Respond {
                status: StatusCode::SERVICE_UNAVAILABLE,
                body: "warming up",
            },
            StubAction::Respond {
                status: StatusCode::BAD_GATEWAY,
                body: "proxy reset",
            },
            StubAction::Respond {
                status: StatusCode::CREATED,
                body: r#"{"message_id":"msg-3"}"#,
            },
        ])
        .await;
        let delays = Arc::new(Mutex::new(Vec::new()));
        let client = retrying_client(gateway.base_url.clone(), delays.clone());

        let message_id = client
            .publish(sample_publish_message())
            .await
            .expect("publish should succeed after transient failures");

        assert_eq!(message_id, "msg-3");
        assert_eq!(gateway.attempts(), 3);
        assert_eq!(
            gateway.paths(),
            vec![
                "/messages/publish".to_string(),
                "/messages/publish".to_string(),
                "/messages/publish".to_string(),
            ]
        );
        assert_eq!(
            *delays.lock().expect("test delays lock poisoned"),
            vec![Duration::from_millis(10), Duration::from_millis(20)]
        );
    }

    #[tokio::test]
    async fn receive_retries_transient_gateway_failure() {
        let gateway = start_stub_gateway(vec![
            StubAction::Respond {
                status: StatusCode::SERVICE_UNAVAILABLE,
                body: "not ready",
            },
            StubAction::Respond {
                status: StatusCode::OK,
                body: r#"{"messages":[{"receipt_handle":"rh-1","message_id":"msg-1","topic":"flight.events","tag":null,"key":null,"body":{"ok":true},"properties":{}}]}"#,
            },
        ])
        .await;
        let delays = Arc::new(Mutex::new(Vec::new()));
        let client = retrying_client(gateway.base_url.clone(), delays.clone());

        let messages = client
            .receive(sample_receive_request())
            .await
            .expect("receive should succeed after transient failure");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].receipt_handle, "rh-1");
        assert_eq!(gateway.attempts(), 2);
        assert_eq!(
            *delays.lock().expect("test delays lock poisoned"),
            vec![Duration::from_millis(10)]
        );
    }

    #[tokio::test]
    async fn ack_retries_transient_gateway_failure() {
        let gateway = start_stub_gateway(vec![
            StubAction::Respond {
                status: StatusCode::GATEWAY_TIMEOUT,
                body: "timeout",
            },
            StubAction::Respond {
                status: StatusCode::NO_CONTENT,
                body: "",
            },
        ])
        .await;
        let delays = Arc::new(Mutex::new(Vec::new()));
        let client = retrying_client(gateway.base_url.clone(), delays.clone());

        client
            .ack("rh-1")
            .await
            .expect("ack should succeed after transient failure");

        assert_eq!(gateway.attempts(), 2);
        assert_eq!(
            *delays.lock().expect("test delays lock poisoned"),
            vec![Duration::from_millis(10)]
        );
    }

    #[tokio::test]
    async fn publish_does_not_retry_non_retryable_bad_request() {
        let gateway = start_stub_gateway(vec![
            StubAction::Respond {
                status: StatusCode::BAD_REQUEST,
                body: "invalid topic",
            },
            StubAction::Respond {
                status: StatusCode::CREATED,
                body: r#"{"message_id":"should-not-be-used"}"#,
            },
        ])
        .await;
        let delays = Arc::new(Mutex::new(Vec::new()));
        let client = retrying_client(gateway.base_url.clone(), delays.clone());

        let error = client
            .publish(sample_publish_message())
            .await
            .expect_err("bad request must not be retried into success");

        assert!(matches!(error, MessageQueueError::BadRequest(_)));
        assert_eq!(gateway.attempts(), 1);
        assert!(delays.lock().expect("test delays lock poisoned").is_empty());
    }

    async fn start_stub_gateway(actions: Vec<StubAction>) -> StubGateway {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test gateway");
        let addr = listener.local_addr().expect("read test gateway addr");
        let attempts = Arc::new(AtomicUsize::new(0));
        let paths = Arc::new(Mutex::new(Vec::new()));
        let actions = Arc::new(Mutex::new(VecDeque::from(actions)));

        let server_attempts = attempts.clone();
        let server_paths = paths.clone();
        let server_actions = actions.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _peer)) = listener.accept().await else {
                    break;
                };
                let action = server_actions
                    .lock()
                    .expect("StubGateway: actions lock poisoned")
                    .pop_front();
                let paths = server_paths.clone();
                let attempts = server_attempts.clone();
                tokio::spawn(async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    let path = read_request_path(&mut stream).await;
                    paths.lock().expect("StubGateway: paths lock poisoned").push(path);

                    let Some(action) = action else {
                        write_response(&mut stream, StatusCode::INTERNAL_SERVER_ERROR, "unexpected request").await;
                        return;
                    };

                    match action {
                        StubAction::Respond { status, body } => {
                            write_response(&mut stream, status, body).await;
                        }
                    }
                });
            }
        });

        StubGateway {
            base_url: format!("http://{addr}"),
            attempts,
            paths,
        }
    }

    async fn read_request_path(stream: &mut tokio::net::TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = stream.read(&mut chunk).await.expect("read test request");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }

        let request = String::from_utf8_lossy(&buffer);
        request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("")
            .to_string()
    }

    async fn write_response(stream: &mut tokio::net::TcpStream, status: StatusCode, body: &str) {
        let response = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n{}",
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write test response");
    }
}
