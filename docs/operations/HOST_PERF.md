# 宿主机全栈压测与数据库超参

文档基线：**2026-08-28**。目标：在 Host runtime 上用混合读写模拟 AOC 控制台流量，验收 **5 万 QPS、端到端 p99 ≤ 100ms、全栈 Working Set ≤ 3GiB**。

不要用健康检查 QPS 宣称业务容量。本页的混合场景走认证、防重放签名、航班列表、派工单和真实写路径。

## 1. 入口

| 用途 | 位置 |
|---|---|
| 混合读写压测客户端 | `services/api-server/crates/api/src/bin/mixed_qps_client.rs` |
| 单接口矩阵（旧） | `scripts/perf/run_host_qps_matrix.ps1`、`qps_load_client` |
| 场景定义 | `scripts/perf/scenarios/airport_ops.json`（读写约 90/10） |
| PostgreSQL 超参 | `scripts/perf/tune_postgres.py` |
| 应用 3GiB 档案 | `scripts/perf/apply_host_perf_profile.ps1` |
| 全栈内存采样 | `scripts/perf/collect_host_stack_memory.ps1` |
| 验收编排 | `scripts/perf/run_host_mixed_qps.ps1` |
| 示例 conf | `deploy/postgresql/postgresql-host-perf.conf.example` |
| 压测 Caddy | `deploy/caddy/Caddyfile.host-perf` |

## 2. 先调 PostgreSQL，再压测

```powershell
.\scripts\fms.ps1 -Command start -Runtime host
.\.venv\Scripts\python.exe scripts\perf\tune_postgres.py --dry-run --stack-memory-mb 3072
.\scripts\perf\apply_host_perf_profile.ps1 -StackMemoryMb 3072 -ApplyPostgres
```

`tune_postgres.py` 按 3GiB **全栈**预算给 PostgreSQL 留份额（API / Redis / Caddy / Vault / RocketMQ / mq-gateway 先扣），再算 `shared_buffers`、`work_mem`、`max_connections`、WAL 与 SSD `random_page_cost`。`--apply` 走 `ALTER SYSTEM` + `pg_reload_conf()`；`shared_buffers`、`max_connections`、`shared_preload_libraries` 要重启 Postgres。`--iterate` 读 `pg_stat_database` / `pg_stat_bgwriter`，检查点过密时加大 `max_wal_size`，出现临时文件时加大 `work_mem`。

`--low-latency-writes` 会把 `synchronous_commit` 设为 `off`（掉电可能丢最近一批提交）。默认保持 `on`。

把 `.tmp/perf/host-perf.env` 里的变量注入 API 进程后重启 `fms-server`。其中 `ANTI_REPLAY_STORE=local` 只适用于**单实例** Host：GET/POST 仍校验 HMAC 与 nonce 唯一性，但不每请求写 Redis。多实例必须 `ANTI_REPLAY_STORE=redis`（默认）。

## 3. 跑 5 万 QPS 混合场景

```powershell
$env:FMS_PERF_PASSWORD = "<login password>"
.\scripts\perf\run_host_mixed_qps.ps1 `
  -ApiBaseUrl https://localhost:18443 `
  -Insecure `
  -DurationSec 30 `
  -Concurrency 768 `
  -TargetQps 50000 `
  -MaxP99Ms 100 `
  -MaxStackMemoryMb 3072

对比未压缩：加 `-NoGzip`（客户端不发 `Accept-Encoding: gzip`）。
```

门槛（`gate.json`）：

| 指标 | 门槛 |
|---|---|
| 混合 QPS | ≥ 50_000 |
| 端到端 p99 | ≤ 100ms |
| 全栈 Working Set | ≤ 3072MB（postgres / redis / vault / caddy / fms-server / rocketmq-* / fms-mq-gateway） |
| 非成功 + 传输错误 | ≤ 1% |

结果在 `.tmp/host-mixed-qps/results/<timestamp>/`。

gzip 对照（Caddy HTTPS `localhost:18443`，`airport_ops`，20s；未压缩基线是同一客户端、同一栈）：

| 条件 | QPS | p99 | 均包 | 线上 |
|---|---|---|---|---|
| 未压缩 c=64 | 15_195 | 36ms | 26.7KB | ~3.3 Gbps |
| gzip-1 c=64 | 16_226 | 35ms | 3.1KB | 400 Mbps |
| gzip-1 c=128 | 16_017 | 79ms | 3.1KB | 395 Mbps |
| gzip-1 c=256 | 2_226 | timeout | — | — |

