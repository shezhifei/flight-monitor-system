# 业务事项事件流架构

## 概述

业务事项 (BusinessCase) 从创建到触发 Flowable 工作流的完整事件流。

## 事件流图

```text
┌──────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│  前端表单    │────▶│ BusinessCase        │────▶│ 保存到数据库     │
│  (人工填写)  │     │ Service             │     │ + 航班聚合根     │
└──────────────┘     └─────────────────────┘     └──────────────────┘
                            │                           │
                            │ (1) 创建业务事项          │
                            ▼                           │
                     ┌─────────────────────┐            │
                     │ BusinessCaseEvent   │◀───────────┘
                     │ Publisher (outbox)  │
                     └─────────────────────┘
                            │
                            │ (2) 写入 domain_event_outbox
                            │     → CDC relay → MessageQueue topic
                            │     （默认 EVENTS_DOMAIN_TOPIC=fms.domain-events / RocketMQ）
                            ▼
                     ┌─────────────────────┐
                     │ DomainEventEnvelope │
                     │ → MessageQueue      │
                     └─────────────────────┘
                            │
                            │ (3) 事件订阅器消费
                            ▼
                     ┌─────────────────────┐
                     │ BusinessCaseEvent   │
                     │ Subscriber          │
                     └─────────────────────┘
                            │
              ┌─────────────┴─────────────┐
              │                           │
              ▼                           ▼
     ┌─────────────────┐         ┌─────────────────┐
     │ FlowableCase    │         │ SSE Hub         │
     │ Trigger         │         │ → 实时通知      │
     │ → 启动工作流    │         └─────────────────┘
     └─────────────────┘
```

写路径也可经 API 路由直接 `broadcast_business_case_event` / best-effort 工作流触发（见 `api/src/routes/business_cases/`）；上图描述 outbox → MQ → subscriber 主链路（与 ADR-0003 一致）。

## 事件类型

| 事件类型 | 触发时机 | 副作用 |
|---------|---------|--------|
| `business_case.created` | 创建业务事项 | 触发 Flowable 工作流、SSE 通知（subscriber +/或写路径直推） |
| `business_case.updated` | 更新业务事项 | SSE 通知（subscriber +/或写路径直推） |
| `business_case.deleted` | 删除业务事项 | SSE 通知（subscriber +/或写路径直推） |
| `business_case.appended` | 追加业务事项评论 | 写入 `domain_event_outbox`（`OutboxBusinessCaseEventPublisher`）；**不**经 subscriber SSE（`is_business_case_event_type` 不含此类型；`SseBusinessCaseEventPublisher::publish_appended` 为 no-op） |

## 关键组件

### BusinessCaseEventPublisher

位置：`services/api-server/crates/application/src/services/business_case_service/schemas.rs`（`BusinessCaseEventPublisher` trait），实现于 `services/api-server/crates/server/src/di/adapters.rs`（`OutboxBusinessCaseEventPublisher` / `SseBusinessCaseEventPublisher`）。

职责：
- 将业务事项操作写入 `domain_event_outbox`（outbox 实现）或直推 SSE（SSE 实现，仅 updated 等；appended 为 no-op）
- 总线侧经 CDC relay 投递到 `MessageQueue` topic（默认 `fms.domain-events`），**不是** Redis Stream（见 ADR-0003）

### BusinessCaseEventSubscriber

位置：`services/api-server/crates/application/src/services/domain_event_subscriber_service.rs`（`BusinessCaseEventSubscriber`）

职责：
- 仅处理 `business_case.created` / `updated` / `deleted`（`BUSINESS_CASE_EVENT_TYPES` / `is_business_case_event_type`；**不含** `business_case.appended`）
- 触发 Flowable 工作流（`FlowableBusinessCaseWorkflowTrigger`，created）
- 经 `BusinessCaseEventNotifier`（装配为 `SseBusinessCaseEventPublisher`）发布 SSE 实时通知

## 与航班事件的对比

| 维度 | 航班事件 | 业务事项事件 |
|------|---------|-------------|
| 事件类型 | `flight.*_v2` | `business_case.*` |
| 用途 | 状态回放、异常检测 | 工作流触发、通知 |
| 存储 | `domain_event_outbox`（`event_stream_versions` 仅存于 SQL/迁移，**Rust 未使用**） | `domain_event_outbox` |
| 总线 | MessageQueue topic（默认 `fms.domain-events`） | 同左 |
| 订阅器 | AnomalyDetectionService 等 | BusinessCaseEventSubscriber（created/updated/deleted） |
