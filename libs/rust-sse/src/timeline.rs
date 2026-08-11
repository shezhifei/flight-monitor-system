use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use std::cmp::Ordering;

#[pyfunction]
pub fn layout_dynamic_tracks<'py>(py: Python<'py>, items: &Bound<'py, PyList>) -> PyResult<(Bound<'py, PyList>, Bound<'py, PyList>)> {
    let mut metrics = Vec::with_capacity(items.len());
    
    for idx in 0..items.len() {
        let item = items.get_item(idx)?;
        let dict = item.downcast::<PyDict>()?;
        
        let start_time_py = dict.get_item("start_time")?.unwrap();
        let start_time: f64 = start_time_py.call_method0("timestamp")?.extract()?;
        
        let end_time_py = dict.get_item("end_time")?.unwrap();
        let end_time: f64 = end_time_py.call_method0("timestamp")?.extract()?;
        
        let id_py = dict.get_item("id")?.unwrap();
        let id: String = id_py.extract()?;
        
        metrics.push((idx, start_time, end_time, id));
    }
    
    // Sort by start_time, end_time, id
    metrics.sort_by(|a, b| {
        a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal)
            .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal))
            .then_with(|| a.3.cmp(&b.3))
    });
    
    let mut lane_end_times: Vec<f64> = Vec::new();
    let mut lane_item_counts: HashMap<usize, usize> = HashMap::new();
    
    let sorted_items_pylist = PyList::empty(py);
    
    for (idx, start_time, end_time, _id) in metrics {
        let mut lane_index = 0;
        while lane_index < lane_end_times.len() && start_time < lane_end_times[lane_index] {
            lane_index += 1;
        }
        
        if lane_index == lane_end_times.len() {
            lane_end_times.push(end_time);
        } else {
            if end_time > lane_end_times[lane_index] {
                lane_end_times[lane_index] = end_time;
            }
        }
        
        *lane_item_counts.entry(lane_index).or_insert(0) += 1;
        
        let item = items.get_item(idx)?;
        let dict = item.downcast::<PyDict>()?;
        
        dict.set_item("lane_id", format!("flight-track-{}", lane_index + 1))?;
        dict.set_item("lane_label", format!("时间轨道 {}", lane_index + 1))?;
        dict.set_item("lane_index", lane_index)?;
        dict.set_item("lane_subtrack", 0)?;
        dict.set_item("lane_subtrack_count", 1)?;
        
        sorted_items_pylist.append(dict)?;
    }
    
    let lanes_pylist = PyList::empty(py);
    for index in 0..lane_end_times.len() {
        let lane_dict = PyDict::new(py);
        lane_dict.set_item("id", format!("flight-track-{}", index + 1))?;
        lane_dict.set_item("label", format!("时间轨道 {}", index + 1))?;
        lane_dict.set_item("index", index)?;
        lane_dict.set_item("subtrack_count", 1)?;
        lane_dict.set_item("item_count", lane_item_counts.get(&index).copied().unwrap_or(0))?;
        
        lanes_pylist.append(lane_dict)?;
    }
    
    Ok((lanes_pylist, sorted_items_pylist))
}

struct FixedMetrics {
    idx: usize,
    start_time: f64,
    end_time: f64,
    id: String,
    lane_key: String,
    lane_label: String,
}

