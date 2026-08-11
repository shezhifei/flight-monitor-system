"""Render runtime env files from Vault Agent templates."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import Iterable
from urllib.parse import urlparse, urlunparse


DEFAULT_DOCKER_IMAGE = "hashicorp/vault:1.21"
DEFAULT_AGENT_TIMEOUT_SECONDS = 30


def _env(name: str, default: str = "") -> str:
    return str(os.environ.get(name, default) or "").strip()


def _agent_timeout_seconds() -> int:
    raw_value = _env("VAULT_AGENT_TIMEOUT_SECONDS", str(DEFAULT_AGENT_TIMEOUT_SECONDS))
    try:
        parsed = int(raw_value)
    except ValueError as exc:
        raise RuntimeError(f"VAULT_AGENT_TIMEOUT_SECONDS must be an integer. Actual: {raw_value}") from exc
    if parsed <= 0:
        raise RuntimeError("VAULT_AGENT_TIMEOUT_SECONDS must be greater than 0.")
    return parsed


def _ensure_file(path: Path, description: str) -> Path:
    if not path.exists():
        raise FileNotFoundError(f"{description} not found: {path}")
    if not path.is_file():
        raise ValueError(f"{description} must be a file: {path}")
    return path


def _rewrite_vault_addr_for_docker(vault_addr: str) -> str:
    parsed = urlparse(vault_addr)
    hostname = (parsed.hostname or "").strip().lower()
    if hostname in {"127.0.0.1", "localhost", "::1"}:
        return urlunparse(parsed._replace(netloc=parsed.netloc.replace(parsed.hostname or "", "host.docker.internal")))
    return vault_addr


def _render_agent_config(
    *,
    vault_addr: str,
    role_id_file: Path,
    secret_id_file: Path,
    template_file: Path,
    output_file: Path,
    token_sink_file: Path,
) -> str:
    return f"""
vault {{
  address = "{vault_addr}"
}}

auto_auth {{
  method "approle" {{
    mount_path = "auth/approle"
    config = {{
      role_id_file_path = "{role_id_file.as_posix()}"
      secret_id_file_path = "{secret_id_file.as_posix()}"
      remove_secret_id_file_after_reading = false
    }}
  }}

  sink "file" {{
    config = {{
      path = "{token_sink_file.as_posix()}"
    }}
  }}
}}

template {{
  source      = "{template_file.as_posix()}"
  destination = "{output_file.as_posix()}"
  perms       = "0600"
  error_on_missing_key = true
}}

exit_after_auth = true
pid_file = "{(output_file.parent / 'vault-agent.pid').as_posix()}"
""".strip()


def _render_docker_agent_config(*, vault_addr: str, output_filename: str) -> str:
    return f"""
vault {{
  address = "{vault_addr}"
}}

auto_auth {{
  method "approle" {{
    mount_path = "auth/approle"
    config = {{
      role_id_file_path = "/render/auth/role_id"
      secret_id_file_path = "/render/auth/secret_id"
      remove_secret_id_file_after_reading = false
    }}
  }}

  sink "file" {{
    config = {{
      path = "/render/output/.vault-token"
    }}
  }}
}}

template {{
  source      = "/render/templates/runtime.ctmpl"
  destination = "/render/output/{output_filename}"
  perms       = "0600"
  error_on_missing_key = true
}}