JSON gzip 把均包缩小约 8.7 倍，QPS 几乎不动，所以当前混合场景不是客户端带宽瓶颈。5 万 QPS 门槛仍未达到。

分层直连 `http://127.0.0.1:8000`、c=64、8s（Caddy 不在路径上）：

| 场景 | QPS | p50 | p99 | fms-server CPU | 均包 | stdout |
|---|---|---|---|---|---|---|
| ping | 53_132 | 1.2ms | 1.9ms | 4.3c | 65B | +112MB / 8s |
| flights_list（1s 缓存，Logger 排除） | 47_175 | 1.3ms | 1.8ms | 5.4c | 44KB | +0.08MB |
| auth_me | 35_753 | 1.6ms | 3.5ms | 7.7c | 2.6KB | +75MB |
| notifications/unread-count | 33_693 | 1.9ms | 2.4ms | 15.7c | 18B | +75MB |
| todo_create | 22_551 | 1.3ms | 2.7ms | 10c | 0.5KB | +35MB |
| todos_list | 28_173 | 2.2ms | 2.8ms | 14c | 9.8KB | +62MB |
| monitor-rows | 16_517 | 3.8ms | 4.5ms | 14c | 16KB | +39MB |
| **dispatch-orders** | **1_936** | **33ms** | **37ms** | 15c | 43KB | +5MB |
| mixed 只读 | 21_101 | 0.4ms | 31ms | 15c | 28KB | +22MB |
| mixed 含写入 | 21_172 | 0.5ms | 31ms | 15c | 27KB | +24MB |

`dispatch-orders` 只占混合权重 5%，但优化前 p50 约 33ms，占满 64 个客户端连接里大约一半的等待时间；用 Little 定律算出来的混合 QPS ≈ 64 / 3.3ms ≈ 2 万，和当时实测 2.1 万一致。

批量回执 + 1s 列表缓存 + 关掉 API access log 之后（直连 `:8000`，c=64，8s）：

| 场景 | 优化前 QPS | 优化后 QPS | 优化后 p99 |
|---|---|---|---|
| dispatch-orders | 1_936 | 47_718 | 1.7ms |
| monitor-rows | 16_517 | 48_048 | 1.6ms |
| unread-count | 33_693 | 48_687 | 1.6ms |
| mixed 含写入 | 21_172 | **45_859** | **2.1ms** |

写入（`todo_create`）几乎不拉低混合 QPS。Caddy HTTPS 仍会再削一截（优化前约 2.1 万 → 1.6 万）。

## 4. 热路径上与容量有关的行为

- 航班列表：进程内 1s 响应缓存（`page=1&page_size=20`）+ `FlightService` hot list。
- 监控宽表：`migrations/158_idx_flight_monitor_rows_active_sort.sql` 部分索引；默认 `page=1&page_size=20` 无筛选时 1s 预序列化缓存。
- 派工单列表：回执汇总一次 `ANY($1)` 批量查询，不再按单 N+1。admin、无筛选、`page=1&page_size=20` 再套 1s 响应缓存（按部门隔离，非 admin 不走这条缓存）。
- 未读数：`NOTIFICATION_UNREAD_CACHE_TTL_MS`（默认 2000）按 user_id 短缓存；已读会失效。
- JWT claims / freshness / permission version：短 TTL 缓存，避免每请求打库。
- Anti-replay session secret：DashMap，读路径无全局互斥。
- 本地 nonce store：分片 DashMap，单实例 Host 压测避免 Redis RTT。
- HTTP access log：`FMS_HTTP_ACCESS_LOG=0|off|false|none` 关闭 Actix `Logger`（host-perf 档案默认关）。未设置时仍记录，只排除 `/api/v2/flights`。
- Caddy：`/api/*` 对 JSON 做 gzip level 1（`Accept-Encoding: gzip` 时压缩，`minimum_length 256`）；静态页仍 `zstd gzip`。压测客户端默认发 gzip，summary 里有 `gzip_responses` / `avg_bytes` / `mbps`，用来判断是不是线上带宽在卡 QPS。关闭：`--gzip false`。

连接池不要盲目加大：3GiB 预算下 API `DB_POOL_MAX_CONNECTIONS=24`、Postgres `max_connections=64`。池超过 64 仍由 `.git_hooks/pre-commit-performance-check.sh` 拦截。
