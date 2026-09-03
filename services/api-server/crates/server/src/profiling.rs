//! CPU profiling HTTP helpers gated by ENABLE_PROFILING and internal peers.

use std::net::IpAddr;
use std::sync::Mutex;
use std::time::Duration;

use actix_web::{http::header, HttpRequest, HttpResponse};
use fms_infrastructure::observability::profiling_enabled;

static CPU_PROFILE_LOCK: Mutex<()> = Mutex::new(());

/// Serializes tests that collect real CPU profiles: the underlying sampler is
/// process-wide and `collect_cpu_profile_blocking` rejects concurrent runs.
#[cfg(all(test, unix))]
pub(crate) static CPU_PROFILE_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Burns CPU in a background thread so the profiler actually captures samples;
/// a sleeping process collects none and the flamegraph would come back empty.
#[cfg(all(test, unix))]
pub(crate) fn burn_cpu_for(duration: Duration) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + duration;
        let mut acc: u64 = 0;
        while std::time::Instant::now() < deadline {
            for value in 0..10_000u64 {
                acc = acc.wrapping_mul(31).wrapping_add(value);
            }
        }
        std::hint::black_box(acc);
    })
}

/// The first profile collected in a process can come back empty while the
/// sampler finishes initializing; run and discard a warm-up profile so the
/// asserted one always reflects a warmed-up sampler. Callers must hold
/// `CPU_PROFILE_TEST_LOCK`.
#[cfg(all(test, unix))]
pub(crate) fn warm_up_cpu_profiler() {
    let busy = burn_cpu_for(Duration::from_millis(400));
    let _ = collect_cpu_profile_blocking(Duration::from_millis(300));
    busy.join().expect("busy thread");
}

/// Acquires the shared profile-test lock, ignoring poisoning so one panicked
/// test does not mask the other's failure output.
#[cfg(all(test, unix))]
pub(crate) fn lock_cpu_profile_tests() -> std::sync::MutexGuard<'static, ()> {
    CPU_PROFILE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct CpuProfileArtifact {
    body: Vec<u8>,
    content_type: &'static str,
    filename: Option<&'static str>,
}

impl CpuProfileArtifact {
    #[cfg(unix)]
    fn flamegraph(body: Vec<u8>) -> Self {
        Self {
            body,
            content_type: "image/svg+xml",
            filename: None,
        }
    }

    #[cfg(windows)]
    fn windows_etl(body: Vec<u8>) -> Self {
        Self {
            body,
            content_type: "application/octet-stream",
            filename: Some("fms-server-cpu-profile.etl"),
        }
    }
}

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

pub fn collect_cpu_profile_blocking(duration: Duration) -> Result<CpuProfileArtifact, String> {
    let _profile_guard = match CPU_PROFILE_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => {
            return Err("another CPU profile is already being collected".to_string());
        }
        Err(std::sync::TryLockError::Poisoned(_)) => {
            return Err("CPU profiler lock is poisoned".to_string());
        }
    };

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
        report.flamegraph(&mut body).map_err(|error| error.to_string())?;
        if body.is_empty() {
            return Err("CPU profiler returned an empty flamegraph".to_string());
        }
        Ok(CpuProfileArtifact::flamegraph(body))
    }

    #[cfg(windows)]
    {
        collect_windows_cpu_profile(duration)
    }

    #[cfg(all(not(unix), not(windows)))]
    {
        let _ = duration;
        Err("CPU profiling is supported only on Unix and Windows".to_string())
    }
}