exit_after_auth = true
pid_file = "/render/output/vault-agent.pid"
""".strip()


def _run_local_agent(agent_binary: str, config_file: Path) -> None:
    timeout_seconds = _agent_timeout_seconds()
    try:
        completed = subprocess.run(
            [agent_binary, "agent", "-config", str(config_file)],
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(
            f"Vault agent render timed out after {timeout_seconds}s. "
            "Verify that Vault is reachable at VAULT_ADDR and that it is initialized and unsealed."
        ) from exc
    if completed.returncode != 0:
        raise RuntimeError(
            f"Vault agent render failed with exit code {completed.returncode}: "
            f"{(completed.stdout or '').strip()} {(completed.stderr or '').strip()}".strip()
        )


def _mount_arg(host_path: Path, container_path: str) -> list[str]:
    return ["-v", f"{host_path.resolve()}:{container_path}"]


def _run_docker_agent(
    *,
    docker_image: str,
    config_file: Path,
    template_file: Path,
    role_id_file: Path,
    secret_id_file: Path,
    output_dir: Path,
) -> None:
    docker_vault_addr = _rewrite_vault_addr_for_docker(_env("VAULT_ADDR"))
    command = [
        "docker",
        "run",
        "--rm",
        "--add-host",
        "host.docker.internal:host-gateway",
        "-e",
        f"VAULT_ADDR={docker_vault_addr}",
        *_mount_arg(config_file, "/render/config/agent.hcl"),
        *_mount_arg(template_file, "/render/templates/runtime.ctmpl"),
        *_mount_arg(role_id_file, "/render/auth/role_id"),
        *_mount_arg(secret_id_file, "/render/auth/secret_id"),
        *_mount_arg(output_dir, "/render/output"),
        docker_image,
        "agent",
        "-config=/render/config/agent.hcl",
    ]
    timeout_seconds = _agent_timeout_seconds()
    try:
        completed = subprocess.run(command, check=False, capture_output=True, text=True, timeout=timeout_seconds)
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(
            f"Docker Vault agent render timed out after {timeout_seconds}s. "
            "Verify that Vault is reachable at VAULT_ADDR and that the AppRole files are correct."
        ) from exc
    if completed.returncode != 0:
        raise RuntimeError(
            f"Docker Vault agent render failed with exit code {completed.returncode}: "
            f"{(completed.stdout or '').strip()} {(completed.stderr or '').strip()}".strip()
        )


def _required_keys(template_name: str) -> list[str]:
    template = template_name.lower()
    if template == "docker-all.env.ctmpl":
        return [
            "DB_PASSWORD",
            "DB_REPLICATION_PASSWORD",
            "REDIS_PASSWORD",
            "JWT_SECRET_KEY",
            "JWT_SECRET",
            "AI_CONFIG_ENCRYPTION_KEY",
            "FLOWABLE_ADMIN_PASSWORD",
            "FLOWABLE_DB_PASSWORD",
            "FLOWABLE_PASSWORD",
        ]
    if template == "api.env.ctmpl":
        return [
            "DB_PASSWORD",
            "DB_REPLICATION_PASSWORD",
            "REDIS_PASSWORD",
            "JWT_SECRET_KEY",
            "AI_CONFIG_ENCRYPTION_KEY",
            "FLOWABLE_ADMIN_PASSWORD",
        ]
    if template == "worker.env.ctmpl":
        return [
            "DB_PASSWORD",
            "DB_REPLICATION_PASSWORD",
            "REDIS_PASSWORD",
            "JWT_SECRET_KEY",
            "AI_CONFIG_ENCRYPTION_KEY",
            "FLOWABLE_ADMIN_PASSWORD",
        ]
    if template == "rust-api.env.ctmpl":
        return [
            "DB_PASSWORD",
            "DB_REPLICATION_PASSWORD",
            "REDIS_PASSWORD",
            "JWT_SECRET_KEY",
            "JWT_SECRET",
            "AI_CONFIG_ENCRYPTION_KEY",
            "FLOWABLE_ADMIN_PASSWORD",
            "FLOWABLE_PASSWORD",
        ]
    return []


def _parse_env_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        values[key.strip()] = value.strip()
    return values


def _validate_rendered_env(output_file: Path, required_keys: Iterable[str]) -> None:
    values = _parse_env_file(output_file)
    missing = [key for key in required_keys if not values.get(key, "").strip()]
    if missing:
        raise RuntimeError(f"Vault rendered env file is missing required keys: {', '.join(missing)}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Render an env file from Vault using Vault Agent templates")
    parser.add_argument("--template", required=True, help="Consul-template source file")
    parser.add_argument("--output", default=_env("VAULT_RENDERED_ENV_FILE"), help="Rendered env file")
    parser.add_argument("--mode", choices=["local", "docker"], default="local")
    parser.add_argument("--vault-addr", default=_env("VAULT_ADDR"))
    parser.add_argument("--role-id-file", default=_env("VAULT_ROLE_ID_FILE"))
    parser.add_argument("--secret-id-file", default=_env("VAULT_SECRET_ID_FILE"))
    parser.add_argument("--agent-config", default=_env("VAULT_AGENT_CONFIG"))
    parser.add_argument("--agent-binary", default=_env("VAULT_AGENT_BINARY", "vault"))
    parser.add_argument("--docker-image", default=_env("VAULT_DOCKER_IMAGE", DEFAULT_DOCKER_IMAGE))
    args = parser.parse_args()

    if not args.vault_addr:
        raise RuntimeError("VAULT_ADDR must be set")
    if not args.output:
        raise RuntimeError("VAULT_RENDERED_ENV_FILE or --output must be set")
    if not args.role_id_file:
        raise RuntimeError("VAULT_ROLE_ID_FILE must be set")
    if not args.secret_id_file:
        raise RuntimeError("VAULT_SECRET_ID_FILE must be set")
    if not args.agent_config:
        raise RuntimeError("VAULT_AGENT_CONFIG must be set")

    template_file = _ensure_file(Path(args.template), "Vault template file")
    role_id_file = _ensure_file(Path(args.role_id_file), "Vault AppRole role_id file")
    secret_id_file = _ensure_file(Path(args.secret_id_file), "Vault AppRole secret_id file")
    output_file = Path(args.output)
    config_file = Path(args.agent_config)

    output_file.parent.mkdir(parents=True, exist_ok=True)
    config_file.parent.mkdir(parents=True, exist_ok=True)
    token_sink_file = output_file.parent / ".vault-token"
    if args.mode == "local":
        config_file.write_text(
            _render_agent_config(
                vault_addr=args.vault_addr,
                role_id_file=role_id_file,
                secret_id_file=secret_id_file,
                template_file=template_file,
                output_file=output_file,
                token_sink_file=token_sink_file,
            ),
            encoding="utf-8",
        )
        if shutil.which(args.agent_binary) is None:
            raise RuntimeError(
                "Vault binary not found on PATH: "
                f"{args.agent_binary}. Install the Vault CLI, set VAULT_AGENT_BINARY to a valid executable path, "
                "or rerun bootstrap in docker mode via --mode docker or VAULT_BOOTSTRAP_MODE=docker."
            )
        _run_local_agent(args.agent_binary, config_file)
    else:
        config_file.write_text(
            _render_docker_agent_config(
                vault_addr=args.vault_addr,
                output_filename=output_file.name,
            ),
            encoding="utf-8",
        )
        _run_docker_agent(
            docker_image=args.docker_image,
            config_file=config_file,
            template_file=template_file,
            role_id_file=role_id_file,
            secret_id_file=secret_id_file,
            output_dir=output_file.parent,
        )

    _ensure_file(output_file, "Vault rendered env file")
    _validate_rendered_env(output_file, _required_keys(template_file.name))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
