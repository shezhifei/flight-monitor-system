//! KPI Aggregation - High-performance baseline and comparison computations
//!
//! This module offloads CPU-bound KPI operations from Python to Rust.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;

/// Generate a baseline profile for a given weather category.
///
/// Returns a dict mapping hour strings ("00:00", "01:00", ...) to dicts with
/// volume, on_time_rate, and threshold_margin.
#[pyfunction]
pub fn generate_baseline_profile<'py>(py: Python<'py>, weather_category: &str) -> PyResult<Bound<'py, PyDict>> {
    let base_curve: [(u8, i32, f64); 24] = [
        (0, 5, 0.95), (1, 3, 0.95), (2, 2, 0.95), (3, 2, 0.95),
        (4, 5, 0.95), (5, 10, 0.92), (6, 25, 0.90), (7, 40, 0.88),
        (8, 45, 0.85), (9, 42, 0.85), (10, 40, 0.86), (11, 45, 0.85),
        (12, 48, 0.82), (13, 50, 0.80), (14, 48, 0.82), (15, 45, 0.82),
        (16, 42, 0.84), (17, 40, 0.85), (18, 38, 0.86), (19, 35, 0.88),
        (20, 30, 0.90), (21, 20, 0.92), (22, 15, 0.95), (23, 8, 0.95),
    ];

    let (volume_multiplier, rate_penalty, threshold_margin) = match weather_category {
        "rain" => (0.9, 0.15, 0.10),
        "storm" => (0.6, 0.35, 0.15),
        "snow" => (0.7, 0.25, 0.12),
        _ => (1.0, 0.0, 0.05), // "normal" and everything else
    };

    let result = PyDict::new(py);
    for (hour, vol, rate) in base_curve.iter() {
        let adj_vol = (*vol as f64 * volume_multiplier) as i32;
        let adj_rate = (rate - rate_penalty).clamp(0.0, 1.0);
        let rounded_rate = (adj_rate * 100.0).round() / 100.0;

        let hour_str = format!("{:02}:00", hour);
        let entry = PyDict::new(py);
        entry.set_item("volume", adj_vol)?;
        entry.set_item("on_time_rate", rounded_rate)?;
        entry.set_item("threshold_margin", threshold_margin)?;

        result.set_item(hour_str, entry)?;
    }

    Ok(result)
}

/// Compare KPI metrics between two sets (base vs compare) and compute deltas and change rates.
///
/// Args:
///     metric_keys: List of metric key names
///     base_values: Dict mapping metric keys to float values for the base range
///     compare_values: Dict mapping metric keys to float values for the compare range
///
/// Returns:
///     Dict mapping metric keys to { base, compare, delta, change_rate }
#[pyfunction]
pub fn compare_metrics<'py>(
    py: Python<'py>,
    metric_keys: Vec<String>,
    base_values: HashMap<String, f64>,
    compare_values: HashMap<String, f64>,
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);

    for key in metric_keys.iter() {
        let base_val = base_values.get(key).copied().unwrap_or(0.0);
        let compare_val = compare_values.get(key).copied().unwrap_or(0.0);
        let delta = compare_val - base_val;
        let change_rate = if base_val.abs() > 1e-12 {
            Some(delta / base_val)
        } else {
            None
        };

        let entry = PyDict::new(py);
        entry.set_item("base", base_val)?;
        entry.set_item("compare", compare_val)?;
        entry.set_item("delta", delta)?;
        match change_rate {
            Some(cr) => entry.set_item("change_rate", cr)?,
            None => entry.set_item("change_rate", py.None())?,
        }

        result.set_item(key, entry)?;
    }

    Ok(result)
}

/// Build anomaly overlay items from trend items and an anomaly map.
///
/// Args:
///     trend_dates: List of date strings
///     trend_values: List of corresponding float values (same length as trend_dates)
///     anomaly_counts: Dict mapping date strings to anomaly counts
///
/// Returns:
///     (overlay_items, anomaly_total) where overlay_items is a list of dicts
#[pyfunction]
pub fn build_anomaly_overlay<'py>(
    py: Python<'py>,
    trend_dates: Vec<String>,
    trend_values: Vec<f64>,
    anomaly_counts: HashMap<String, i64>,
) -> PyResult<(Bound<'py, pyo3::types::PyList>, i64)> {
    use pyo3::types::PyList;

    let items = PyList::empty(py);
    let mut total: i64 = 0;

    for (i, date_str) in trend_dates.iter().enumerate() {
        let value = *trend_values.get(i).unwrap_or(&0.0);
        let count = anomaly_counts.get(date_str).copied().unwrap_or(0);
        total += count;

        let entry = PyDict::new(py);
        entry.set_item("date", date_str)?;
        entry.set_item("value", value)?;
        entry.set_item("anomaly_count", count)?;

        items.append(entry)?;
    }

    Ok((items, total))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(generate_baseline_profile, m)?)?;
    m.add_function(wrap_pyfunction!(compare_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(build_anomaly_overlay, m)?)?;
    Ok(())
}
