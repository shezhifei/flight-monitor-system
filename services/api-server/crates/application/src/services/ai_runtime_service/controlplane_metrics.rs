//! AI control-plane latency probes (Task J3).
//!
//! Records `fms_ai_controlplane_duration_seconds{op}` for the Rust-side
//! paths that sit between the sidecar and durable state — lease
//! authorization, checkpoint persistence and command enqueue — i.e.
//! everything except the LLM itself. The performance target is P99 ≤ 200ms
//! (see `docs/observability/SLO.md`); this module only makes the target
//! observable and deliberately never asserts wall-clock limits in tests.

use std::time::{Duration, Instant};

pub const METRIC_NAME: &str = "fms_ai_controlplane_duration_seconds";

/// Lease authorization path: context load + decision + lease command enqueue.
pub const OP_LEASE: &str = "lease";
/// Checkpoint persistence path (`handle_checkpoint` upsert + supersede).
pub const OP_CHECKPOINT: &str = "checkpoint";
/// Single `ai_runtime_commands` enqueue round-trip.
pub const OP_COMMAND_ENQUEUE: &str = "command_enqueue";

/// Record one control-plane duration sample for `op`.
pub fn observe_controlplane(op: &str, elapsed: Duration) {
    metrics::histogram!(METRIC_NAME, "op" => op.to_string()).record(elapsed.as_secs_f64());
}

/// Drop-guard timer: records the enclosing scope's elapsed time under
/// `METRIC_NAME` when the guard goes out of scope (early returns included).
pub struct ControlPlaneTimer {
    op: &'static str,
    started: Instant,
}

impl ControlPlaneTimer {
    pub fn new(op: &'static str) -> Self {
        Self {
            op,
            started: Instant::now(),
        }
    }
}

impl Drop for ControlPlaneTimer {
    fn drop(&mut self) {
        observe_controlplane(self.op, self.started.elapsed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
    use std::sync::{Arc, Mutex};

    struct CapturingHistogram {
        name: String,
        labels: Vec<(String, String)>,
        samples: Arc<Mutex<Vec<(String, Vec<(String, String)>, f64)>>>,
    }

    impl metrics::HistogramFn for CapturingHistogram {
        fn record(&self, value: f64) {
            self.samples
                .lock()
                .expect("sample store poisoned")
                .push((self.name.clone(), self.labels.clone(), value));
        }
    }

    /// Test recorder that routes every registered histogram through a
    /// shared capture list owned by the test.
    #[derive(Default)]
    struct HistogramCaptureRecorder {
        captures: Arc<Mutex<Vec<(String, Vec<(String, String)>, f64)>>>,
    }

    impl Recorder for HistogramCaptureRecorder {
        fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
        fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
        fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
        fn register_counter(&self, _: &Key, _: &Metadata<'_>) -> Counter {
            Counter::noop()
        }
        fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
            Gauge::noop()
        }
        fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
            let labels = key
                .labels()
                .map(|l| (l.key().to_string(), l.value().to_string()))
                .collect();
            Histogram::from_arc(Arc::new(CapturingHistogram {
                name: key.name().to_string(),
                labels,
                samples: Arc::clone(&self.captures),
            }))
        }
    }

    #[test]
    fn observe_records_labelled_samples() {
        let recorder = HistogramCaptureRecorder::default();
        let captures = recorder.captures.clone();
        metrics::with_local_recorder(&recorder, || {
            observe_controlplane(OP_LEASE, Duration::from_millis(12));
            observe_controlplane(OP_CHECKPOINT, Duration::from_millis(3));
            observe_controlplane(OP_COMMAND_ENQUEUE, Duration::from_micros(250));
        });
        let samples = captures.lock().expect("capture store poisoned");
        let find = |op: &str| {
            samples
                .iter()
                .find(|(name, labels, _)| name == METRIC_NAME && labels.iter().any(|(k, v)| k == "op" && v == op))
                .map(|(_, _, value)| *value)
        };
        assert_eq!(find(OP_LEASE), Some(0.012));
        assert_eq!(find(OP_CHECKPOINT), Some(0.003));
        assert_eq!(find(OP_COMMAND_ENQUEUE), Some(0.000_25));
    }

    #[test]
    fn drop_guard_records_when_scope_ends() {
        let recorder = HistogramCaptureRecorder::default();
        let captures = recorder.captures.clone();
        metrics::with_local_recorder(&recorder, || {
            let _probe = ControlPlaneTimer::new(OP_CHECKPOINT);
            // Elapsed time is intentionally not asserted against any SLO
            // wall-clock; only that a sample lands with a sane value.
        });
        let samples = captures.lock().expect("capture store poisoned");
        let sample = samples
            .iter()
            .find(|(name, labels, _)| {
                name == METRIC_NAME && labels.iter().any(|(k, v)| k == "op" && v == OP_CHECKPOINT)
            })
            .expect("checkpoint sample recorded on drop");
        assert!(sample.2 >= 0.0, "duration sample must be non-negative");
    }
}
