# Flight Monitor System — Ontology V1 契约

> 日期：2026-05-11
> 版本：1.0
> 状态：草稿；当前 `feat/ontology-definition` 分支只完成飞机中心运行资源子集，
> 本契约列出的完整 AI 动作集合仍未全部落地。
> 目标：定义航班运行 Ontology V1 的对象类型、动作类别、契约规则，为 AI Agent 提供可执行的结构化业务语言。

---

## 1. 概述

Ontology V1 是 Flight Monitor System 的 AI 可读业务模型，定义了 AI 能够理解和操作的**核心对象**、**合法动作**、**输入输出契约**和**风险策略**。

### 1.1 设计原则

1. **最小化**：只包含最高价值运行场景所需的对象和动作
2. **确定性**：每个动作都有明确的输入 schema、输出 schema 和执行路径
3. **可验证**：所有动作输出必须经过 Rust 层 schema 校验
4. **可审计**：每个动作都携带 ontology version、correlation_id 和审计字段

### 1.2 Ontology 版本策略

- 每个 ontology 都有版本号（如 `v1.0`）
- 版本变更通过 migration 管理，不破坏向后兼容
- Python sidecar 消费带版本的 schema 快照
- 每个 AI job、action proposal、pending action 都标注 ontology version

**版本命名决策（Phase 0，2026-08-12）**：

| 范围 | 版本标识 | 出现位置 |
|---|---|---|
| AI Ontology schema | `flight-ops.v1`（常量 `FLIGHT_OPS_ONTOLOGY_VERSION`） | `ontology_version` 字段、schema 导出、proposal、AI job |
| 飞机中心资源本体 | 独立域模型，无 AI `ontology_version` | `/api/v2/ontology`、migration 119 |

禁止使用裸 `v1.0` / `1.0` / `1.0.0` 作为 ontology 版本。`correlation_id`、
`object_id`、`arguments`（运行时取值）是 proposal 运行时字段（§4.1），
不出现在 schema 导出中；schema 只声明 `arguments_schema`。

---

## 2. V1 对象类型

### 2.1 Flight / FlightLeg（航班 / 航段）

**对象 ID 策略**：`flight_id`（ULID 格式）

**核心字段**：

```json
{
  "flight_id": "string",
  "flight_number": "string",
  "status": "enum: scheduled|prev_departed|arrived|check_in_end|boarding|boarding_urge|boarding_end|departed|next_arrived|cancelled|delayed",
  "stand": "string|null",
  "gate": "string|null",
  "terminal": "string|null",
  "scheduled_departure": "datetime|null",
  "scheduled_arrival": "datetime|null",
  "estimated_departure": "datetime|null",
  "estimated_arrival": "datetime|null",
  "actual_departure": "datetime|null",
  "actual_arrival": "datetime|null",
  "registration": "string|null",
  "aircraft_type": "string|null",
  "inbound_leg": {
    "flight_no": "string",
    "origin_code": "string",
    "destination_code": "string",
    "scheduled_time": "datetime",
    "estimated_time": "datetime|null",
    "actual_time": "datetime|null"
  },
  "outbound_leg": {
    "flight_no": "string",
    "origin_code": "string",
    "destination_code": "string",
    "scheduled_time": "datetime",
    "estimated_time": "datetime|null",
    "actual_time": "datetime|null"
  },
  "anomaly_summary": "map<string, any>",
  "labels": "string[]",
  "flight_remarks": "string|null",
  "created_at": "datetime",
  "updated_at": "datetime",
  "version": "integer"
}
```

**初始关系**：

| 关系类型 | 目标对象 | 描述 |
|---|---|---|
| `stands_at` | Stand | 当前或计划停靠的机位 |
| `dispatch_orders` | DispatchOrder[] | 关联的派工单列表 |
| `anomalies` | Anomaly[] | 关联的异常列表 |
| `business_cases` | BusinessCase[] | 关联的业务事项 |

### 2.2 Stand（机位）

**对象 ID 策略**：`stand_id`（字符串格式，如 `S1`、`W23`）

**核心字段**：

