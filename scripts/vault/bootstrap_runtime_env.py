"""Render Vault secrets and build the final runtime env file."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from scripts.vault.build_runtime_env import parse_env_file


def _resolve_path(raw_path: str, *, cwd: Path) -> str:
    path = Path(raw_path)
    if path.is_absolute():
        return str(path)
    return str((cwd / path).resolve())


def _load_base_env(base_env_path: Path) -> dict[str, str]:
    values = parse_env_file(base_env_path)
    for key, value in values.items():
        os.environ[key] = value
    return values


def _require_value(values: dict[str, str], key: str) -> str:
    value = str(values.get(key, "") or "").strip()
    if not value:
        raise RuntimeError(f"{key} must be set in the bootstrap env file")
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description="Bootstrap a runtime env file from Vault")
    parser.add_argument("--base-env", required=True, help="Base non-secret env file")
    parser.add_argument("--template", required=True, help="Vault Agent template file")
    parser.add_argument("--runtime-env", required=True, help="Merged runtime env output")
    parser.add_argument("--rendered-env", help="Rendered secret env output")
    parser.add_argument("--agent-config", help="Vault Agent config output")
    parser.add_argument("--mode", choices=["local", "docker"], default="local")
    args = parser.parse_args()

    repo_root = Path.cwd()
    base_env_path = Path(args.base_env).resolve()
    base_values = _load_base_env(base_env_path)

    vault_addr = _require_value(base_values, "VAULT_ADDR")
    role_id_file = _resolve_path(_require_value(base_values, "VAULT_ROLE_ID_FILE"), cwd=repo_root)
    secret_id_file = _resolve_path(_require_value(base_values, "VAULT_SECRET_ID_FILE"), cwd=repo_root)
    rendered_env = Path(args.rendered_env or _require_value(base_values, "VAULT_RENDERED_ENV_FILE"))
    if not rendered_env.is_absolute():
        rendered_env = (repo_root / rendered_env).resolve()
    runtime_env = Path(args.runtime_env)
    if not runtime_env.is_absolute():
        runtime_env = (repo_root / runtime_env).resolve()

    default_agent_config = base_values.get("VAULT_AGENT_CONFIG", "").strip()
    agent_config_raw = args.agent_config or default_agent_config
    if not agent_config_raw:
        raise RuntimeError("VAULT_AGENT_CONFIG must be set in the bootstrap env file")
    agent_config = Path(_resolve_path(agent_config_raw, cwd=repo_root))

    env = os.environ.copy()
    env["VAULT_ADDR"] = vault_addr
    env["VAULT_ROLE_ID_FILE"] = role_id_file
    env["VAULT_SECRET_ID_FILE"] = secret_id_file
    env["VAULT_RENDERED_ENV_FILE"] = str(rendered_env)
    env["VAULT_AGENT_CONFIG"] = str(agent_config)

    render_command = [
        sys.executable,
        "scripts/vault/render_vault_env.py",
        "--template",
        str(Path(args.template).resolve()),
        "--output",
        str(rendered_env),
        "--mode",
        args.mode,
        "--role-id-file",
        role_id_file,
        "--secret-id-file",
        secret_id_file,
        "--agent-config",
        str(agent_config),
        "--vault-addr",
        vault_addr,
    ]
    rendered_env.parent.mkdir(parents=True, exist_ok=True)
    agent_config.parent.mkdir(parents=True, exist_ok=True)
    render_result = subprocess.run(render_command, check=False, capture_output=True, text=True, env=env)
    if render_result.returncode != 0:
        raise RuntimeError(
            "Vault render failed: "
            f"{(render_result.stdout or '').strip()} {(render_result.stderr or '').strip()}".strip()
        )

    build_command = [
        sys.executable,
        "scripts/vault/build_runtime_env.py",
        "--base-env",
        str(base_env_path),
        "--rendered-env",
        str(rendered_env),
        "--output",
        str(runtime_env),
    ]
    build_result = subprocess.run(build_command, check=False, capture_output=True, text=True, env=env)
    if build_result.returncode != 0:
        raise RuntimeError(
            "Runtime env build failed: "
            f"{(build_result.stdout or '').strip()} {(build_result.stderr or '').strip()}".strip()
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
