# Dispatch Collaboration Event Mapping

本文件定义派工协同账本 `dispatch_collaboration_events` 与两个读模型之间的固定映射关系：

- 统一协同视图 `/api/v2/dispatch/collaboration/...`
- 工单时间线 `/api/v2/dispatch-orders/{order_id}/timeline`

目标不是重复描述数据库字段，而是锁定**哪些事件是协同主语义**、**哪些事件允许投影到工单时间线**、以及**哪些事件绝不能混入派工列表或工单时间线**。

## 1. 总原则

- 协同账本是追加式审计主轴，记录跨工单、群聊、通知的统一协作事实
- 统一协同视图直接消费账本事件，可同时呈现工单、消息、通知与群摘要
- 工单时间线只消费“工单状态迁移语义”，不消费聊天与通知事件
- 派工列表 `/api/v2/dispatch-orders` 只返回工单投影，不混入协同消息、通知摘要、统一事件流

## 2. 事件类型分层

### 2.1 工单事件

这些事件属于工单业务事实，可进入统一协同视图，也允许投影为工单时间线动作：

| 事件类型 | 统一协同视图 | 工单时间线动作 |
| --- | --- | --- |
| `order_created` | 显示 | `created` |
| `order_accepted` | 显示 | `accepted` |
| `order_started` | 显示 | `started` |
| `order_completed` | 显示 | `completed` |
| `order_cancelled` | 显示 | `cancelled` |
| `order_checked_in` | 显示 | `checked_in` |
| `order_issue_reported` | 显示 | `issue_reported` |
| `order_replanned` | 显示 | `replanned` |

### 2.2 群聊事件

这些事件属于协作事实，只进入统一协同视图，不进入工单时间线：

| 事件类型 | 统一协同视图 | 工单时间线 |
| --- | --- | --- |
| `group_upserted` | 显示 | 不投影 |
| `group_member_synced` | 显示 | 不投影 |
| `message_sent` | 显示 | 不投影 |
| `group_read_synced` | 显示 | 不投影 |
| `group_archived` | 显示 | 不投影 |

### 2.3 通知事件

这些事件属于触达事实，只进入统一协同视图，不进入工单时间线：

| 事件类型 | 统一协同视图 | 工单时间线 |
| --- | --- | --- |
| `notification_created` | 显示 | 不投影 |
| `notification_delivered` | 显示 | 不投影 |
| `notification_acknowledged` | 显示 | 不投影 |

## 3. 工单时间线投影规则

工单时间线是派工产品面使用的状态历程读模型；统一协同视图则覆盖工单、群聊和通知。两者消费同一协同账本，但承担不同展示契约。

投影规则固定如下：

- 输入源：`DispatchCollaborationQueryService.get_order_timeline(...)`
- 输入范围：仅按 `dispatch_order_id` 读取账本事件
- 允许投影：仅 `order_*` 事件中明确列入白名单的工单事件
- 输出字段：
  - `id` ← `event_id` 或 `source_record_id`
  - `action` ← 映射表中的兼容动作名
  - `actor_id` ← `actor_user_id`
  - `actor_username` ← `payload.actor_username` 或 `payload.username`
  - `details` ← `payload` 去掉用户名辅助字段后的剩余内容
  - `created_at` ← `occurred_at`

不允许：

- 将 `message_sent` 混入工单时间线
- 将 `notification_*` 混入工单时间线
- 在工单时间线里重新拼装群摘要、未读数、通知状态

## 4. 统一协同视图字段责任

统一协同视图承担以下聚合责任：

- 工单摘要：来自工单投影/工单事实
- 群摘要：来自群投影
- 最近消息：来自消息投影或账本筛选
- 最近通知：来自通知投影或账本筛选
- 统一事件流：直接来自协同账本

这意味着新增协同摘要字段时，应优先修改统一协同查询服务，而不是回写工单时间线路由或群聊路由。

## 5. 防回退约束

- 若新增协同事件类型，必须先决定它属于工单、群聊还是通知语义
- 若不是工单状态迁移语义，不得投影到工单时间线
- 若需要新前端消费协同信息，默认进入 `/api/v2/dispatch/collaboration/...`
- `/api/v2/dispatch-orders/{order_id}/timeline` 只扩展工单状态历程，不承载新的跨域聚合编排