#[cfg(windows)]
fn collect_windows_cpu_profile(duration: Duration) -> Result<CpuProfileArtifact, String> {
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn wpr_path() -> Result<PathBuf, String> {
        let system_root =
            std::env::var_os("SystemRoot").ok_or_else(|| "SystemRoot is not set; cannot locate wpr.exe".to_string())?;
        let path = PathBuf::from(system_root).join("System32").join("wpr.exe");
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!(
                "Windows Performance Recorder was not found at {}",
                path.display()
            ))
        }
    }

    fn run_wpr<I, S>(wpr: &Path, args: I) -> Result<Output, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Command::new(wpr)
            .args(args)
            .output()
            .map_err(|error| format!("failed to execute {}: {error}", wpr.display()))
    }

    fn command_error(action: &str, output: &Output) -> String {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("exit status {}", output.status)
        };
        let normalized_detail = detail.to_ascii_lowercase();
        let permission_hint = if normalized_detail.contains("0xc5585011")
            || normalized_detail.contains("profile system performance")
        {
            " Run fms-server with elevated rights or grant its service account the 'Profile system performance' (SeSystemProfilePrivilege) user right."
        } else {
            ""
        };
        format!("WPR failed to {action}: {detail}.{permission_hint}")
    }

    let wpr = wpr_path()?;
    let sequence = SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let process_id = std::process::id();
    let session_name = format!("fms-server-{process_id}-{sequence}");
    let output_path = std::env::temp_dir().join(format!("fms-server-cpu-profile-{process_id}-{sequence}.etl"));

    let start_args = [
        OsString::from("-start"),
        OsString::from("CPU.light"),
        OsString::from("-filemode"),
        OsString::from("-instancename"),
        OsString::from(&session_name),
    ];
    let start = run_wpr(&wpr, start_args)?;
    if !start.status.success() {
        return Err(command_error("start CPU sampling", &start));
    }

    if !duration.is_zero() {
        std::thread::sleep(duration);
    }

    let stop_args = [
        OsString::from("-stop"),
        output_path.as_os_str().to_owned(),
        OsString::from("FMS server CPU profile"),
        OsString::from("-skipPdbGen"),
        OsString::from("-compress"),
        OsString::from("-instancename"),
        OsString::from(&session_name),
    ];
    let stop = match run_wpr(&wpr, stop_args) {
        Ok(output) => output,
        Err(error) => {
            let _ = run_wpr(
                &wpr,
                [
                    OsString::from("-cancel"),
                    OsString::from("-instancename"),
                    OsString::from(&session_name),
                ],
            );
            let _ = fs::remove_file(&output_path);
            return Err(error);
        }
    };
    if !stop.status.success() {
        let _ = run_wpr(
            &wpr,
            [
                OsString::from("-cancel"),
                OsString::from("-instancename"),
                OsString::from(&session_name),
            ],
        );
        let _ = fs::remove_file(&output_path);
        return Err(command_error("stop CPU sampling", &stop));
    }

    let body =
        fs::read(&output_path).map_err(|error| format!("failed to read WPR trace {}: {error}", output_path.display()));
    let cleanup = fs::remove_file(&output_path)
        .map_err(|error| format!("failed to remove WPR trace {}: {error}", output_path.display()));

    let body = match (body, cleanup) {
        (Ok(body), Ok(())) => body,
        (Err(read_error), Ok(())) => return Err(read_error),
        (Ok(_), Err(cleanup_error)) => return Err(cleanup_error),
        (Err(read_error), Err(cleanup_error)) => {
            return Err(format!("{read_error}; additionally, {cleanup_error}"));
        }
    };
    if body.is_empty() {
        return Err("WPR returned an empty ETL trace".to_string());
    }
    Ok(CpuProfileArtifact::windows_etl(body))
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
        Ok(Ok(artifact)) => {
            let mut response = HttpResponse::Ok();
            response
                .insert_header((header::CONTENT_TYPE, artifact.content_type))
                .insert_header((header::CACHE_CONTROL, "no-store"));
            if let Some(filename) = artifact.filename {
                response.insert_header((
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{filename}\""),
                ));
            }
            response.body(artifact.body)
        }
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
    use super::{is_internal_ip, profile_duration_from_query};
    use actix_web::test as actix_test;
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
    fn profile_duration_defaults_to_ten_seconds_and_caps_at_thirty() {
        let default_request = actix_test::TestRequest::default().to_http_request();
        assert_eq!(profile_duration_from_query(&default_request), Duration::from_secs(10));

        let capped_request = actix_test::TestRequest::with_uri("/?seconds=300").to_http_request();
        assert_eq!(profile_duration_from_query(&capped_request), Duration::from_secs(30));
    }

    #[cfg(unix)]
    #[test]
    fn collect_cpu_profile_returns_svg_not_a_placeholder() {
        let _serial = super::lock_cpu_profile_tests();
        super::warm_up_cpu_profiler();
        let busy = super::burn_cpu_for(Duration::from_millis(600));
        let artifact = super::collect_cpu_profile_blocking(Duration::from_millis(400)).expect("profile");
        busy.join().expect("busy thread");
        let text = String::from_utf8_lossy(&artifact.body).to_ascii_lowercase();
        assert!(text.contains("<svg"), "expected flamegraph svg, got {text}");
        assert_eq!(artifact.content_type, "image/svg+xml");
        assert!(artifact.filename.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn windows_profile_artifact_is_an_etl_download() {
        let artifact = super::CpuProfileArtifact::windows_etl(vec![1, 2, 3]);
        assert_eq!(artifact.content_type, "application/octet-stream");
        assert_eq!(artifact.filename, Some("fms-server-cpu-profile.etl"));
        assert_eq!(artifact.body, vec![1, 2, 3]);
    }
}
