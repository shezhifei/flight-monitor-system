"""Build a runtime env file by merging bootstrap config with Vault-rendered secrets."""

from __future__ import annotations

import argparse
from pathlib import Path


def parse_env_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        raise FileNotFoundError(f"env file not found: {path}")

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip()
    return values


def _normalize_path_values(values: dict[str, str]) -> None:
    for key in (
        "VAULT_ROLE_ID_FILE",
        "VAULT_SECRET_ID_FILE",
        "VAULT_AGENT_CONFIG",
        "VAULT_RENDERED_ENV_FILE",
    ):
        raw_value = values.get(key, "").strip()
        if not raw_value:
            continue

        path = Path(raw_value)
        if not path.is_absolute():
            values[key] = str((Path.cwd() / path).resolve())


def _apply_common_derivations(
    values: dict[str, str],
    *,
    rendered_env_path: Path,
    output_path: Path,
) -> None:
    values["FMS_RUNTIME_ENV_FILE"] = str(output_path.resolve())
    values["FMS_VAULT_RENDERED_ENV_FILE"] = str(rendered_env_path.resolve())

    db_host = values.get("DB_HOST", "").strip()
    db_port = values.get("DB_PORT", "").strip()
    db_name = values.get("DB_NAME", "").strip()
    db_user = values.get("DB_USER", "").strip()
    db_password = values.get("DB_PASSWORD", "").strip()
    if db_host and db_port and db_name and db_user and db_password and not values.get("DATABASE_URL", "").strip():
        values["DATABASE_URL"] = f"postgres://{db_user}:{db_password}@{db_host}:{db_port}/{db_name}"
    if db_password and not values.get("POSTGRES_PASSWORD", "").strip():
        values["POSTGRES_PASSWORD"] = db_password
    if db_user and not values.get("POSTGRES_USER", "").strip():
        values["POSTGRES_USER"] = db_user
    if db_name and not values.get("POSTGRES_DB", "").strip():
        values["POSTGRES_DB"] = db_name
    if db_password and not values.get("PGPASSWORD", "").strip():
        values["PGPASSWORD"] = db_password
    if db_host and not values.get("PGHOST", "").strip():
        values["PGHOST"] = db_host
    if db_port and not values.get("PGPORT", "").strip():
        values["PGPORT"] = db_port
    if db_user and not values.get("PGUSER", "").strip():
        values["PGUSER"] = db_user

    redis_host = values.get("REDIS_HOST", "").strip()
    redis_port = values.get("REDIS_PORT", "").strip()
    redis_db = values.get("REDIS_DB", "0").strip() or "0"
    redis_password = values.get("REDIS_PASSWORD", "").strip()
    if redis_host and redis_port and redis_password and not values.get("REDIS_URL", "").strip():
        values["REDIS_URL"] = f"redis://:{redis_password}@{redis_host}:{redis_port}/{redis_db}"

    flowable_api_url = values.get("FLOWABLE_API_URL", "").strip()
    if flowable_api_url and not values.get("FLOWABLE_BASE_URL", "").strip():
        values["FLOWABLE_BASE_URL"] = flowable_api_url

    if values.get("FLOWABLE_ADMIN_PASSWORD", "").strip() and not values.get("FLOWABLE_PASSWORD", "").strip():
        values["FLOWABLE_PASSWORD"] = values["FLOWABLE_ADMIN_PASSWORD"]
    if values.get("FLOWABLE_ADMIN_PASSWORD", "").strip() and not values.get(
        "FLOWABLE_REST_APP_ADMIN_PASSWORD", ""
    ).strip():
        values["FLOWABLE_REST_APP_ADMIN_PASSWORD"] = values["FLOWABLE_ADMIN_PASSWORD"]
    if values.get("FLOWABLE_DB_PASSWORD", "").strip() and not values.get(
        "SPRING_DATASOURCE_PASSWORD", ""
    ).strip():
        values["SPRING_DATASOURCE_PASSWORD"] = values["FLOWABLE_DB_PASSWORD"]
    if values.get("FLOWABLE_DB_USER", "").strip() and not values.get(
        "SPRING_DATASOURCE_USERNAME", ""
    ).strip():
        values["SPRING_DATASOURCE_USERNAME"] = values["FLOWABLE_DB_USER"]
    flowable_db_name = values.get("FLOWABLE_DB_NAME", "").strip()
    if flowable_db_name and not values.get("SPRING_DATASOURCE_URL", "").strip():
        values["SPRING_DATASOURCE_URL"] = f"jdbc:postgresql://{db_host or 'postgres'}:{db_port or '5432'}/{flowable_db_name}"

    if values.get("JWT_SECRET_KEY", "").strip() and not values.get("JWT_SECRET", "").strip():
        values["JWT_SECRET"] = values["JWT_SECRET_KEY"]


def write_env_file(path: Path, values: dict[str, str]) -> None:
    lines = [f"{key}={value}" for key, value in values.items()]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description="Merge bootstrap env with Vault-rendered secrets")
    parser.add_argument("--base-env", required=True, help="Base non-secret env file")
    parser.add_argument("--rendered-env", required=True, help="Vault-rendered secret env file")
    parser.add_argument("--output", required=True, help="Merged runtime env file")
    args = parser.parse_args()

    base_values = parse_env_file(Path(args.base_env))
    rendered_values = parse_env_file(Path(args.rendered_env))
    runtime_values = dict(base_values)
    runtime_values.update(rendered_values)
    _normalize_path_values(runtime_values)
    rendered_env_path = Path(args.rendered_env)
    output_path = Path(args.output)
    _apply_common_derivations(
        runtime_values,
        rendered_env_path=rendered_env_path,
        output_path=output_path,
    )
    write_env_file(output_path, runtime_values)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
