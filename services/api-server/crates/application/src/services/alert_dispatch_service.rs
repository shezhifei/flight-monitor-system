use std::sync::Arc;
#[allow(unused_imports)]
use std::time::Duration;

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde_json::{json, Value};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy)]
enum AlertLevel {
    Info,
    Warning,
    Error,
    Critical,
}

impl AlertLevel {
    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "info" => Self::Info,
            "error" => Self::Error,
            "critical" => Self::Critical,
            _ => Self::Warning,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone)]
struct EmailDeliveryConfig {
    enabled: bool,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    from_address: String,
    timeout_seconds: u64,
}

impl EmailDeliveryConfig {
    fn from_env() -> Self {
        Self {
            enabled: env_bool("SMTP_ENABLED", false),
            host: std::env::var("SMTP_HOST")
                .ok()
                .map(|value| value.trim().to_string())
                .unwrap_or_default(),
            port: std::env::var("SMTP_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(587),
            username: std::env::var("SMTP_USERNAME")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            password: std::env::var("SMTP_PASSWORD")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            from_address: std::env::var("SMTP_FROM_ADDRESS")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "no-reply@localhost".to_string()),
            timeout_seconds: std::env::var("SMTP_TIMEOUT_SECONDS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(10),
        }
    }
}

pub struct AlertDispatchService {
    http_client: reqwest::Client,
}

impl AlertDispatchService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            http_client: crate::http_client::shared_http_client(),
        })
    }

    pub async fn dispatch_test_alert(
        &self,
        title: &str,
        message: &str,
        level: &str,
        channels: &[String],
        recipients: &[String],
        requested_by: &str,
    ) {
        let alert_level = AlertLevel::parse(level);
        let metadata = json!({
            "type": "manual_test_alert",
            "requested_by": requested_by,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let webhook_payload = json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "title": title,
            "message": message,
            "level": alert_level.as_str(),
            "metadata": metadata,
        });

        for channel in channels {
            match channel.trim().to_ascii_lowercase().as_str() {
                "log" => self.dispatch_log(title, message, alert_level),
                "webhook" => {
                    self.dispatch_webhook(&webhook_payload).await;
                }
                "email" => {
                    self.dispatch_email(title, message, alert_level, recipients).await;
                }
                unknown if !unknown.is_empty() => {
                    warn!(channel = unknown, "unsupported alert channel");
                }
                _ => {}
            }
        }
    }

    fn dispatch_log(&self, title: &str, message: &str, level: AlertLevel) {
        let body = format!(
            "[ALERT][{}] {}: {}",
            level.as_str().to_ascii_uppercase(),
            title,
            message
        );
        match level {
            AlertLevel::Info => info!(alert.title = title, alert.message = message, "{body}"),
            AlertLevel::Warning => warn!(alert.title = title, alert.message = message, "{body}"),
            AlertLevel::Error | AlertLevel::Critical => {
                error!(alert.title = title, alert.message = message, "{body}")
            }
        }
    }

    async fn dispatch_webhook(&self, payload: &Value) {
        let Some(url) = std::env::var("ALERT_WEBHOOK_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            warn!("alert webhook channel requested but ALERT_WEBHOOK_URL is not configured");
            return;
        };

        match self.http_client.post(url).json(payload).send().await {
            Ok(response) if response.status().is_success() => {
                info!("alert webhook delivered");
            }
            Ok(response) => {
                warn!(status = %response.status(), "alert webhook returned non-success status");
            }
            Err(error) => {
                error!(error = %error, "alert webhook delivery failed");
            }
        }
    }

    async fn dispatch_email(&self, title: &str, message: &str, level: AlertLevel, recipients_override: &[String]) {
        let config = EmailDeliveryConfig::from_env();
        if !config.enabled {
            warn!("alert email channel requested but SMTP is disabled");
            return;
        }
        if config.host.trim().is_empty() {
            warn!("alert email channel requested but SMTP_HOST is empty");
            return;
        }

        let recipients = resolve_email_recipients(recipients_override);
        if recipients.is_empty() {
            warn!("alert email channel requested but no recipients are configured");
            return;
        }

        let subject = format!(
            "[Flight Monitor][{}] {}",
            level.as_str().to_ascii_uppercase(),
            title.trim()
        );
        let body = format!(
            "告警时间: {}\n告警级别: {}\n告警标题: {}\n告警内容: {}\n",
            chrono::Utc::now().to_rfc3339(),
            level.as_str(),
            title.trim(),
            message.trim(),
        );

        let send_result =
            tokio::task::spawn_blocking(move || send_email_blocking(&config, &subject, &body, &recipients)).await;

        match send_result {
            Ok(Ok(())) => info!("alert email delivered"),
            Ok(Err(error)) => warn!(error = %error, "alert email delivery failed"),
            Err(error) => warn!(error = %error, "alert email task join failed"),
        }
    }
}

fn send_email_blocking(
    config: &EmailDeliveryConfig,
    subject: &str,
    body: &str,
    recipients: &[String],
) -> Result<(), String> {
    let from: Mailbox = config
        .from_address
        .parse()
        .map_err(|error| format!("invalid SMTP_FROM_ADDRESS: {error}"))?;

    let mut builder = Message::builder().from(from).subject(subject);
    for recipient in recipients {
        let mailbox: Mailbox = recipient
            .parse()
            .map_err(|error| format!("invalid recipient address {recipient}: {error}"))?;
        builder = builder.to(mailbox);
    }
    let email = builder
        .body(body.to_string())
        .map_err(|error| format!("failed to build email message: {error}"))?;

    let mut transport_builder = SmtpTransport::builder_dangerous(&config.host)
        .port(config.port)
        .timeout(Some(Duration::from_secs(config.timeout_seconds)));
    if let Some(username) = &config.username {
        transport_builder = transport_builder.credentials(Credentials::new(
            username.clone(),
            config.password.as_deref().unwrap_or_default().to_owned(),
        ));
    }
    let transport = transport_builder.build();
    transport
        .send(&email)
        .map(|_| ())
        .map_err(|error| format!("smtp send failed: {error}"))
}

fn resolve_email_recipients(recipients_override: &[String]) -> Vec<String> {
    let explicit = recipients_override
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if !explicit.is_empty() {
        return explicit;
    }

    std::env::var("ALERT_EMAIL_TO")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}
