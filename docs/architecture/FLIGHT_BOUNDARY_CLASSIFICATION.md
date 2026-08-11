# Flight Boundary Classification

## 核心运行状态

- 航班身份、资源位、时间状态、航段、版本号
- 状态变迁对象与聚合未提交变更
- 仓储原子保存、变更记录、`domain_event_outbox`

这些职责必须保持在 `domain -> application write model -> infrastructure repository` 主链内。

## 视图派生信息

- 航班详情缓存载荷
- 航班列表载荷
- protobuf / json 输出形状
- SSE / WebSocket 广播消息体

这些职责属于 projection / realtime adapter，不属于核心状态真相源。

## 外围上下文附属信息

- 异常检测触发
- 派工重算
- AI 辅助能力
- 前端页面组合与嵌入式面板

这些职责只能消费航班事实，不能反向定义航班核心模型。

## 补丁逻辑分类矩阵

| 类型 | 典型表现 | 当前边界结论 |
| --- | --- | --- |
| 缓存补丁 | 写后立即刷新详情/列表缓存 | 允许临时存在，但必须经显式写计划驱动 |
| 实时补丁 | 写后直接 SSE / PubSub 广播 | 允许兼容保留，但不得回灌核心状态 |
| 协议补丁 | 路由内 protobuf/json 兼容分支 | 后续迁往 projection / delivery adapter |
| 页面补丁 | 旧 HTML/JS 页面自行拼状态 | 不得继续承接新业务 |
| 迁移补丁 | feature flag / 兼容钩子 | 必须清楚标注并限制在外围 |

## 内核不再承载的职责

- 直接推流
- 直接拼前端载荷
- 判断缓存形状兼容分支
- 根据页面消费方式决定领域行为

## 当前 Phase 1 收口结果

- 写后副作用已通过 `FlightWritePersistencePlan` 和 `FlightWriteSideEffectRequest` 明确化。
- 仓储读模型反序列化不再依赖应用层 mapper。
- 监控实时/告警输出已通过 port + adapter 连接，而不是基础设施直接引用应用交付模块。