```json
{
  "stand_id": "string",
  "code": "string",
  "terminal": "string|null",
  "area": "string|null",
  "stand_type": "string|null",
  "size_category": "string|null",
  "is_active": "boolean",
  "position_lat": "float",
  "position_lng": "float",
  "current_flight_id": "string|null",
  "reservations": [
    {
      "flight_id": "string",
      "start_time": "datetime",
      "end_time": "datetime",
      "reason": "string"
    }
  ],
  "constraints": [
    {
      "type": "enum: closure|restriction|maintenance",
      "start_time": "datetime",
      "end_time": "datetime|null",
      "reason": "string"
    }
  ]
}
```

**初始关系**：

| 关系类型 | 目标对象 | 描述 |
|---|---|---|
| `current_flight` | Flight | 当前停靠的航班 |
| `reservations` | Flight[] | 预留的航班列表 |
| `constraints` | Constraint[] | 机位约束 |

### 2.3 DispatchOrder（派工单）

**对象 ID 策略**：`dispatch_order_id`（ULID 格式）

**核心字段**：

```json
{
  "dispatch_order_id": "string",
  "flight_id": "string",
  "task_type": "string",
  "task_type_name": "string|null",
  "stand_id": "string|null",
  "stand_code": "string|null",
  "terminal": "string|null",
  "assignee_type": "enum: team|individual",
  "team_id": "string|null",
  "team_name": "string|null",
  "individual_user_id": "string|null",
  "individual_username": "string|null",
  "driver_team_id": "string|null",
  "driver_user_id": "string|null",
  "status": "enum: pending|assigned|in_progress|completed|cancelled",
  "dispatch_type": "enum: auto|manual",
  "planned_start_time": "datetime|null",
  "planned_end_time": "datetime|null",
  "actual_start_time": "datetime|null",
  "actual_end_time": "datetime|null",
  "estimated_completion_time": "datetime|null",
  "publication_state": "enum: prepublished|published|cancelled",
  "workflow_status": "string",
  "lock_level": "enum: active|frozen|manual_lock|optimizable",
  "members": [
    {
      "user_id": "string",
      "username": "string|null",
      "role": "enum: leader|member|driver",
      "check_in_time": "datetime|null",
      "check_out_time": "datetime|null"
    }
  ],
  "equipment_assignment": [
    {
      "equipment_id": "string",
      "code": "string"
    }
  ],
  "score_breakdown": "map<string, float>",
  "recommendation_score": "float|null",
  "conflict_reason": "string|null",
  "created_at": "datetime",
  "updated_at": "datetime"
}
```

**初始关系**：

| 关系类型 | 目标对象 | 描述 |
|---|---|---|
| `flight` | Flight | 关联航班 |
| `team` | Team | 作业班组 |
| `equipment` | Equipment[] | 分配的设备 |
| `process_instance` | WorkflowRun | 关联流程实例 |

### 2.4 Team / Personnel（班组 / 人员）

**对象 ID 策略**：`team_id` / `user_id`（ULID 格式）

**Team 核心字段**：

```json
{
  "team_id": "string",
  "name": "string",
  "team_type_id": "string|null",
  "team_type_name": "string|null",
  "code": "string|null",
  "leader_id": "string|null",
  "terminal": "string|null",
  "current_status": "enum: on_duty|off_duty|break",
  "current_position_lat": "float|null",
  "current_position_lng": "float|null",
  "current_stand_id": "string|null",
  "members": [
    {
      "user_id": "string",
      "username": "string|null",
      "display_name": "string|null",
      "role": "enum: leader|member|driver",
      "qualifications": ["string"],
      "can_drive": "boolean",
      "is_active": "boolean"
    }
  ],
  "current_dispatch_orders": [
    {
      "dispatch_order_id": "string",
      "task_type": "string",
      "status": "string",
      "planned_end_time": "datetime|null"
    }
  ]
}
```

**Personnel 核心字段**：

