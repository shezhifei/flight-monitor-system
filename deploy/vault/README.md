# Vault CE Bootstrap

This repository uses **Vault Community Edition + AppRole + Vault Agent templates** as the only supported delivery path for long-lived runtime secrets.

## Files

- `deploy/vault/docker-compose.vault.yml`: local Vault CE container
- `deploy/vault/config/vault.hcl`: single-node Raft config
- `deploy/vault/policies/*.hcl`: least-privilege read policies per runtime role
- `deploy/vault/templates/*.ctmpl`: Vault Agent templates rendered before startup
- `deploy/vault/bootstrap.secrets.env.example`: seed file format example
- `deploy/vault/Start-VaultDocker.ps1`: start local Vault container
- `deploy/vault/Stop-VaultDocker.ps1`: stop local Vault container
- `scripts/vault/Initialize-VaultCe.ps1`: initialize/unseal Vault, enable KV/AppRole/audit, write policies, create AppRoles, optionally seed secrets

## Quick Start

1. Start Vault:

```powershell
powershell -ExecutionPolicy Bypass -File deploy/vault/Start-VaultDocker.ps1
```

2. Copy the seed template and replace every placeholder:

```powershell
Copy-Item deploy/vault/bootstrap.secrets.env.example deploy/vault/bootstrap.secrets.env
```

3. Initialize and configure Vault:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/vault/Initialize-VaultCe.ps1
```

The bootstrap script prefers a local `vault` CLI when available. If the CLI is not installed, it automatically falls back to `docker compose exec vault vault ...` against the running Vault container.

4. Start the application entrypoint you need:

- `scripts/fms.ps1 -Command start -Runtime docker`
- `scripts/fms.ps1 -Command start -Runtime host`

For `scripts/fms.ps1 -Command start -Runtime host`, Vault bootstrap now defaults to:

- `local` when the `vault` CLI or `VAULT_AGENT_BINARY` is available
- automatic fallback to `docker` when the CLI is missing but Docker is available

If you need to pin the behavior, set one of these in the bootstrap env file or the current shell:

```powershell
$env:VAULT_BOOTSTRAP_MODE = "docker"
$env:VAULT_AGENT_BINARY = "C:\tools\vault.exe"
```

## Output Artifacts

These files are intentionally gitignored:

- `deploy/vault/.runtime/root-token.txt`
- `deploy/vault/.runtime/unseal-keys.json`
- `deploy/vault/approle/*.role_id`
- `deploy/vault/approle/*.secret_id`
- `deploy/vault/bootstrap.secrets.env`

Treat them as sensitive operational material.

## Secret Paths

- `kv/fms/shared`
- `kv/fms/api`
- `kv/fms/worker`
- `kv/fms/rust-api`
- `kv/fms/flowable`

## AppRoles

- `fms-api`
- `fms-worker`
- `fms-rust-api`
- `fms-ops-bootstrap`

## Rotation

- Rotate AppRole secret IDs with:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/vault/Initialize-VaultCe.ps1 -RotateSecretIds
```

- Update any changed secret values in `deploy/vault/bootstrap.secrets.env`, then rerun the same script.
