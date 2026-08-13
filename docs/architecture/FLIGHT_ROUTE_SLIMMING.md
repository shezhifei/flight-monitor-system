# Flight Route Slimming

## 已完成的瘦身

- `create_flight` 不再直接执行 payload mapping 和服务参数展开
- `update_flight` 不再直接执行字段级策略判定和写调用编排
- create/update 两条写路径统一进入 `FlightCommandGateway`

## 路由保留职责

- 认证与权限前置校验
- 构造 `FlightCommandContext`
- 将应用层异常映射为 HTTP 状态码
- 响应模型封装

## 已移出路由的职责

- payload 到 domain 写输入的映射
- 字段级策略判定与记录
- 写后主链结果整合

## 后续承接点

- 进一步移出 create 路径中的 profile header 组装
- 将更多缓存/实时派生信息从 route-facing 层完全迁到 projection / adapter 层