```json
{
  "user_id": "string",
  "username": "string",
  "display_name": "string|null",
  "qualifications": [
    {
      "code": "string",
      "name": "string",
      "level_code": "string",
      "valid_from": "datetime|null",
      "valid_to": "datetime|null"
    }
  ],
  "current_team_id": "string|null",
  "current_status": "enum: on_duty|off_duty|break",
  "current_location": {
    "type": "enum: stand|team_base|mobile",
    "stand_id": "string|null",
    "team_id": "string|null",
    "position_lat": "float|null",
    "position_lng": "float|null"
  },
  "active_dispatch_orders": [
    {
      "dispatch_order_id": "string",
      "task_type": "string",
      "check_in_time": "datetime|null"
    }
  ]
}
```

### 2.5 Equipment（设备）

**对象 ID 策略**：`equipment_id`（ULID 格式）

**核心字段**：

```json
{
  "equipment_id": "string",
  "code": "string",
  "equipment_type_id": "string|null",
  "equipment_type_name": "string|null",
  "name": "string|null",
  "license_plate": "string|null",
  "terminal": "string|null",
  "status": "enum: available|in_use|maintenance|retired",
  "current_position_lat": "float|null",
  "current_position_lng": "float|null",
  "current_stand_id": "string|null",
  "current_dispatch_id": "string|null",
  "last_position_update": "datetime|null",
  "next_maintenance_date": "date|null",
  "is_active": "boolean"
}
```

### 2.6 Anomaly（异常）

**对象 ID 策略**：`anomaly_id`（ULID 格式）

**核心字段**：

```json
{
  "anomaly_id": "string",
  "flight_id": "string|null",
  "anomaly_type": "enum: service_node_timeout|gate_stand_conflict|kpi_degradation|ai_risk|dispatch_issue",
  "severity": "enum: low|medium|high|critical",
  "title": "string",
  "description": "string|null",
  "status": "enum: open|acknowledged|resolved",
  "detected_at": "datetime",
  "resolved_at": "datetime|null",
  "escalation_level": "integer",
  "linked_todo_id": "string|null",
  "rule_id": "string|null",
  "context_data": "map<string, any>",
  "created_at": "datetime",
  "updated_at": "datetime"
}
```

**初始关系**：

| 关系类型 | 目标对象 | 描述 |
|---|---|---|
| `flight` | Flight|null | 关联航班 |
| `stand` | Stand|null | 关联机位 |
| `dispatch_order` | DispatchOrder|null | 关联派工单 |
| `linked_todo` | Todo|null | 关联待办 |

### 2.7 BusinessCase / WorkflowRun（业务事项 / 流程实例）

**对象 ID 策略**：`business_case_id` / `workflow_run_id`（ULID 格式）

**BusinessCase 核心字段**：

```json
{
  "business_case_id": "string",
  "case_type": "string",
  "case_type_name": "string|null",
  "title": "string",
  "description": "string|null",
  "status": "enum: draft|open|in_progress|resolved|closed|cancelled",
  "priority": "enum: low|medium|high|urgent",
  "department_id": "string|null",
  "department_name": "string|null",
  "flight_id": "string|null",
  "workflow_run_id": "string|null",
  "current_assignee": {
    "type": "enum: user|team|department",
    "id": "string|null",
    "name": "string|null"
  },
  "created_at": "datetime",
  "updated_at": "datetime"
}
```

### 2.8 Notification / Todo（通知 / 待办）

**对象 ID 策略**：`notification_id` / `todo_id`（ULID 格式）

**Notification 核心字段**：

```json
{
  "notification_id": "string",
  "title": "string",
  "body": "string",
  "notification_type": "string",
  "user_id": "string",
  "is_read": "boolean",
  "read_at": "datetime|null",
  "source_type": "string|null",
  "source_id": "string|null",
  "action_url": "string|null",
  "created_at": "datetime"
}
```

**Todo 核心字段**：

```json
{
  "todo_id": "string",
  "title": "string",
  "description": "string|null",
  "priority": "enum: low|medium|high|urgent",
  "status": "enum: pending|in_progress|completed|cancelled",
  "category": "string|null",
  "due_date": "datetime|null",
  "assigned_to": "string|null",
  "source_type": "string|null",
  "source_id": "string|null",
  "created_at": "datetime",
  "updated_at": "datetime"
}
```

