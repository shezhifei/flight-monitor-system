//! 航班状态机
//!
//! 对应 Python `src/domain/models/flight_state_machine.py`。

use super::value_objects::FlightStatus;
use std::collections::HashSet;

/// 获取当前状态允许的下一状态集合
pub fn get_allowed_transitions(current: FlightStatus) -> HashSet<FlightStatus> {
    use FlightStatus::*;
    match current {
        Scheduled => [PrevDeparted, Delayed, Cancelled].into_iter().collect(),
        PrevDeparted => [Arrived, Delayed, Cancelled].into_iter().collect(),
        Arrived => [CheckInEnd, Delayed, Cancelled].into_iter().collect(),
        CheckInEnd => [Boarding, Cancelled].into_iter().collect(),
        Boarding => [BoardingUrge, BoardingEnd, Cancelled].into_iter().collect(),
        BoardingUrge => [BoardingEnd, Cancelled].into_iter().collect(),
        BoardingEnd => [Departed, Cancelled].into_iter().collect(),
        Departed => [NextArrived].into_iter().collect(),
        NextArrived => HashSet::new(),
        Cancelled => HashSet::new(),
        Delayed => [Scheduled, PrevDeparted, Cancelled].into_iter().collect(),
    }
}

/// 校验状态转换是否合法
pub fn can_transition(current: FlightStatus, target: FlightStatus) -> bool {
    get_allowed_transitions(current).contains(&target)
}

/// 导出状态流转图（可视化友好格式）
pub fn export_transition_map() -> Vec<(FlightStatus, Vec<FlightStatus>)> {
    use FlightStatus::*;
    let all = [
        Scheduled,
        PrevDeparted,
        Arrived,
        CheckInEnd,
        Boarding,
        BoardingUrge,
        BoardingEnd,
        Departed,
        NextArrived,
        Cancelled,
        Delayed,
    ];
    all.into_iter()
        .map(|s| {
            let mut targets: Vec<FlightStatus> = get_allowed_transitions(s).into_iter().collect();
            targets.sort_by_key(|t| t.code());
            (s, targets)
        })
        .collect()
}
