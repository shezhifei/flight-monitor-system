use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct RedisLatencySnapshot {
    pub connected: bool,
    pub latency_ms: f64,
}

pub async fn measure_redis_latency_from_env() -> RedisLatencySnapshot {
    let Some(redis_url) = std::env::var("REDIS_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return disconnected();
    };

    let Ok(client) = redis::Client::open(redis_url) else {
        return disconnected();
    };

    let start = Instant::now();
    let Ok(mut connection) = client.get_multiplexed_tokio_connection().await else {
        return disconnected();
    };

    if !redis::cmd("PING").query_async::<String>(&mut connection).await.is_ok() {
        return disconnected();
    }

    RedisLatencySnapshot {
        connected: true,
        latency_ms: round_to_2(start.elapsed().as_secs_f64() * 1000.0),
    }
}

fn disconnected() -> RedisLatencySnapshot {
    RedisLatencySnapshot {
        connected: false,
        latency_ms: -1.0,
    }
}

fn round_to_2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