---

## 3. V1 动作类别

### 3.1 只读动作（Read-Only）

只读动作不产生副作用，可直接执行。

#### 3.1.1 `flight.get_context`

**描述**：获取航班完整上下文快照

**输入 schema**：

```json
{
  "flight_id": "string (required)",
  "include_relations": ["dispatch_orders", "anomalies", "business_cases", "labels"]
}
```

**输出 schema**：

```json
{
  "flight": "Flight",
  "dispatch_orders": "DispatchOrder[]",
  "anomalies": "Anomaly[]",
  "business_cases": "BusinessCase[]",
  "labels": "string[]",
  "evidence": {
    "retrieved_at": "datetime",
    "ontology_version": "string"
  }
}
```

**执行归属**：Rust repositories，只读

**所需权限**：`flight:read`

#### 3.1.2 `flight.search`

**描述**：搜索航班列表

**输入 schema**：

```json
{
  "flight_no": "string|null",
  "status": "string|null",
  "origin": "string|null",
  "destination": "string|null",
  "date": "date|null",
  "has_open_anomaly": "boolean|null",
  "limit": "integer (default: 50, max: 200)",
  "offset": "integer (default: 0)"
}
```

**输出 schema**：

```json
{
  "flights": "Flight[]",
  "total": "integer",
  "evidence": {
    "retrieved_at": "datetime",
    "query_params": "object"
  }
}
```

**执行归属**：Rust repositories，只读

**所需权限**：`flight:read`

#### 3.1.3 `dispatch.get_status`

**描述**：获取派工单状态摘要

**输入 schema**：

```json
{
  "dispatch_order_id": "string (required)"
}
```

**输出 schema**：

```json
{
  "dispatch_order": "DispatchOrder",
  "team": "Team|null",
  "equipment": "Equipment[]",
  "conflicts": [
    {
      "type": "enum: time_conflict|resource_conflict|constraint_violation",
      "description": "string"
    }
  ],
  "evidence": {
    "retrieved_at": "datetime"
  }
}
```

#### 3.1.4 `anomaly.list_open`

**描述**：列出所有未解决的异常

**输入 schema**：

```json
{
  "severity": "string|null",
  "flight_id": "string|null",
  "limit": "integer (default: 50)"
}
```

**输出 schema**：

```json
{
  "anomalies": "Anomaly[]",
  "total": "integer",
  "summary": {
    "critical": "integer",
    "high": "integer",
    "medium": "integer",
    "low": "integer"
  }
}
```

#### 3.1.5 `stand.check_availability`

**描述**：检查机位可用性

**输入 schema**：

```json
{
  "stand_id": "string (required)",
  "time_window": {
    "start": "datetime (required)",
    "end": "datetime (required)"
  }
}
```

**输出 schema**：

```json
{
  "stand": "Stand",
  "is_available": "boolean",
  "conflicts": [
    {
      "flight_id": "string",
      "start_time": "datetime",
      "end_time": "datetime",
      "reason": "string"
    }
  ],
  "alternative_suggestions": [
    {
      "stand_id": "string",
      "score": "float (0-1)"
    }
  ]
}
```

#### 3.1.6 `report.generate_briefing`

**描述**：生成班前/班中运行简报

**输入 schema**：

```json
{
  "department_id": "string|null",
  "shift_start": "datetime|null",
  "shift_end": "datetime|null",
  "scope": "enum: all|inbound|outbound"
}
```

**输出 schema**：

```json
{
  "briefing": {
    "title": "string",
    "generated_at": "datetime",
    "flights_summary": {
      "total": "integer",
      "arrivals": "integer",
      "departures": "integer",
      "delayed": "integer",
      "cancelled": "integer"
    },
    "dispatch_summary": {
      "total": "integer",
      "pending": "integer",
      "in_progress": "integer",
      "completed": "integer"
    },
    "anomaly_summary": {
      "open": "integer",
      "critical": "integer"
    },
    "upcoming_tasks": [
      {
        "dispatch_order_id": "string",
        "task_type": "string",
        "flight_no": "string",
        "planned_time": "datetime"
      }
    ],
    "checklist": ["string"]
  },
  "confidence": "float",
  "limitations": ["string"]
}
```