#[pyfunction]
pub fn layout_fixed_lanes<'py>(py: Python<'py>, items: &Bound<'py, PyList>) -> PyResult<(Bound<'py, PyList>, Bound<'py, PyList>)> {
    let mut metrics_vec = Vec::with_capacity(items.len());
    
    for idx in 0..items.len() {
        let item = items.get_item(idx)?;
        let dict = item.downcast::<PyDict>()?;
        
        let start_time_py = dict.get_item("start_time")?.unwrap();
        let start_time: f64 = start_time_py.call_method0("timestamp")?.extract()?;
        
        let end_time_py = dict.get_item("end_time")?.unwrap();
        let end_time: f64 = end_time_py.call_method0("timestamp")?.extract()?;
        
        let id_py = dict.get_item("id")?.unwrap();
        let id: String = id_py.extract()?;
        
        let lane_key = match dict.get_item("lane_key")? {
            Some(v) => if v.is_none() { "lane:unknown".to_string() } else { v.extract().unwrap_or_else(|_| "lane:unknown".to_string()) },
            None => "lane:unknown".to_string(),
        };
        
        let lane_label = match dict.get_item("lane_label")? {
            Some(v) => if v.is_none() { lane_key.clone() } else { v.extract().unwrap_or_else(|_| lane_key.clone()) },
            None => lane_key.clone(),
        };
        
        metrics_vec.push(FixedMetrics { idx, start_time, end_time, id, lane_key, lane_label });
    }
    
    let mut grouped: HashMap<String, Vec<FixedMetrics>> = HashMap::new();
    let mut lane_labels: HashMap<String, String> = HashMap::new();
    for m in metrics_vec {
        let key = m.lane_key.clone();
        lane_labels.insert(key.clone(), m.lane_label.clone());
        grouped.entry(key).or_default().push(m);
    }
    
    let mut lane_keys: Vec<String> = grouped.keys().cloned().collect();
    
    lane_keys.sort_by(|a, b| {
        let is_unassigned_a = if a.contains("__unassigned__") { 1 } else { 0 };
        let is_unassigned_b = if b.contains("__unassigned__") { 1 } else { 0 };
        
        let label_a = lane_labels.get(a).unwrap().to_lowercase();
        let label_b = lane_labels.get(b).unwrap().to_lowercase();
        
        is_unassigned_a.cmp(&is_unassigned_b)
            .then_with(|| label_a.cmp(&label_b))
            .then_with(|| a.cmp(b))
    });
    
    let lanes_pylist = PyList::empty(py);
    let mut layout_items_vec = Vec::new();
    
    for (lane_index, lane_key) in lane_keys.iter().enumerate() {
        let mut lane_items = grouped.remove(lane_key).unwrap();
        let num_items = lane_items.len();
        
        lane_items.sort_by(|a, b| {
            a.start_time.partial_cmp(&b.start_time).unwrap_or(Ordering::Equal)
                .then_with(|| a.end_time.partial_cmp(&b.end_time).unwrap_or(Ordering::Equal))
                .then_with(|| a.id.cmp(&b.id))
        });
        
        let mut subtrack_end_times: Vec<f64> = Vec::new();
        let mut lane_layout_details = Vec::with_capacity(lane_items.len());
        
        for m in lane_items {
            let mut subtrack_index = 0;
            while subtrack_index < subtrack_end_times.len() && m.start_time < subtrack_end_times[subtrack_index] {
                subtrack_index += 1;
            }
            
            if subtrack_index == subtrack_end_times.len() {
                subtrack_end_times.push(m.end_time);
            } else {
                if m.end_time > subtrack_end_times[subtrack_index] {
                    subtrack_end_times[subtrack_index] = m.end_time;
                }
            }
            
            lane_layout_details.push((m.idx, m.start_time, m.id, subtrack_index));
        }
        
        let subtrack_count = if subtrack_end_times.is_empty() { 1 } else { subtrack_end_times.len() };
        let label = lane_labels.get(lane_key).unwrap();
        
        for (idx, start_time, id, subtrack_index) in lane_layout_details {
            let item = items.get_item(idx)?;
            let dict = item.downcast::<PyDict>()?;
            
            dict.set_item("lane_id", lane_key)?;
            dict.set_item("lane_index", lane_index)?;
            dict.set_item("lane_label", label)?;
            dict.set_item("lane_subtrack", subtrack_index)?;
            dict.set_item("lane_subtrack_count", subtrack_count)?;
            
            layout_items_vec.push((lane_index, start_time, id, idx));
        }
        
        let lane_dict = PyDict::new(py);
        lane_dict.set_item("id", lane_key)?;
        lane_dict.set_item("label", label)?;
        lane_dict.set_item("index", lane_index)?;
        lane_dict.set_item("subtrack_count", subtrack_count)?;
        lane_dict.set_item("item_count", num_items)?;
        
        lanes_pylist.append(lane_dict)?;
    }
    
    // Sort layout items globally
    layout_items_vec.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
            .then_with(|| a.2.cmp(&b.2))
    });
    
    let layout_items_pylist = PyList::empty(py);
    for (_, _, _, idx) in layout_items_vec {
        let item = items.get_item(idx)?;
        layout_items_pylist.append(item)?;
    }
    
    Ok((lanes_pylist, layout_items_pylist))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(layout_dynamic_tracks, m)?)?;
    m.add_function(wrap_pyfunction!(layout_fixed_lanes, m)?)?;
    Ok(())
}
