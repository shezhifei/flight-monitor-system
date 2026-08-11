"""验证顶层文档不再在标准 Docker 拓扑中引用已移除的 compose worker 服务。"""
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
DOCS_TO_CHECK = [
    "CLAUDE.md",
    "README.md",
    "QUICK_START.md",
    "docs/DEPLOYMENT.md",
]

STALE_TOPOLOGY = "rust-api, worker, flowable"


def test_no_worker_in_standard_docker_topology_comment():
    for doc in DOCS_TO_CHECK:
        path = REPO_ROOT / doc
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8")
        assert STALE_TOPOLOGY not in text, (
            f"{doc} still lists removed compose service in standard topology comment. "
            "Expected rust-api, flowable, postgres, redis, rocketmq, mq-gateway."
        )


def test_claude_md_migration_version_current():
    claude = (REPO_ROOT / "CLAUDE.md").read_text(encoding="utf-8")
    migrations_dir = REPO_ROOT / "migrations"
    latest_prefix = sorted(p.name.split("_", 1)[0] for p in migrations_dir.glob("*.sql"))[-1]
    assert f"Latest at time of writing: `{latest_prefix}_" in claude, (
        f"CLAUDE.md migration hint should reference latest prefix `{latest_prefix}_`"
    )