### 3.2 建议动作（Advisory）

建议动作产生 `AiActionProposal`，不直接执行。

#### 3.2.1 `flight.suggest_stand_adjustment`

**描述**：建议调整航班机位

**输入 schema**：

```json
{
  "flight_id": "string (required)",
  "reason": "string (required)",
  "preferred_stand_id": "string|null"
}
```

**输出 schema**（生成 proposal）：

```json
{
  "proposal": {
    "proposal_id": "string (ULID)",
    "ontology_version": "string",
    "object_type": "flight",
    "object_id": "string",
    "action_name": "stand_adjustment",
    "arguments": {
      "current_stand_id": "string|null",
      "proposed_stand_id": "string",
      "reason": "string"
    },
    "risk_level": "enum: low|medium|high|critical",
    "before_snapshot": "Flight",
    "after_preview": "Flight",
    "diff_summary": {
      "changed_fields": ["string"],
      "impact": "string"
    },
    "constraint_results": [
      {
        "constraint": "string",
        "passed": "boolean",
        "details": "string|null"
      }
    ],
    "approval_policy": "string",
    "confidence": "float",
    "reasoning": "string"
  }
}
```

**执行归属**：Rust pending action pipeline

**所需权限**：`flight:write`

#### 3.2.2 `dispatch.suggest_replan`

**描述**：建议派工重排

**输入 schema**：

```json
{
  "flight_id": "string (required)",
  "task_type": "string|null",
  "reason": "string (required)"
}
```

**输出 schema**：

```json
{
  "proposal": {
    "proposal_id": "string",
    "ontology_version": "string",
    "object_type": "dispatch_order",
    "action_name": "replan",
    "arguments": {
      "dispatch_order_id": "string",
      "replan_type": "enum: reassign_team|reschedule|reassign_equipment|full",
      "proposed_changes": {
        "team_id": "string|null",
        "planned_start_time": "datetime|null",
        "equipment_ids": ["string"]|null
      },
      "reason": "string"
    },
    "risk_level": "enum: low|medium|high|critical",
    "before_snapshot": "DispatchOrder",
    "after_preview": "DispatchOrder",
    "diff_summary": {
      "changed_fields": ["string"],
      "constraint_violations": ["string"],
      "score_change": "float"
    },
    "constraint_results": [
      {
        "constraint": "string",
        "passed": "boolean",
        "details": "string|null"
      }
    ],
    "approval_policy": "string",
    "confidence": "float",
    "reasoning": "string"
  }
}
```

#### 3.2.3 `anomaly.suggest_escalation`

**描述**：建议异常升级

**输入 schema**：

```json
{
  "anomaly_id": "string (required)",
  "escalation_type": "enum: increase_severity|notify_supervisor|create_business_case|abort_operation"
}
```

**输出 schema**：

```json
{
  "proposal": {
    "proposal_id": "string",
    "ontology_version": "string",
    "object_type": "anomaly",
    "object_id": "string",
    "action_name": "escalate",
    "arguments": {
      "escalation_type": "string",
      "new_severity": "string|null",
      "notify_users": ["string"]|null,
      "business_case_template": "string|null"
    },
    "risk_level": "enum: low|medium|high|critical",
    "reasoning": "string"
  }
}
```

#### 3.2.4 `flight.suggest_delay_action`

**描述**：建议航班延误处置动作

**输入 schema**：

```json
{
  "flight_id": "string (required)",
  "delay_minutes": "integer (required)"
}
```

**输出 schema**：

```json
{
  "proposal": {
    "proposal_id": "string",
    "object_type": "flight",
    "object_id": "string",
    "action_name": "update_delay",
    "arguments": {
      "new_estimated_departure": "datetime",
      "new_cobt": "datetime|null"
    },
    "risk_level": "medium",
    "reasoning": "string",
    "related_actions": [
      {
        "action_name": "dispatch.reschedule",
        "dispatch_order_ids": ["string"],
        "reason": "string"
      }
    ]
  }
}
```

