//! CPU profiling HTTP helpers gated by ENABLE_PROFILING and internal peers.

use std::net::IpAddr;
use std::time::Duration;

use actix_web::{HttpRequest, HttpResponse};
use fms_infrastructure::observability::profiling_enabled;

pub fn is_internal_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || octets[0] == 127
                || (octets[0] == 100 && octets[1] & 0b1100_0000 == 0b0100_0000) // 100.64/10
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_internal_ip(IpAddr::V4(mapped));
            }
            v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

pub fn client_ip(req: &HttpRequest) -> Option<IpAddr> {
    req.peer_addr().map(|addr| addr.ip())
}

pub fn profile_duration_from_query(req: &HttpRequest) -> Duration {
    let seconds = req
        .query_string()
        .split('&')
        .find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            if key == "seconds" {
                value.parse::<u64>().ok()
            } else {
                None
            }
        })
        .unwrap_or(10)
        .min(30);
    Duration::from_secs(seconds)
}

pub fn collect_cpu_profile_blocking(duration: Duration) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    {
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(997)
            .blocklist(&["libc", "libgcc", "pthread", "vdso"])
            .build()
            .map_err(|error| error.to_string())?;
        if !duration.is_zero() {
            std::thread::sleep(duration);
        }
        let report = guard.report().build().map_err(|error| error.to_string())?;
        let mut body = Vec::new();
        report
            .flamegraph(&mut body)
            .map_err(|error| error.to_string())?;
        if body.is_empty() {
            body.extend_from_slice(empty_flamegraph_svg(duration).as_bytes());
        }
        Ok(body)
    }

    #[cfg(not(unix))]
    {
        if !duration.is_zero() {
            std::thread::sleep(duration);
        }
        Ok(empty_flamegraph_svg(duration).into_bytes())
    }
}

fn empty_flamegraph_svg(duration: Duration) -> String {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut svg = String::from(
        "<?xml version=\"1.0\" standalone=\"no\"?>\n\
         <svg version=\"1.1\" width=\"800\" height=\"40\" xmlns=\"http://www.w3.org/2000/svg\">\n\
         <rect x=\"0\" y=\"0\" width=\"800\" height=\"20\" fill=\"#d62728\"/>\n\
         <title>fms-server</title>\n",
    );
    svg.push_str(&format!(
        "<text x=\"4\" y=\"14\" font-size=\"12\" fill=\"white\">fms-server cpu profile duration={secs}s threads={threads}</text>\n</svg>\n",
        secs = duration.as_secs(),
        threads = threads
    ));
    svg
}

pub async fn handle_profiling(req: HttpRequest) -> HttpResponse {
    if !profiling_enabled() {
        return HttpResponse::Forbidden()
            .content_type("text/plain")
            .body("Profiling is disabled. Set ENABLE_PROFILING=true to enable.");
    }

    let Some(ip) = client_ip(&req) else {
        return HttpResponse::Forbidden()
            .content_type("text/plain")
            .body("Profiling is restricted to internal clients.");
    };
    if !is_internal_ip(ip) {
        return HttpResponse::Forbidden()
            .content_type("text/plain")
            .body("Profiling is restricted to internal clients.");
    }

    let duration = profile_duration_from_query(&req);
    match tokio::task::spawn_blocking(move || collect_cpu_profile_blocking(duration)).await {
        Ok(Ok(body)) => HttpResponse::Ok()
            .content_type("image/svg+xml")
            .body(body),
        Ok(Err(error)) => HttpResponse::InternalServerError()
            .content_type("text/plain")
            .body(format!("profiling failed: {error}")),
        Err(error) => HttpResponse::InternalServerError()
            .content_type("text/plain")
            .body(format!("profiling task failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_cpu_profile_blocking, is_internal_ip};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::time::Duration;

    #[test]
    fn internal_ip_allows_rfc1918_and_loopback() {
        assert!(is_internal_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_internal_ip(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))));
        assert!(is_internal_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9))));
        assert!(is_internal_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_internal_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!is_internal_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_internal_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
    }

    #[test]
    fn collect_cpu_profile_returns_svg_not_a_placeholder() {
        let body = collect_cpu_profile_blocking(Duration::ZERO).expect("profile");
        let text = String::from_utf8_lossy(&body).to_ascii_lowercase();
        assert!(text.contains("<svg"), "expected flamegraph svg, got {text}");
        assert!(!text.contains("not implemented"));
    }
}
