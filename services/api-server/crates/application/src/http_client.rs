use std::sync::OnceLock;
use std::time::Duration;

fn env_usize(keys: &[&str], default: usize) -> usize {
    keys.iter()
        .find_map(|key| std::env::var(key).ok().and_then(|value| value.parse::<usize>().ok()))
        .unwrap_or(default)
}

fn env_u64(keys: &[&str], default: u64) -> u64 {
    keys.iter()
        .find_map(|key| std::env::var(key).ok().and_then(|value| value.parse::<u64>().ok()))
        .unwrap_or(default)
}

fn build_pooled_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(env_usize(&["REQWEST_POOL_MAX_IDLE_PER_HOST"], 20))
        .pool_idle_timeout(Duration::from_secs(env_u64(&["REQWEST_POOL_IDLE_TIMEOUT_SECS"], 90)))
        .tcp_keepalive(Duration::from_secs(60))
        .build()
        .expect("failed to build shared reqwest client")
}

pub fn shared_http_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(build_pooled_client).clone()
}