#### 3.2.5 `notification.suggest_broadcast`

**描述**：建议发送广播通知

**输入 schema**：

```json
{
  "template": "string (required)",
  "context": {
    "flight_id": "string|null",
    "stand_id": "string|null",
    "message": "string|null"
  },
  "recipients": {
    "type": "enum: all|role|team|department|individual",
    "target_ids": ["string"]|null
  }
}
```

**输出 schema**：

```json
{
  "proposal": {
    "proposal_id": "string",
    "object_type": "notification",
    "action_name": "broadcast",
    "arguments": {
      "title": "string",
      "body": "string",
      "recipients": ["string"],
      "source_type": "ai_suggestion",
      "source_id": "string"
    },
    "risk_level": "low",
    "reasoning": "string"
  }
}
```

### 3.3 受控写动作（Controlled Write）

受控写动作通过 pending action 或 Flowable 审批后执行。

#### 3.3.1 `flight.update_stand`

**描述**：调整航班机位

**执行路径**：`proposal → pending_action → approval → Rust FlightService.update_stand()`

**输入 schema**：

```json
{
  "flight_id": "string (required)",
  "new_stand_id": "string (required)",
  "reason": "string (required)"
}
```

**所需权限**：`flight:write`

**审批策略**：

| 风险等级 | 条件 | 策略 |
|---|---|---|
| LOW | 请求者有 `flight:auto_write` 权限 | 自动执行 |
| MEDIUM | 无 | 进入 pending approval |
| HIGH | 无 | 进入 Flowable 审批 |

#### 3.3.2 `flight.update_delay`

**描述**：更新航班延误信息

**执行路径**：`proposal → pending_action → approval → Rust FlightService.update_partial()`

**输入 schema**：

```json
{
  "flight_id": "string (required)",
  "estimated_departure": "datetime|null",
  "estimated_arrival": "datetime|null",
  "reason": "string|null"
}
```

**所需权限**：`flight:write`

#### 3.3.3 `dispatch.update_status`

**描述**：更新派工单状态

**执行路径**：`proposal → pending_action → approval → Rust DispatchOrderRepository.update_status()`

**输入 schema**：

```json
{
  "dispatch_order_id": "string (required)",
  "new_status": "enum: pending|assigned|in_progress|completed|cancelled",
  "actual_start_time": "datetime|null",
  "actual_end_time": "datetime|null",
  "notes": "string|null"
}
```

**所需权限**：`dispatch:write`

#### 3.3.4 `dispatch.reassign`

**描述**：重新分配派工单

**执行路径**：`proposal → pending_action → approval → Rust DispatchService.reassign()`

**输入 schema**：

```json
{
  "dispatch_order_id": "string (required)",
  "new_team_id": "string|null",
  "new_individual_user_id": "string|null",
  "reason": "string (required)"
}
```

**所需权限**：`dispatch:admin`

#### 3.3.5 `dispatch.publish`

**描述**：发布预发布的派工单

**执行路径**：`proposal → pending_action → approval → Rust DispatchService.publish()`

**输入 schema**：

```json
{
  "dispatch_order_ids": ["string (required)"],
  "force": "boolean (default: false)"
}
```

**所需权限**：`dispatch:publish`

#### 3.3.6 `anomaly.acknowledge`

**描述**：确认异常

**执行路径**：`proposal → pending_action → Rust AnomalyRepository.acknowledge()`

**输入 schema**：

```json
{
  "anomaly_id": "string (required)",
  "note": "string|null"
}
```

**所需权限**：`anomaly:write`

#### 3.3.7 `anomaly.resolve`

**描述**：解决异常

**执行路径**：`proposal → pending_action → Rust AnomalyRepository.resolve()`

**输入 schema**：

```json
{
  "anomaly_id": "string (required)",
  "resolution_note": "string|null"
}
```

**所需权限**：`anomaly:write`

#### 3.3.8 `notification.send`

**描述**：发送通知

**执行路径**：`proposal → Rust NotificationService.send()`

**输入 schema**：

