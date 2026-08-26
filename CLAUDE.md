# CLAUDE.md

Guidance for coding agents working in this repository.

## What this is

Flight Monitor System — airport flight operations: master data, dispatch, auth, anomalies,
AI tool execution, mobile work, realtime events.

- **Rust** (`services/api-server/`) is the default HTTP backend.
- **Python** is only for the AI sidecar (and optional worker/runtime).
- Wire dependencies at composition time; no import-time singletons.
- Secrets: Vault CE + AppRole + Vault Agent rendered files — do not commit long-lived secrets.

```
Browser / Vue MPA -> Caddy/Nginx edge -> Rust API (Actix-web)
  -> PostgreSQL / Redis / RocketMQ gateway / Flowable（嵌入式引擎，api-server 进程内）
  -> Python AI sidecar (FastAPI; tools, NL Query, LLM eval)
```

## Commands

Windows / PowerShell throughout.

### Whole system

```powershell
.\scripts\fms.ps1 -Command start -Runtime docker
.\scripts\fms.ps1 -Command stop  -Runtime docker
.\scripts\fms.ps1 -Command logs  -Runtime docker
.\scripts\fms.ps1 -Command start -Runtime host
```

Runtimes: `docker`, `host`, `edge`.  
Check: `https://localhost:18443/api/v2/health/ping`, `https://localhost:18443/frontend/login.html`.

### Rust (`services/api-server/`)

```powershell
cd services\api-server
cargo test
cargo test -p api routes::ai
cargo build --release
```

MQ gateway is separate: `cd services\mq-gateway; cargo test`.

### Python AI sidecar

Always use the repo venv:

```powershell
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar
.\.venv\Scripts\python.exe -m pytest services/ai-sidecar/tests/sidecar/test_runtime_service.py::test_name
```

`conftest.py` puts `services/ai-sidecar` on `sys.path`.  
Entrypoint: `scripts/host/ai_sidecar_entrypoint.py`.

### Vue (`frontend/vue-app/`)

```powershell
cd frontend\vue-app
npm install
npm run typecheck
npm run test
npm run build
npm run dev
```

Built pages are served at `/frontend/<page>.html`.

## Layout

### Rust API (`services/api-server/crates/`)

| Crate | Role |
|---|---|
| `domain/` | models + ports |
| `application/` | use cases + schemas |
| `infrastructure/` | Postgres, Redis, MQ, adapters |
| `api/` | HTTP handlers, SSE, routes under `routes/` |
| `server/` | process entry + wiring |

AI HTTP surface is split across `ai_*` route modules. Model/tool work is forwarded to the Python sidecar from `routes/ai/` (there is no separate mounted `ai-proxy` module).

### Python sidecar (`services/ai-sidecar/src/`)

Same layering idea (`domain/ai/`, `application/services/ai/`, `infrastructure/ai/`) with DI in `src/di/container.py`. Active code is mostly under `infrastructure/ai/` (config v2, LLM stream, tools, MCP, skills, cache, runtime). Full historical Python backend: `legacy-backend/` (gitignored).

### Frontend

Vue 3 + Vite + TypeScript multi-page app. Primary pages from `frontend/vue-app/dist/` at `/frontend/<page>.html`. Legacy static pages under `/frontend/html/<page>.html` are compatibility only.

Visual language (ops console only): `docs/architecture/SIGNAL_SURFACE.md`. Specimen: `frontend/signal-surface-preview.html`. Read both before changing UI. Do not treat it as a generic design system.

## Migrations

SQL files in `migrations/`, numeric order. Next number only; never renumber.  
Latest at time of writing: `137_*`.

## Conventions

- Prefer ports/adapters; do not reach across layers.
- Do not commit secrets, runtime state, DB dumps, or generated credentials.
- Local agent dirs (`.agents/`, `.claude/`, `.codex/`, `.opencode/`, `.shared/`, …) and most plan drafts stay out of git — see `.gitignore`.

## Docs baseline

- `README.md`, `QUICK_START.md`
- `docs/SYSTEM_MANUAL.md`, `docs/DEPLOYMENT.md`
- `docs/API_ROUTE_SNAPSHOT.md`, `docs/SOURCE_OF_TRUTH.md`
- `docs/DOCUMENTATION_WORKFLOW.md`, `docs/GLOSSARY.md`
- `docs/architecture/*`, `docs/observability/*`
