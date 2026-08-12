"""软删除审计写入（审计合规：业务数据禁止物理删除）。

每个软删除动作生效后，向 ``system_audit_logs``（迁移 005）写入一条审计记录。
仓储层无法感知操作者，``user_id`` 留空；审计写入为 best-effort，失败仅记
日志，不阻断删除主流程（删除本身已持久化）。
"""
import json
import logging

logger = logging.getLogger(__name__)


async def record_soft_delete(conn, entity_type: str, entity_id: str, action: str = "soft_delete") -> None:
    """写入一条软删除审计记录（best-effort，需在连接/事务上下文中调用）。"""
    try:
        await conn.execute(
            "INSERT INTO system_audit_logs (entity_type, entity_id, action, changes) "
            "VALUES (%s, %s, %s, %s)",
            (entity_type, str(entity_id), action, json.dumps({"reason": "soft_delete"})),
        )
    except Exception as e:  # noqa: BLE001 - 审计失败不阻断主流程
        logger.warning(
            "写入软删除审计失败 entity_type=%s entity_id=%s: %s", entity_type, entity_id, e
        )