```json
{
  "title": "string (required)",
  "body": "string (required)",
  "user_ids": ["string (required)"],
  "source_type": "string|null",
  "source_id": "string|null",
  "action_url": "string|null"
}
```

**所需权限**：`notification:send`

#### 3.3.9 `label.add`

**描述**：为航班添加标签

**执行路径**：`proposal → pending_action → Rust LabelService.add_to_flight()`

**输入 schema**：

```json
{
  "flight_id": "string (required)",
  "label": "string (required)"
}
```

**所需权限**：`flight:write`

#### 3.3.10 `workflow.start`

**描述**：发起业务流程

**执行路径**：`proposal → Flowable 审批 → Rust WorkflowService.start()`

**输入 schema**：

```json
{
  "workflow_template_id": "string (required)",
  "flight_id": "string|null",
  "context": "object|null"
}
```

**所需权限**：`workflow:start`

---

## 4. 契约规则

### 4.1 每个动作必须包含

| 字段 | 类型 | 描述 |
|---|---|---|
| `ontology_version` | string | 本体版本（如 `v1.0`） |
| `correlation_id` | string | 关联请求 ID（ULID） |
| `object_type` | string | 对象类型 |
| `object_id` | string | 对象 ID |
| `action_name` | string | 动作名称 |
| `arguments` | object | 输入参数 |
| `risk_level` | enum | 风险等级 |
| `required_permissions` | string[] | 所需权限列表 |
| `approval_policy` | string | 审批策略 |

### 4.2 只读动作规则

- 可使用 `ai_query` schema 和 repositories
- 无需创建 pending action
- 响应携带 `evidence` 字段，标明数据来源和时间

### 4.3 写动作规则

- 必须调用 Rust application services 和 repositories
- 禁止 LLM 或 Python 直接 SQL 写入业务表
- 每个写动作必须携带 `before_snapshot` 和 `after_preview`
- 审批执行前重新校验权限、对象版本、约束和幂等键

### 4.4 约束验证

每个写动作的 proposal 必须包含约束验证结果：

```json
{
  "constraint_results": [
    {
      "constraint": "stand_capacity_check",
      "passed": true,
      "details": "Stand S1 has capacity for 2 more flights"
    },
    {
      "constraint": "time_window_overlap",
      "passed": false,
      "details": "Conflicts with dispatch order DO-001 from 14:00-14:30"
    }
  ]
}
```

### 4.5 风险等级定义

| 等级 | 描述 | 默认策略 |
|---|---|---|
| `LOW` | 只读或草稿型动作，无副作用或可自动回滚 | 自动执行或有权限时自动执行 |
| `MEDIUM` | 有局部副作用，需要确认 | 进入 pending approval |
| `HIGH` | 有显著业务影响 | 始终需要人工审批 |
| `CRITICAL` | 影响核心运行或安全 | 始终需要 Flowable 审批 |

---

## 5. 映射到现有 Rust 服务

### 5.1 Flight 相关动作

| Ontology 动作 | Rust Service | Rust Method | 端口/仓储 |
|---|---|---|---|
| `flight.get_context` | `FlightRepository` | `find_by_id` | `FlightRepository` |
| `flight.search` | `FlightRepository` | `search` | `FlightRepository` |
| `flight.update_stand` | `FlightService` | `update_stand` | `FlightRepository.update_partial` |
| `flight.update_delay` | `FlightService` | `update_delay` | `FlightRepository.update_partial` |

### 5.2 Dispatch 相关动作

| Ontology 动作 | Rust Service | Rust Method | 端口/仓储 |
|---|---|---|---|
| `dispatch.get_status` | `DispatchOrderRepository` | `find_by_id` | `DispatchOrderRepository` |
| `dispatch.update_status` | `DispatchService` | `update_status` | `DispatchOrderRepository.update_status` |
| `dispatch.reassign` | `DispatchService` | `reassign` | `DispatchOrderRepository.save` |
| `dispatch.publish` | `DispatchService` | `publish` | `DispatchOrderRepository` |

### 5.3 Anomaly 相关动作

