//! Dispatch Calculator - High-performance distance and cost matrix computation
//!
//! This module provides parallel computation for dispatch optimization:
//! - Haversine distance calculation
//! - Batch cost matrix computation (parallel)
//! - Time conflict detection (parallel)

use pyo3::prelude::*;
use rayon::prelude::*;
use std::f64::consts::PI;

const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

/// Position with latitude and longitude
#[pyclass]
#[derive(Clone, Debug)]
pub struct Position {
    #[pyo3(get, set)]
    pub lat: f64,
    #[pyo3(get, set)]
    pub lng: f64,
}

#[pymethods]
impl Position {
    #[new]
    fn new(lat: f64, lng: f64) -> Self {
        Position { lat, lng }
    }

    fn __repr__(&self) -> String {
        format!("Position(lat={}, lng={})", self.lat, self.lng)
    }
}

/// Calculate distance between two points using Haversine formula (meters)
#[pyfunction]
pub fn haversine_distance(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let to_rad = |deg: f64| deg * PI / 180.0;

    let lat1_rad = to_rad(lat1);
    let lng1_rad = to_rad(lng1);
    let lat2_rad = to_rad(lat2);
    let lng2_rad = to_rad(lng2);

    let dlat = lat2_rad - lat1_rad;
    let dlng = lng2_rad - lng1_rad;

    let a = (dlat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (dlng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_METERS * c
}

/// Estimate travel time in minutes given distance in meters and speed in km/h
#[pyfunction]
pub fn estimate_travel_time(distance_meters: f64, speed_kmh: f64) -> f64 {
    if speed_kmh <= 0.0 {
        return f64::INFINITY;
    }
    (distance_meters / 1000.0) / speed_kmh * 60.0
}

/// Compute cost matrix in parallel
/// 
/// Args:
///     task_positions: List of (lat, lng) tuples for tasks
///     team_positions: List of (lat, lng) tuples for teams
///     speed_kmh: Travel speed in km/h
/// 
/// Returns:
///     2D list of travel times (minutes) - tasks x teams
#[pyfunction]
pub fn compute_cost_matrix_parallel(
    task_positions: Vec<(f64, f64)>,
    team_positions: Vec<(f64, f64)>,
    speed_kmh: f64,
) -> Vec<Vec<f64>> {
    task_positions
        .par_iter()
        .map(|(t_lat, t_lng)| {
            team_positions
                .iter()
                .map(|(tm_lat, tm_lng)| {
                    let distance = haversine_distance(*tm_lat, *tm_lng, *t_lat, *t_lng);
                    estimate_travel_time(distance, speed_kmh)
                })
                .collect()
        })
        .collect()
}

/// Compute time conflicts between tasks (parallel O(N^2))
/// 
/// Args:
///     starts: List of start timestamps (Unix seconds)
///     ends: List of end timestamps (Unix seconds)
/// 
/// Returns:
///     List of (i, j) tuples where task i and j have overlapping time windows
#[pyfunction]
pub fn compute_time_conflicts(starts: Vec<i64>, ends: Vec<i64>) -> Vec<(usize, usize)> {
    let n = starts.len();
    if n < 2 {
        return Vec::new();
    }

    (0..n)
        .into_par_iter()
        .flat_map(|i| {
            ((i + 1)..n)
                .filter_map(|j| {
                    // Check if time windows overlap: start_i < end_j AND start_j < end_i
                    if starts[i] < ends[j] && starts[j] < ends[i] {
                        Some((i, j))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Compute feasibility matrix (parallel)
/// 
/// Args:
///     task_required_types: List of required team type IDs for each task
///     team_type_ids: List of team type IDs
/// 
/// Returns:
///     2D list of booleans - True if team type matches task requirement
#[pyfunction]
pub fn compute_feasibility_matrix(
    task_required_types: Vec<Vec<String>>,
    team_type_ids: Vec<String>,
) -> Vec<Vec<bool>> {
    task_required_types
        .par_iter()
        .map(|required| {
            team_type_ids
                .iter()
                .map(|team_type| required.contains(team_type))
                .collect()
        })
        .collect()
}

/// Register all dispatch calculator functions to the Python module
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build_global();

    m.add_class::<Position>()?;
    m.add_function(wrap_pyfunction!(haversine_distance, m)?)?;
    m.add_function(wrap_pyfunction!(estimate_travel_time, m)?)?;
    m.add_function(wrap_pyfunction!(compute_cost_matrix_parallel, m)?)?;
    m.add_function(wrap_pyfunction!(compute_time_conflicts, m)?)?;
    m.add_function(wrap_pyfunction!(compute_feasibility_matrix, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_distance() {
        // Beijing to Shanghai: approximately 1068 km
        let lat1 = 39.9042;
        let lng1 = 116.4074;
        let lat2 = 31.2304;
        let lng2 = 121.4737;
        
        let distance = haversine_distance(lat1, lng1, lat2, lng2);
        assert!((distance - 1_068_000.0).abs() < 50_000.0); // Within 50km tolerance
    }

    #[test]
    fn test_travel_time() {
        let distance = 10_000.0; // 10 km
        let speed = 20.0; // 20 km/h
        let time = estimate_travel_time(distance, speed);
        assert!((time - 30.0).abs() < 0.01); // Should be 30 minutes
    }

    #[test]
    fn test_time_conflicts() {
        let starts = vec![0, 10, 25];
        let ends = vec![15, 20, 35];
        let conflicts = compute_time_conflicts(starts, ends);
        // Task 0 (0-15) and Task 1 (10-20) overlap
        assert!(conflicts.contains(&(0, 1)));
        // Task 1 (10-20) and Task 2 (25-35) do not overlap
        assert!(!conflicts.contains(&(1, 2)));
    }
}
