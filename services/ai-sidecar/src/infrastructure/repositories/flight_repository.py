"""航班仓库接口

定义航班聚合根的持久化接口，支持泛型和配置化
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from datetime import date
from typing import Any

from src.domain.aggregates.flight import FlightAggregate


@dataclass
class RepositoryConfig:
    """仓库配置"""

    enable_caching: bool = True
    cache_ttl: int = 300  # 5分钟
    enable_metrics: bool = True
    batch_size: int = 100
    enable_soft_delete: bool = False


@dataclass
class QueryOptions:
    """查询选项"""

    limit: int | None = None
    offset: int | None = None
    order_by: str | None = None
    order_direction: str = "ASC"
    include_deleted: bool = False


class AsyncFlightRepository(ABC):
    """异步航班仓储接口"""

    @abstractmethod
    async def save(self, aggregate: FlightAggregate) -> bool:
        """异步保存航班聚合根"""
        pass

    @abstractmethod
    async def find_by_id(self, flight_id) -> FlightAggregate | None:
        """根据ID查找航班"""
        pass

    @abstractmethod
    async def load_for_update(self, flight_id: str) -> FlightAggregate | None:
        """命令写路径专用：从事件流回放聚合状态。"""
        pass

    @abstractmethod
    async def find_all(self, limit: int = 100, offset: int = 0) -> list[FlightAggregate]:
        """异步查询航班列表"""
        pass

    @abstractmethod
    async def count_all(self) -> int:
        """异步统计航班总数。"""
        pass

    @abstractmethod
    async def search(self, criteria: dict[str, Any], limit: int = 100, offset: int = 0) -> list[FlightAggregate]:
        """异步搜索航班（数据库级过滤）

        Args:
            criteria: 搜索条件字典，支持:
                - flight_no: 航班号模糊匹配 (ILIKE)
                - status: 状态精确匹配
                - origin: 出发地精确匹配
                - destination: 目的地精确匹配
            limit: 返回数量限制
            offset: 分页偏移
        """
        pass

    @abstractmethod
    async def find_gate_map(self, flight_ids: list[str]) -> dict[str, str | None]:
        """批量查询航班登机口映射。"""
        pass

    @abstractmethod
    async def find_from_view(self, flight_id: str) -> FlightAggregate | None:
        """从读模型直接查询航班（不进行事件回放）"""
        pass

    @abstractmethod
    async def find_by_leg_natural_key(
        self,
        *,
        leg_type: str,
        flight_no: str,
        operation_date: date,
        limit: int = 5,
    ) -> list[FlightAggregate]:
        """按方向航段 + 航班号 + 运行日期查询候选航班。"""
        pass

    @abstractmethod
    async def find_business_cases_by_flight_ids(self, flight_ids: list[str]) -> list[Any]:
        """批量根据航班ID查询业务事项"""
        pass

    @abstractmethod
    async def find_latest_dispatch_timeline_snapshot(self, flight_ids: list[str]) -> dict[str, dict[str, Any]]:
        """批量查询航班最新调度时间线节点快照。"""
        pass

    # --------------- Precedent Retrieval ---------------

    @abstractmethod
    async def find_policy_interceptions_by_error_code(
        self,
        error_code: str,
        limit: int = 5,
        *,
        include_archived: bool = True,
    ) -> list[dict[str, Any]]:
        """根据 error_code 检索 policy_interception 类型的业务事项。

        查询条件:
          - case_type = 'policy_interception'
          - context->>'error_code' = error_code
        排序: created_at DESC
        来源: flight_business_cases (当前) + archived_flight_business_cases (可选)

        Returns:
            展示所需最小字段列表 (case_id, flight_id, case_type, description,
            context, status, created_at, created_by, stand, gate).
        """
        pass

    # --------------- Rule Impact Counter ---------------

    @abstractmethod
    async def count_flights_by_criteria(
        self,
        criteria: dict[str, Any],
    ) -> int:
        """按搜索条件统计航班数量 (count-only)。

        复用 search() 相同的 criteria 字典语义，但只返回计数。
        用于规则影响评估等场景，避免加载完整聚合根。
        """
        pass