| Ontology 动作 | Rust Service | Rust Method | 端口/仓储 |
|---|---|---|---|
| `anomaly.list_open` | `AnomalyRepository` | `find_by_status` | `AnomalyRepository` |
| `anomaly.acknowledge` | `AnomalyService` | `acknowledge` | `AnomalyRepository.acknowledge` |
| `anomaly.resolve` | `AnomalyService` | `resolve` | `AnomalyRepository.resolve` |

### 5.4 Stand 相关动作

| Ontology 动作 | Rust Service | Rust Method | 端口/仓储 |
|---|---|---|---|
| `stand.check_availability` | `StandRepository` | `find_by_id` | `StandRepository` + `FlightRepository` |

### 5.5 Notification 相关动作

| Ontology 动作 | Rust Service | Rust Method | 端口/仓储 |
|---|---|---|---|
| `notification.send` | `NotificationService` | `send` | `NotificationRepository.save` |
| `notification.suggest_broadcast` | `NotificationService` | `prepare_broadcast` | N/A（仅计算） |

---

## 6. 缺失服务补全计划

以下动作需要新增 Rust service method：

| 动作 | 状态 | 计划 |
|---|---|---|
| `flight.suggest_stand_adjustment` | 缺失 | 新增 `StandRecommendationService` |
| `dispatch.suggest_replan` | 缺失 | 新增 `DispatchReplanAdvisorService` |
| `anomaly.suggest_escalation` | 缺失 | 新增 `AnomalyEscalationAdvisorService` |
| `flight.suggest_delay_action` | 缺失 | 新增 `DelayAdvisorService` |
| `report.generate_briefing` | 部分存在 | 扩展 `DashboardWorkbenchService` |
| `label.add` | 缺失 | 新增 `LabelService.add_to_flight` |

---

## 7. JSON Schema 导出

Ontology V1 schema 可通过以下方式导出给 Python sidecar：

```bash
GET /api/v2/ai/ontology/schema
```

响应结构（与实现一致：`fms_domain::ontology::schema_export::OntologySchemaExport`）：

```json
{
  "ontology_version": "flight-ops.v1",
  "description": "Flight Operations Ontology Schema V1",
  "exported_at": "2026-05-11T00:00:00Z",
  "objects": { "Flight": { "name": "...", "fields": {}, "relations": {}, "actions": {} } },
  "actions": {
    "Flight.change_stand": {
      "ontology_version": "flight-ops.v1",
      "object_type": "Flight",
      "action_name": "change_stand",
      "category": "write",
      "arguments_schema": { "type": "object", "required": ["new_stand_id"] },
      "risk_level": "medium",
      "required_permissions": ["flight:write"],
      "approval_policy": "require_approval",
      "execution_mapping": "DomainActionExecutor.Flight.change_stand"
    }
  },
  "risk_policies": {
    "low": "auto_execute",
    "medium": "require_approval",
    "high": "require_approval",
    "critical": "require_approval"
  },
  "constraints": [
    {
      "object_type": "Flight",
      "action_name": "change_stand",
      "constraint": { "constraint_type": "Precondition", "expression": "...", "description": "..." }
    }
  ]
}
```

序列化规则：

- `exported_at` 为导出时刻（UTC RFC3339）；schema 内容不变时其余字段不变。
- `actions` 键固定为 `{Object}.{action_name}`；`objects` 内仍保留嵌套动作定义。
- 固定 fixture：`docs/fixtures/flight_ops_v1_ontology_schema.json`（键排序、
  确定性序列化），由 Rust `flight_ops_v1_export_matches_fixture` 与
  Python `test_ontology_schema_fixture.py` 双向校验。

---

## 8. 验收标准

- [ ] Ontology V1 已文档化，并可导出 JSON 给 Python sidecar
- [ ] 每个 V1 写动作都能映射到现有 Rust service 或明确计划中的 service method
- [ ] 每个 V1 只读动作都能映射到 repository、read view 或 query service
- [ ] 每个动作都有 JSON schema 定义
- [ ] 每个动作都有风险等级和审批策略
- [ ] 每个 proposal 都包含约束验证结果
