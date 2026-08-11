use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::ports::runtime_diagnostic_repository::RuntimeDiagnosticRepository;

const SHADOW_DIFF_TOPIC: &str = "shadow_compare";
const WRITE_DIFF_TOPIC: &str = "write_verification";

pub struct RuntimeDiagnosticsService<R: RuntimeDiagnosticRepository + ?Sized> {
    repo: Arc<R>,
}

impl<R: RuntimeDiagnosticRepository + ?Sized> RuntimeDiagnosticsService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    pub async fn recent_shadow_diffs(
        &self,
        count: usize,
        path_prefix: Option<&str>,
    ) -> Result<Vec<Value>, DomainError> {
        let path_filter = path_prefix.map(str::trim).filter(|value| !value.is_empty());

        Ok(self
            .repo
            .fetch_recent(SHADOW_DIFF_TOPIC, count as i64)
            .await?
            .into_iter()
            .filter(|entry| {
                let Some(path_prefix) = path_filter else {
                    return true;
                };

                entry
                    .get("path")
                    .and_then(Value::as_str)
                    .map(|path| path.starts_with(path_prefix))
                    .unwrap_or(false)
            })
            .collect())
    }

    pub async fn shadow_stats(&self, minutes: i64) -> Result<Value, DomainError> {
        let cutoff_ts = (chrono::Utc::now().timestamp() - minutes * 60) as f64;
        let records = self.repo.fetch_recent(SHADOW_DIFF_TOPIC, 10_000).await?;
        Ok(Self::build_shadow_stats(records, minutes, cutoff_ts))
    }

    pub async fn shadow_diff_event_count(&self) -> Result<i64, DomainError> {
        self.repo.count_by_topic(SHADOW_DIFF_TOPIC).await
    }

    pub async fn verification_stats(&self) -> Result<Value, DomainError> {
        let records = self.repo.fetch_recent(WRITE_DIFF_TOPIC, 1000).await?;
        Ok(Self::build_verification_stats(records))
    }

    pub async fn diagnostics_connected(&self) -> bool {
        self.repo.ping().await.unwrap_or(false)
    }

    pub fn build_shadow_stats(records: Vec<Value>, minutes: i64, cutoff_ts: f64) -> Value {
        let recent = records
            .into_iter()
            .filter(|entry| entry.get("ts").and_then(Value::as_f64).unwrap_or(0.0) >= cutoff_ts)
            .collect::<Vec<_>>();

        let total = recent.len();
        let mismatches = recent
            .iter()
            .filter(|entry| !entry.get("match").and_then(Value::as_bool).unwrap_or(true))
            .collect::<Vec<_>>();
        let match_rate = if total > 0 {
            ((total - mismatches.len()) as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        let mut by_path: BTreeMap<String, (usize, BTreeSet<String>, BTreeSet<String>, BTreeSet<String>)> =
            BTreeMap::new();
        for entry in &mismatches {
            let path = entry
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let bucket = by_path
                .entry(path)
                .or_insert_with(|| (0, BTreeSet::new(), BTreeSet::new(), BTreeSet::new()));
            bucket.0 += 1;

            if let Some(values) = entry.get("missing_in_rust").and_then(Value::as_array) {
                for value in values.iter().filter_map(Value::as_str) {
                    bucket.1.insert(value.to_string());
                }
            }

            if let Some(values) = entry.get("extra_in_rust").and_then(Value::as_array) {
                for value in values.iter().filter_map(Value::as_str) {
                    bucket.2.insert(value.to_string());
                }
            }

            if let Some(values) = entry.get("value_diffs").and_then(Value::as_object) {
                for key in values.keys() {
                    bucket.3.insert(key.clone());
                }
            }
        }

        let by_path_serializable = by_path
            .into_iter()
            .map(|(path, (count, missing, extra, diff_keys))| {
                (
                    path,
                    json!({
                        "count": count,
                        "missing_in_rust": missing.into_iter().collect::<Vec<_>>(),
                        "extra_in_rust": extra.into_iter().collect::<Vec<_>>(),
                        "value_diff_keys": diff_keys.into_iter().collect::<Vec<_>>(),
                    }),
                )
            })
            .collect::<BTreeMap<_, _>>();

        json!({
            "window_minutes": minutes,
            "total_comparisons": total,
            "mismatches": mismatches.len(),
            "match_rate_percent": ((match_rate * 100.0).round() / 100.0),
            "by_path": by_path_serializable,
        })
    }

    pub fn build_verification_stats(records: Vec<Value>) -> Value {
        let mut total = 0usize;
        let mut status_mismatches = 0usize;
        let mut data_diffs = 0usize;
        let mut by_path: BTreeMap<String, BTreeMap<&'static str, usize>> = BTreeMap::new();

        for record in records {
            total += 1;
            let status_match = record.get("status_match").and_then(Value::as_bool).unwrap_or(true);
            let has_data_diff = record.get("has_data_diff").and_then(Value::as_bool).unwrap_or(false);
            let path = record
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();

            if !status_match {
                status_mismatches += 1;
            }
            if has_data_diff {
                data_diffs += 1;
            }

            const TOTAL_BUCKET: &str = "total";
            const STATUS_MISMATCHES_BUCKET: &str = "status_mismatches";
            const DATA_DIFFS_BUCKET: &str = "data_diffs";

            let bucket = by_path.entry(path).or_insert_with(|| {
                BTreeMap::from([
                    (TOTAL_BUCKET, 0usize),
                    (STATUS_MISMATCHES_BUCKET, 0usize),
                    (DATA_DIFFS_BUCKET, 0usize),
                ])
            });
            *bucket.get_mut(TOTAL_BUCKET).expect("total bucket exists") += 1;
            if !status_match {
                *bucket.get_mut(STATUS_MISMATCHES_BUCKET).expect("status bucket exists") += 1;
            }
            if has_data_diff {
                *bucket.get_mut(DATA_DIFFS_BUCKET).expect("diff bucket exists") += 1;
            }
        }

        let match_rate = if total > 0 {
            ((total - data_diffs) as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        json!({
            "total_comparisons": total,
            "status_mismatches": status_mismatches,
            "data_diffs": data_diffs,
            "match_rate_percent": ((match_rate * 100.0).round() / 100.0),
            "by_path": by_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::json;
    use serde_json::Value;

    use super::RuntimeDiagnosticsService;
    use fms_domain::error::DomainError;
    use fms_domain::ports::runtime_diagnostic_repository::RuntimeDiagnosticRepository;

    struct FakeRuntimeDiagnosticRepository;

    #[async_trait]
    impl RuntimeDiagnosticRepository for FakeRuntimeDiagnosticRepository {
        async fn fetch_recent(&self, _topic: &str, _limit: i64) -> Result<Vec<Value>, DomainError> {
            Ok(Vec::new())
        }

        async fn count_by_topic(&self, _topic: &str) -> Result<i64, DomainError> {
            Ok(0)
        }

        async fn ping(&self) -> Result<bool, DomainError> {
            Ok(true)
        }
    }

    #[test]
    fn shadow_stats_aggregate_mismatches_by_path_with_sorted_key_sets() {
        let stats = RuntimeDiagnosticsService::<FakeRuntimeDiagnosticRepository>::build_shadow_stats(
            vec![
                json!({
                    "ts": 1000.0,
                    "match": false,
                    "path": "/api/a",
                    "missing_in_rust": ["z", "a"],
                    "extra_in_rust": ["b"],
                    "value_diffs": {
                        "delta": {"python": 1, "rust": 2},
                        "alpha": {"python": 3, "rust": 4}
                    }
                }),
                json!({
                    "ts": 1001.0,
                    "match": false,
                    "path": "/api/a",
                    "missing_in_rust": ["m", "a"],
                    "extra_in_rust": ["c", "b"],
                    "value_diffs": {
                        "beta": {"python": 5, "rust": 6}
                    }
                }),
                json!({
                    "ts": 1002.0,
                    "match": false,
                    "path": "/api/b",
                    "missing_in_rust": ["x"],
                    "extra_in_rust": [],
                    "value_diffs": {}
                }),
                json!({
                    "ts": 1003.0,
                    "match": true,
                    "path": "/api/a"
                }),
                json!({
                    "ts": 100.0,
                    "match": false,
                    "path": "/api/old"
                }),
            ],
            30,
            999.0,
        );

        assert_eq!(stats["window_minutes"], json!(30));
        assert_eq!(stats["total_comparisons"], json!(4));
        assert_eq!(stats["mismatches"], json!(3));
        assert_eq!(stats["match_rate_percent"], json!(25.0));
        assert_eq!(
            stats["by_path"]["/api/a"],
            json!({
                "count": 2,
                "missing_in_rust": ["a", "m", "z"],
                "extra_in_rust": ["b", "c"],
                "value_diff_keys": ["alpha", "beta", "delta"],
            })
        );
        assert_eq!(
            stats["by_path"]["/api/b"],
            json!({
                "count": 1,
                "missing_in_rust": ["x"],
                "extra_in_rust": [],
                "value_diff_keys": [],
            })
        );
    }

    #[test]
    fn verification_stats_aggregate_status_and_data_diffs_by_path() {
        let stats = RuntimeDiagnosticsService::<FakeRuntimeDiagnosticRepository>::build_verification_stats(vec![
            json!({
                "path": "/api/a",
                "status_match": false,
                "has_data_diff": true
            }),
            json!({
                "path": "/api/a",
                "status_match": true,
                "has_data_diff": true
            }),
            json!({
                "path": "/api/b",
                "status_match": false,
                "has_data_diff": false
            }),
            json!({
                "path": "/api/b",
                "status_match": true,
                "has_data_diff": false
            }),
        ]);

        assert_eq!(stats["total_comparisons"], json!(4));
        assert_eq!(stats["status_mismatches"], json!(2));
        assert_eq!(stats["data_diffs"], json!(2));
        assert_eq!(stats["match_rate_percent"], json!(50.0));
        assert_eq!(
            stats["by_path"]["/api/a"],
            json!({
                "total": 2,
                "status_mismatches": 1,
                "data_diffs": 2,
            })
        );
        assert_eq!(
            stats["by_path"]["/api/b"],
            json!({
                "total": 2,
                "status_mismatches": 1,
                "data_diffs": 0,
            })
        );
    }
}
