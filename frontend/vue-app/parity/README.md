# Vue / Legacy Strict Parity Assets

This directory contains tracked, deterministic parity evidence derived from the tracked legacy frontend subset at `frontend/legacy/`. The retired `frontend/backup/legacy-frontend-archive` copy remains outside version control and is never served by a production route.

## Legacy source location

Parity scripts resolve the legacy root in this order:

1. `FMS_LEGACY_FRONTEND_ROOT`
2. `frontend/legacy` relative to the repository

Windows example:

```powershell
$env:FMS_LEGACY_FRONTEND_ROOT = 'C:\flight-monitor-system\frontend\legacy'
```

Validate the legacy root before extracting contracts or refreshing screenshots:

```powershell
cd frontend\vue-app
npm run parity:check-legacy-root
```

Validation is intentionally strict. It requires the expected asset directories and the exact 21-page HTML inventory. Missing or unexpected HTML pages fail with a non-zero exit code so a partial or changed source tree cannot silently become the baseline.

## Local legacy server

Start the loopback-only server with:

```powershell
npm run parity:serve-legacy
```

The server listens on `http://127.0.0.1:3100` and exposes only allow-listed legacy roots under `/frontend/`. It is intended for Playwright baseline capture and must not be wired into Rust, reverse-proxy, container, or production configuration.

## Fixture policy

- Shared authentication and clock fixtures live in `fixtures/common/`.
- Page-owned API and state fixtures live in `fixtures/pages/<page>/`.
- Every production page directory contains a `manifest.json`. `awaiting-contract-capture` is an explicit pending state, not parity evidence; task-specific fixtures replace that state as contracts are captured.
- Fixtures use current Rust DTO field names and are shared by legacy capture and Vue verification.
- Unregistered `/api/v2/*` requests fail fixture-mode tests.
- SSE fixtures use explicit named events and valid event-stream framing.

Derived contracts and approved screenshots will be committed in later parity phases. Refreshing legacy evidence must always be an explicit action against a validated local root.
