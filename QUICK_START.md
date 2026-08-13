# 快速开始

文档基线：**2026-08-11**。默认用 Docker 标准拓扑或 Host Rust 本机联调；不要把 Python HTTP API 当作启动路径。

## 1. Docker 标准拓扑

### 前置

- Windows 10/11，Docker Desktop
- 内存建议 16 GB+（24 GB 更稳）
- 已做 Vault bootstrap，或准备好 `deploy/vault/bootstrap.secrets.env` 让启动脚本初始化本地密钥

### 启动

```powershell
.\scripts\fms.ps1 -Command start -Runtime docker
```

也可双击：`deploy/docker/Start-FlightMonitorDocker.bat`

脚本大致会：检查 Docker → 准备 `deploy/docker/.env.local` → Vault Agent 渲染密钥 → 构建/复用镜像 → 拉起拓扑 → 启动宿主机 Caddy → 等待 Rust API 与 Flowable 就绪。

默认相关服务：`postgres`、`redis`、`rocketmq-namesrv`、`rocketmq-broker`、`mq-gateway`、`rust-api`、`flowable`、Vault 相关服务，以及宿主机 Caddy。新的 HTTP 行为应落在 `services/api-server/`。

### 验证

- `https://localhost:18443/api/v2/health/ping`
- `https://localhost:18443/frontend/login.html`
- `https://localhost:18443/frontend/dashboard.html`
- `http://localhost:8082/flowable-rest/service/management/engine`

```powershell
docker compose -f deploy\docker\docker-compose.distributed.yml ps
.\scripts\fms.ps1 -Command logs -Runtime docker
.\scripts\fms.ps1 -Command stop -Runtime docker
```

## 2. Host Rust 本机联调

适合断点调试和宿主机联调：

```powershell
.\scripts\fms.ps1 -Command start -Runtime host
```

常见行为：加载 `.env` 与 Vault 渲染 env；检测 PostgreSQL；按需起 Redis / Vault / Tomcat·Flowable / Caddy；默认迁移（bootstrap + `sqlx migrate run`）；默认 `cargo build --release` 后后台起 `fms-server.exe`。

参数：

| 参数 | 作用 |
|---|---|
| `-SkipBuild` | 用已有二进制 |
| `-UseCargoRun` | 前台 `cargo run --release` |
| `-SkipMigrations` | 跳过迁移 |

依赖：本机 PostgreSQL、Java、Rust toolchain，以及仓库内 `vault.exe`、`redis-server.exe`、`tomcat/`。

```powershell
.\scripts\fms.ps1 -Command status -Runtime host
.\scripts\fms.ps1 -Command logs   -Runtime host
.\scripts\fms.ps1 -Command stop   -Runtime host
```

组件日志：`.runtime/host-services/{service}/`。PostgreSQL 若是系统服务，host stop 不会自动关掉。

## 3. 边缘部署

```powershell
.\deploy\docker\Start-FlightMonitorDocker-Edge.bat
```

典型服务：`nginx`、`rust-api`、`postgres`、`redis`。验证：`http://localhost:18080/api/v2/health/ping`、`http://localhost:18080/frontend/login.html`。停止用 `Stop-FlightMonitorDocker-Edge.bat`。

## 4. Python AI 侧车

```powershell
.\.venv\Scripts\python.exe scripts\host\ai_sidecar_entrypoint.py
```

依赖示例（按需）：

```powershell
.\.venv\Scripts\pip.exe install -r legacy-backend\requirements.txt
```

历史完整 Python 后端在 `legacy-backend/`（本地归档）。

## 5. 数据库

Host 模式默认会迁移；仅 Docker 外挂库或手动管理时用本节。

```powershell
psql -U postgres -f scripts\database\setup_postgresql.sql
sqlx migrate run --database-url $env:DATABASE_URL
psql -v ON_ERROR_STOP=1 --file scripts\database\verify_runtime_schema.sql $env:DATABASE_URL
```

- 最新迁移：`121_add_soft_delete_columns.sql`（`120` 移除全部外键，`121` 加软删除列；产品删除统一软删，引用完整性由应用层 + `scripts/database/check_referential_integrity.sql` 巡检保证）
- 空库可直接 `sqlx migrate run --source migrations`
- `CREATE INDEX CONCURRENTLY` 必须单独文件，首行 `-- no-transaction`（见 107–112 一类迁移）

## 6. 前端

```powershell
cd frontend\vue-app
npm install
npm run typecheck
npm run build
```

正式页面：`frontend/vue-app/dist/` → `/frontend/<page>.html`（如 login、dashboard、flight_monitor、dispatch_board、system_status）。兼容旧页：`/frontend/html/<page>.html`。根路径 `/` 跳到 `/frontend/login.html`。

## 7. 常用命令

```powershell
.\scripts\fms.ps1 -Command start|status|logs|stop|restart -Runtime docker
.\scripts\fms.ps1 -Command start -Runtime host -SkipBuild
cd services\api-server; cargo test
cd frontend\vue-app; npm run typecheck; npm run build
```

## 8. 常见问题

**Docker 起不来**  
Docker Desktop 是否在跑；`docker compose version`；重跑 start 让脚本补 bootstrap。

**Vault 渲染失败**  
查 `VAULT_ADDR`、role/secret id 文件路径；首次本机按 `docs/DEPLOYMENT.md` 准备 `bootstrap.secrets.env`。

**登录页 404**  
标准：`https://localhost:18443/frontend/login.html`；边缘：`http://localhost:18080/...`。

**健康检查 404**  
标准用 18443 的 `/api/v2/health/ping`；直连 Rust 时核对监听端口（Docker 常映射 `127.0.0.1:18080`）。

**前端没更新**  
确认 `npm run build`；访问的是 `/frontend/<page>.html` 不是 html 兼容路径；容器内要重建镜像才吃到新 dist。

**Python 报错**  
用 `.\.venv\Scripts\python.exe`，依赖装进该 venv。

## 9. 下一步

`README.md` → `docs/DEPLOYMENT.md` → `docs/SYSTEM_MANUAL.md` → `docs/API_ROUTE_SNAPSHOT.md` → `docs/SOURCE_OF_TRUTH.md`
