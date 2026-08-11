# Contributing

Thanks for your interest in Flight Monitor System. This repository is being
prepared for release under the Apache License, Version 2.0.

## Development Baseline

- Use Windows PowerShell commands when following repository-local scripts.
- Keep changes aligned with the existing layered architecture:
  - `services/api-server/crates/domain/` contains domain models and ports.
  - `services/api-server/crates/application/` contains use-case services.
  - `services/api-server/crates/infrastructure/` contains adapters.
  - `services/api-server/crates/api/` contains HTTP, SSE, and route concerns.
  - `services/api-server/crates/server/` performs application wiring.
- Prefer explicit dependency wiring over import-time side effects or hidden
  global state.
- Do not commit local secrets, runtime state, database dumps, generated
  service credentials, or local build output.

## Verification

Run the checks that match your change:

```powershell
cd services\api-server
cargo test
cargo build --release
```

```powershell
cd frontend\vue-app
npm run typecheck
npm run build
```

```powershell
cd services\mq-gateway
cargo test
```

## Contribution License

Unless explicitly stated otherwise, any contribution intentionally submitted
for inclusion in this project is provided under the Apache License, Version
2.0, without additional terms or conditions.
