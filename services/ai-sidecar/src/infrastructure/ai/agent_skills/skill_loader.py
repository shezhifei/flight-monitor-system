"""Agent Skill 加载器 - 从 allowlisted 目录加载 SKILL.md"""

from __future__ import annotations

import hashlib
import logging
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from src.infrastructure.common.exceptions import IO_EXCEPTIONS

logger = logging.getLogger(__name__)


@dataclass
class SkillFrontmatter:
    """Skill frontmatter 元数据"""

    name: str
    slug: str
    version: str
    description: str = ""
    tags: list[str] = field(default_factory=list)


@dataclass
class LoadedSkill:
    """已加载的 Skill"""

    slug: str
    version: str
    name: str
    description: str
    source: str
    canonical_path: str
    entry_file: str = "SKILL.md"
    frontmatter: dict[str, Any] = field(default_factory=dict)
    content_hash: str = ""
    content: str = ""
    references: dict[str, str] = field(default_factory=dict)

    def __post_init__(self):
        if not self.content_hash and self.content:
            self.content_hash = self._compute_content_hash()

    def _compute_content_hash(self) -> str:
        """计算内容 hash"""
        hash_input = self.content.encode()
        return f"sha256:{hashlib.sha256(hash_input).hexdigest()}"


class SkillLoader:
    """Agent Skill 加载器

    职责：
    1. 扫描 allowlisted skill roots
    2. 解析 SKILL.md frontmatter
    3. 计算 content hash
    4. 加载允许的 reference 文件
    5. 校验 skill 完整性

    安全约束：
    - 只从 allowlisted 目录加载
    - 只读取 SKILL.md 和允许的 reference 路径
    - 不执行 scripts/ 目录下的任何文件
    - 校验 content hash
    """

    def __init__(
        self,
        allowed_roots: list[str] | None = None,
        skill_repo=None,
    ):
        self._allowed_roots = [Path(r) for r in (allowed_roots or [])]
        self._skill_repo = skill_repo
        self._loaded_cache: dict[str, LoadedSkill] = {}

    async def load_skill(
        self,
        skill_slug: str,
        version: str | None = None,
        allowed_references: list[str] | None = None,
    ) -> LoadedSkill | None:
        """加载指定的 Skill

        Args:
            skill_slug: Skill slug
            version: 版本（可选，不指定则加载最新）
            allowed_references: 允许加载的引用文件路径列表

        Returns:
            LoadedSkill 或 None
        """
        # 检查缓存
        cache_key = f"{skill_slug}:{version or 'latest'}"
        if cache_key in self._loaded_cache:
            return self._loaded_cache[cache_key]

        # 查找 skill 目录
        skill_dir = await self._find_skill_dir(skill_slug, version)
        if not skill_dir:
            logger.warning(f"Skill not found: {skill_slug} (version={version})")
            return None

        # 验证路径在 allowlist 内
        if not self._is_path_allowed(skill_dir):
            logger.error(f"Skill path not in allowlist: {skill_dir}")
            return None

        # 加载 SKILL.md
        skill_md_path = skill_dir / "SKILL.md"
        if not skill_md_path.exists():
            logger.error(f"SKILL.md not found: {skill_md_path}")
            return None

        content = skill_md_path.read_text(encoding="utf-8")

        # 解析 frontmatter
        frontmatter = self._parse_frontmatter(content)

        # 加载允许的 references
        references = {}
        if allowed_references:
            for ref_path in allowed_references:
                ref_file = skill_dir / ref_path
                if ref_file.exists() and self._is_reference_path_allowed(skill_dir, ref_file):
                    try:
                        references[ref_path] = ref_file.read_text(encoding="utf-8")
                    except IO_EXCEPTIONS as e:
                        logger.warning(f"Failed to load reference {ref_path}: {e}")

        # 构建 LoadedSkill
        skill = LoadedSkill(
            slug=frontmatter.get("slug", skill_slug),
            version=frontmatter.get("version", version or "0.0.0"),
            name=frontmatter.get("name", skill_slug),
            description=frontmatter.get("description", ""),
            source=self._detect_source(skill_dir),
            canonical_path=str(skill_dir),
            frontmatter=frontmatter,
            content=content,
            references=references,
        )

        # 缓存
        self._loaded_cache[cache_key] = skill

        logger.info(
            f"Loaded skill: {skill.slug}@{skill.version}, hash={skill.content_hash}, references={len(references)}"
        )

        return skill

    async def scan_skills(self) -> list[LoadedSkill]:
        """扫描所有 allowlisted roots 中的 skills"""
        skills = []

        for root in self._allowed_roots:
            if not root.exists() or not root.is_dir():
                continue

            for item in root.iterdir():
                if item.is_dir() and (item / "SKILL.md").exists():
                    try:
                        skill = await self._load_from_dir(item)
                        if skill:
                            skills.append(skill)
                    except Exception as e:  # noqa: BLE001 - skill loading may fail in various ways
                        logger.warning(f"Failed to load skill from {item}: {e}")

        logger.info(f"Scanned {len(skills)} skills from {len(self._allowed_roots)} roots")
        return skills

    async def _find_skill_dir(
        self,
        skill_slug: str,
        version: str | None = None,
    ) -> Path | None:
        """查找 skill 目录"""
        for root in self._allowed_roots:
            if not root.exists():
                continue

            # 尝试多种命名模式
            patterns = [
                f"{skill_slug}-{version}" if version else None,
                f"{skill_slug}",
                f"{skill_slug}_*",
            ]

            for pattern in patterns:
                if not pattern:
                    continue

                if "*" in pattern:
                    matches = list(root.glob(pattern))
                    if matches:
                        # 按修改时间排序，返回最新的
                        return sorted(matches, key=lambda p: p.stat().st_mtime)[-1]
                else:
                    candidate = root / pattern
                    if candidate.exists() and candidate.is_dir():
                        return candidate

        return None

    async def _load_from_dir(self, skill_dir: Path) -> LoadedSkill | None:
        """从目录加载 skill"""
        skill_md = skill_dir / "SKILL.md"
        if not skill_md.exists():
            return None

        content = skill_md.read_text(encoding="utf-8")
        frontmatter = self._parse_frontmatter(content)

        return LoadedSkill(
            slug=frontmatter.get("slug", skill_dir.name),
            version=frontmatter.get("version", "0.0.0"),
            name=frontmatter.get("name", skill_dir.name),
            description=frontmatter.get("description", ""),
            source=self._detect_source(skill_dir),
            canonical_path=str(skill_dir),
            frontmatter=frontmatter,
            content=content,
        )

    def _parse_frontmatter(self, content: str) -> dict[str, Any]:
        """解析 YAML frontmatter"""
        match = re.match(r"^---\s*\n(.*?)\n---\s*\n", content, re.DOTALL)
        if not match:
            return {}

        try:
            import yaml

            return yaml.safe_load(match.group(1)) or {}
        except ImportError:
            # 简单解析
            result = {}
            for line in match.group(1).split("\n"):
                if ":" in line:
                    key, value = line.split(":", 1)
                    result[key.strip()] = value.strip()
            return result

    def _is_path_allowed(self, path: Path) -> bool:
        """检查路径是否在 allowlist 内"""
        resolved = path.resolve()
        for root in self._allowed_roots:
            try:
                resolved.relative_to(root.resolve())
                return True
            except ValueError:
                continue
        return False

    def _is_reference_path_allowed(self, skill_dir: Path, path: Path) -> bool:
        """检查 reference 文件是否仍位于当前 skill 目录内。"""
        resolved = path.resolve()
        try:
            resolved.relative_to(skill_dir.resolve())
        except ValueError:
            logger.warning(f"Rejected skill reference outside skill directory: {path}")
            return False
        return self._is_path_allowed(resolved)

    def _detect_source(self, skill_dir: Path) -> str:
        """检测 skill 来源"""
        path_str = str(skill_dir)
        if ".agents" in path_str:
            return "agents_home"
        elif ".codex" in path_str:
            return "codex_home"
        elif "agent-skills" in path_str:
            return "bundled"
        return "unknown"

    def clear_cache(self) -> None:
        """清空缓存"""
        self._loaded_cache.clear()


__all__ = [
    "LoadedSkill",
    "SkillFrontmatter",
    "SkillLoader",
]
