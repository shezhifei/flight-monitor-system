"""报告生成工具执行器。"""

from datetime import timedelta
from typing import Any

from jsonschema import Draft202012Validator

from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.feature_flags import is_ai_feature_enabled
from src.infrastructure.ai.monitoring.metrics import record_report_schema_validation

from .base import BaseToolExecutor, ToolCategory
from .report_tools import ReportToolName

REPORT_PROMPT_TEMPLATE = """你是一名机场运行部的报告撰写专家。请根据以下航班变更日志，生成一份《航班运行异常报告》。

# 航班信息
- 航班号：{flight_number}
- 日期：{execution_date}
- 机型：{aircraft_type}
- 事件类型：{incident_type}

# 变更日志（按时间排序）
{change_logs}

# 备注信息
- 航班备注：{flight_remarks}
- 配载备注：{load_planning_remarks}
- 机务备注：{maintenance_remarks}

# 报告要求
1. 以时间线形式呈现事件经过
2. 标注关键节点（异常起始、处置措施、恢复正常）
3. 列出信息报送情况（如有）
4. 给出简短复盘建议

请使用Markdown格式输出报告。"""


REPORT_JSON_SCHEMA: dict[str, Any] = {
    "type": "object",
    "required": [
        "schema_version",
        "report_type",
        "title",
        "summary",
        "time_range",
        "findings",
        "metrics",
        "actions",
        "sources",
    ],
    "properties": {
        "schema_version": {"type": "string", "minLength": 1},
        "report_type": {
            "type": "string",
            "enum": ["ops_incident", "trend", "daily_brief"],
        },
        "title": {"type": "string", "minLength": 1},
        "summary": {"type": "string"},
        "time_range": {"type": "object"},
        "findings": {"type": "array"},
        "metrics": {"type": "array"},
        "actions": {"type": "array"},
        "sources": {"type": "array"},
    },
    "additionalProperties": True,
}


