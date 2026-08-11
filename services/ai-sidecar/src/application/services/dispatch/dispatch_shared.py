"""
派工共享模块

提供 Position 和 DispatchCalculator，支持 Rust 加速（如果可用）
"""

from dataclasses import dataclass
from math import atan2, cos, radians, sin, sqrt
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

# 尝试加载 Rust 加速模块
_USE_RUST = False
try:
    from rust_sse import (
        compute_cost_matrix_parallel as _rust_cost_matrix,
    )
    from rust_sse import (
        compute_feasibility_matrix as _rust_feasibility,
    )
    from rust_sse import (
        compute_time_conflicts as _rust_time_conflicts,
    )
    from rust_sse import (
        estimate_travel_time as _rust_travel_time,
    )
    from rust_sse import (
        haversine_distance as _rust_haversine,
    )

    _USE_RUST = True
    logger.info("Rust dispatch acceleration enabled")
except ImportError:
    logger.warning("Rust dispatch module not available, using Python fallback")


@dataclass
class Position:
    """坐标位置"""

    lat: float
    lng: float
    stand_id: str | None = None


class DispatchCalculator:
    """派工计算器 - 距离和时间计算（支持 Rust 加速）"""

    EARTH_RADIUS_METERS = 6371000
    DEFAULT_SPEED_KMH = 20  # 机坪移动速度 km/h

    @staticmethod
    def is_rust_enabled() -> bool:
        """检查 Rust 加速是否可用"""
        return _USE_RUST

    @staticmethod
    def haversine_distance(lat1: float, lng1: float, lat2: float, lng2: float) -> float:
        """使用 Haversine 公式计算两点间距离（米）"""
        if _USE_RUST:
            return _rust_haversine(lat1, lng1, lat2, lng2)

        # Python 回退实现
        lat1, lng1, lat2, lng2 = map(radians, [lat1, lng1, lat2, lng2])
        dlat = lat2 - lat1
        dlng = lng2 - lng1

        a = sin(dlat / 2) ** 2 + cos(lat1) * cos(lat2) * sin(dlng / 2) ** 2
        c = 2 * atan2(sqrt(a), sqrt(1 - a))

        return DispatchCalculator.EARTH_RADIUS_METERS * c

    @staticmethod
    def estimate_travel_time(distance_meters: float, speed_kmh: float | None = None) -> float:
        """估算移动时间（分钟）"""
        speed = speed_kmh or DispatchCalculator.DEFAULT_SPEED_KMH
        if _USE_RUST:
            return _rust_travel_time(distance_meters, speed)
        return (distance_meters / 1000) / speed * 60

    @staticmethod
    def compute_cost_matrix(tasks: list[Any], teams: list[Any], speed_kmh: float | None = None) -> list[list[float]]:
        """
        批量计算成本矩阵（到达时间，分钟）

        Args:
            tasks: 任务列表，每个任务需有 stand_position 属性
            teams: 班组列表，每个班组需有 position 属性
            speed_kmh: 移动速度（km/h）

        Returns:
            二维列表 [tasks x teams] 的移动时间（分钟）
        """
        speed = speed_kmh or DispatchCalculator.DEFAULT_SPEED_KMH

        if _USE_RUST:
            task_positions = [(t.stand_position.lat, t.stand_position.lng) for t in tasks]
            team_positions = [(t.position.lat, t.position.lng) for t in teams]
            return _rust_cost_matrix(task_positions, team_positions, speed)

        # Python 回退
        cost = []
        for task in tasks:
            row = []
            for team in teams:
                distance = DispatchCalculator.haversine_distance(
                    team.position.lat, team.position.lng, task.stand_position.lat, task.stand_position.lng
                )
                travel_time = DispatchCalculator.estimate_travel_time(distance, speed)
                row.append(travel_time)
            cost.append(row)
        return cost

    @staticmethod
    def compute_time_conflicts(tasks: list[Any]) -> list[tuple[int, int]]:
        """
        计算时间冲突对（任务时间窗口重叠）

        Args:
            tasks: 任务列表，每个任务需有 planned_start 和 planned_end 属性

        Returns:
            冲突对列表 [(i, j), ...]
        """
        if _USE_RUST:
            starts = [int(t.planned_start.timestamp()) for t in tasks]
            ends = [int(t.planned_end.timestamp()) for t in tasks]
            return _rust_time_conflicts(starts, ends)

        # Python 回退
        conflicts = []
        for i in range(len(tasks)):
            for j in range(i + 1, len(tasks)):
                if tasks[i].planned_start < tasks[j].planned_end and tasks[j].planned_start < tasks[i].planned_end:
                    conflicts.append((i, j))
        return conflicts

    @staticmethod
    def compute_feasibility_matrix(tasks: list[Any], teams: list[Any]) -> list[list[bool]]:
        """
        计算可行性矩阵（班组类型是否匹配）

        Args:
            tasks: 任务列表，每个任务需有 required_team_type_ids 属性
            teams: 班组列表，每个班组需有 team_type_id 属性

        Returns:
            二维列表 [tasks x teams] 的可行性布尔值
        """
        if _USE_RUST:
            task_types = [list(t.required_team_type_ids) for t in tasks]
            team_types = [t.team_type_id for t in teams]
            return _rust_feasibility(task_types, team_types)

        # Python 回退
        feasible = []
        for task in tasks:
            row = []
            for team in teams:
                is_feasible = team.team_type_id in task.required_team_type_ids
                row.append(is_feasible)
            feasible.append(row)
        return feasible

    @staticmethod
    def calculate_score(
        team_position: Position,
        equipment_positions: list[Position],
        target_stand: Position,
        weights: dict[str, float] | None = None,
    ) -> dict[str, float]:
        """
        计算派工方案评分
        """
        weights = weights or {"wait_time": 0.7, "distance": 0.3}

        # 计算班组到机位的距离
        team_distance = DispatchCalculator.haversine_distance(
            team_position.lat, team_position.lng, target_stand.lat, target_stand.lng
        )
        team_travel_time = DispatchCalculator.estimate_travel_time(team_distance)

        # 计算设备相关时间
        equipment_travel_times = []
        equipment_distances = []

        for eq_pos in equipment_positions:
            eq_distance = DispatchCalculator.haversine_distance(
                eq_pos.lat, eq_pos.lng, target_stand.lat, target_stand.lng
            )
            equipment_distances.append(eq_distance)
            equipment_travel_times.append(DispatchCalculator.estimate_travel_time(eq_distance))

        # 最终到达时间 = 所有资源中最慢的
        all_travel_times = [team_travel_time, *equipment_travel_times]
        max_travel_time = max(all_travel_times) if all_travel_times else 0

        # 总移动距离
        total_distance = team_distance + sum(equipment_distances)

        # 归一化评分
        wait_score = max_travel_time / 60  # 转换为小时
        distance_score = total_distance / 1000  # 转换为公里

        final_score = weights["wait_time"] * wait_score + weights["distance"] * distance_score

        return {
            "score": final_score,
            "travel_time_minutes": max_travel_time,
            "total_distance_meters": total_distance,
            "team_travel_time": team_travel_time,
            "team_distance": team_distance,
        }