class ReportToolExecutor(BaseToolExecutor):
    """报告生成工具执行器"""

    def __init__(self, history_service=None, flight_service=None, ai_entity=None, default_user: str = "ReportAgent"):
        super().__init__(default_user)
        self._history_service = history_service
        self._flight_service = flight_service
        self._ai_entity = ai_entity

    def get_category(self) -> ToolCategory:
        return ToolCategory.REPORT

    def _register_handlers(self) -> None:
        self._handlers = {ReportToolName.GENERATE_INCIDENT_REPORT.value: self._handle_generate_report}

    def set_services(self, history_service=None, flight_service=None, ai_entity=None):
        if history_service:
            self._history_service = history_service
        if flight_service:
            self._flight_service = flight_service
        if ai_entity:
            self._ai_entity = ai_entity

    async def _handle_generate_report(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理生成报告请求"""
        flight_id = self._require_arg(args, "flight_id")

        incident_type = args.get("incident_type", "其他")
        time_range_hours = args.get("time_range_hours", 24)
        include_remarks = args.get("include_remarks", True)
        try:
            normalized_hours = max(1, min(int(time_range_hours), 168))
        except (TypeError, ValueError):
            normalized_hours = 24

        flight_number = "未知"
        execution_date = "未知"
        aircraft_type = "未知"
        flight_remarks = "无"
        load_planning_remarks = "无"
        maintenance_remarks = "无"

        if self._flight_service:
            flight = await self._safe_call(
                lambda: self._flight_service.get_flight(flight_id),
                "获取航班信息失败",
            )
            if flight:
                flight_obj = self._unwrap(flight, "get_flight")
                flight_number = str(self._extract_value(getattr(flight_obj, "flight_number", None)) or "未知")
                execution_date = self._derive_report_date(flight_obj)
                aircraft_type = str(self._extract_value(getattr(flight_obj, "aircraft_type_detail", None)) or "未知")
                if include_remarks:
                    flight_remarks = getattr(flight_obj, "flight_remarks", None) or "无"
                    load_planning_remarks = getattr(flight_obj, "load_planning_remarks", None) or "无"
                    maintenance_remarks = getattr(flight_obj, "aircraft_maintenance_remarks", None) or "无"

        change_logs = "暂无变更日志"
        if self._history_service:
            end_time = utc_now()
            start_time = end_time - timedelta(hours=normalized_hours)
            logs = await self._safe_call(
                lambda: self._history_service.get_flight_update_history(
                    flight_id=flight_id,
                    start_time=start_time,
                    end_time=end_time,
                    page_size=100,
                ),
                "获取变更日志失败",
                default=[],
            )
            if logs:
                change_logs = "\n".join(
                    [
                        f"- {log.get('timestamp', '未知时间')}: {log.get('field', '未知字段')} 从 {log.get('old_value', '空')} 变更为 {log.get('new_value', '空')} (操作人: {log.get('updated_by', '系统')})"
                        for log in logs
                    ]
                )

        prompt = REPORT_PROMPT_TEMPLATE.format(
            flight_number=flight_number,
            execution_date=execution_date,
            aircraft_type=aircraft_type,
            incident_type=incident_type,
            change_logs=change_logs,
            flight_remarks=flight_remarks,
            load_planning_remarks=load_planning_remarks,
            maintenance_remarks=maintenance_remarks,
        )

        def _report_fallback(exc: Exception | None) -> str:
            if exc:
                return f"# {flight_number} 异常事件报告\n\n生成失败: {exc!s}\n\n## 原始数据\n\n{change_logs}"
            return f"# {flight_number} 异常事件报告\n\n（AI未配置，仅返回原始数据）\n\n## 变更日志\n\n{change_logs}"

        report_content = await self._run_ai_task(
            prompt=prompt,
            ai_entity=self._ai_entity,
            error_message="AI生成报告失败",
            fallback_builder=_report_fallback,
        )

        report_json = self._build_report_json(
            report_markdown=report_content,
            flight_id=flight_id,
            flight_number=flight_number,
            incident_type=str(incident_type),
            time_range_hours=normalized_hours,
            change_logs=change_logs,
        )
        validation = self._validate_report_schema(report_json)
        report_markdown = self._select_report_markdown(
            report_json=report_json,
            original_markdown=report_content,
            validation=validation,
        )

        return self._success_response(
            flight_id=flight_id,
            flight_number=flight_number,
            incident_type=incident_type,
            report=report_markdown,
            report_markdown=report_markdown,
            report_json=report_json,
            validation=validation,
            generated_at=utc_now().isoformat(),
        )

    @staticmethod
    def _build_report_json(
        *,
        report_markdown: str,
        flight_id: str,
        flight_number: str,
        incident_type: str,
        time_range_hours: int,
        change_logs: str,
    ) -> dict[str, Any]:
        findings = []
        for line in str(change_logs or "").splitlines():
            text = line.strip()
            if not text:
                continue
            findings.append({"text": text[:240]})
            if len(findings) >= 8:
                break

        summary = str(report_markdown or "").strip().splitlines()
        summary_text = summary[0][:240] if summary else f"{flight_number} {incident_type} 报告"
        return {
            "schema_version": "1.0",
            "report_type": "ops_incident",
            "title": f"{flight_number} {incident_type} 事件报告",
            "summary": summary_text,
            "time_range": {
                "hours": max(1, int(time_range_hours or 24)),
            },
            "findings": findings,
            "metrics": [
                {
                    "name": "change_log_lines",
                    "value": len([line for line in str(change_logs or "").splitlines() if line.strip()]),
                },
            ],
            "actions": [],
            "sources": [
                {"type": "flight", "flight_id": flight_id, "flight_number": flight_number},
            ],
        }

    def _validate_report_schema(self, report_json: dict[str, Any]) -> dict[str, Any]:
        strict_enabled = is_ai_feature_enabled("AI_REPORT_SCHEMA_V1", default=True)
        report_type = str(report_json.get("report_type") or "unknown") if isinstance(report_json, dict) else "unknown"

        if strict_enabled:
            if not isinstance(report_json, dict):
                validation_payload = {
                    "schema_valid": False,
                    "errors": ["report_json must be an object"],
                }
                record_report_schema_validation(
                    schema_valid=False,
                    mode="jsonschema",
                    report_type=report_type,
                    error_count=1,
                )
                return validation_payload

            validator = Draft202012Validator(REPORT_JSON_SCHEMA)
            errors = sorted(validator.iter_errors(report_json), key=lambda item: list(item.path))
            normalized_errors = []
            for err in errors:
                path = ".".join(str(part) for part in err.path) or "$"
                normalized_errors.append(f"{path}: {err.message}")

            schema_valid = len(normalized_errors) == 0
            record_report_schema_validation(
                schema_valid=schema_valid,
                mode="jsonschema",
                report_type=report_type,
                error_count=len(normalized_errors),
            )
            return {
                "schema_valid": schema_valid,
                "errors": normalized_errors,
            }

        required_fields = {
            "schema_version",
            "report_type",
            "title",
            "summary",
            "time_range",
            "findings",
            "metrics",
            "actions",
            "sources",
        }
        errors = []
        if not isinstance(report_json, dict):
            errors.append("report_json must be an object")
            record_report_schema_validation(
                schema_valid=False,
                mode="legacy",
                report_type=report_type,
                error_count=1,
            )
            return {"schema_valid": False, "errors": errors}

        for field in sorted(required_fields):
            if field not in report_json:
                errors.append(f"missing required field: {field}")
        schema_valid = len(errors) == 0
        record_report_schema_validation(
            schema_valid=schema_valid,
            mode="legacy",
            report_type=report_type,
            error_count=len(errors),
        )
        return {"schema_valid": schema_valid, "errors": errors}

    @classmethod
    def _select_report_markdown(
        cls,
        *,
        report_json: dict[str, Any],
        original_markdown: str,
        validation: dict[str, Any],
    ) -> str:
        if not bool(validation.get("schema_valid", False)):
            return str(original_markdown or "")
        return cls._render_report_markdown(report_json, original_markdown)

    @staticmethod
    def _render_report_markdown(report_json: dict[str, Any], original_markdown: str) -> str:
        title = str(report_json.get("title") or "运行报告")
        summary = str(report_json.get("summary") or "").strip()
        findings = report_json.get("findings") if isinstance(report_json.get("findings"), list) else []
        metrics = report_json.get("metrics") if isinstance(report_json.get("metrics"), list) else []

        finding_lines = "\n".join(
            [f"- {item.get('text', '')}" for item in findings if isinstance(item, dict) and item.get("text")]
        )
        metric_lines = "\n".join(
            [f"- {item.get('name')}: {item.get('value')}" for item in metrics if isinstance(item, dict)]
        )

        template_markdown = (
            f"# {title}\n\n"
            f"## 摘要\n{summary or '无'}\n\n"
            f"## 关键发现\n{finding_lines or '- 无'}\n\n"
            f"## 指标\n{metric_lines or '- 无'}\n\n"
            "## 详细正文\n"
            f"{original_markdown or '无'}\n"
        )
        return template_markdown

    @staticmethod
    def _derive_report_date(flight_obj: Any) -> str:
        for attr in ("scheduled_departure", "estimated_departure", "scheduled_arrival", "estimated_arrival"):
            value = getattr(flight_obj, attr, None)
            if value is None:
                continue
            if hasattr(value, "date"):
                return str(value.date())
            text = str(value)
            if text:
                return text.split("T", 1)[0]
        return "未知"


__all__ = ["ReportToolExecutor"]
