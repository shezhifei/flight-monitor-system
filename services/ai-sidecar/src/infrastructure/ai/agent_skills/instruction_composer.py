"""Agent Skill 指令组合器 - 将 skill 内容转换为 LLM 指令上下文"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class SkillInstructionFragment:
    """Skill 指令片段"""

    skill_slug: str
    skill_version: str
    content_hash: str
    instruction: str
    references: dict[str, str] = field(default_factory=dict)
    token_count: int = 0


@dataclass
class ComposedInstructions:
    """组合后的指令"""

    fragments: list[SkillInstructionFragment]
    combined_text: str
    total_tokens: int
    skill_hashes: list[str] = field(default_factory=list)
    hash: str = ""

    def __post_init__(self):
        if not self.hash:
            self.hash = self._compute_hash()

    def _compute_hash(self) -> str:
        """计算组合指令 hash"""
        import hashlib as hl

        hash_input = "|".join(self.skill_hashes)
        return hl.sha256(hash_input.encode()).hexdigest()[:16]


class SkillInstructionComposer:
    """Skill 指令组合器

    职责：
    1. 根据实体配置和任务类型选择需要加载的 skills
    2. 加载 skill 内容和允许的 references
    3. 按优先级排序并组合为指令片段
    4. 应用上下文预算限制
    5. 生成可用于 prompt 的组合指令

    安全约束：
    - 只加载 approved 且 content_hash 匹配的 skill
    - 不执行 scripts/
    - 应用 max_instruction_tokens 限制
    """

    def __init__(
        self,
        skill_loader=None,
        skill_repo=None,
        token_counter=None,
    ):
        self._skill_loader = skill_loader
        self._skill_repo = skill_repo
        self._token_counter = token_counter

    async def compose(
        self,
        entity_id: str,
        task_type: str | None = None,
        max_total_tokens: int = 3000,
    ) -> ComposedInstructions | None:
        """为实体组合 skill 指令

        Args:
            entity_id: 实体 ID
            task_type: 任务类型（用于 task_routed 激活策略）
            max_total_tokens: 最大总 token 数

        Returns:
            ComposedInstructions 或 None
        """
        if not self._skill_repo:
            return None

        # 获取实体的 skill bindings
        bindings = await self._skill_repo.find_bindings_by_entity(entity_id)

        # 过滤启用的 bindings
        active_bindings = [b for b in bindings if b.get("enabled", False)]

        if not active_bindings:
            return None

        # 按任务类型过滤
        if task_type:
            active_bindings = [b for b in active_bindings if self._is_binding_active_for_task(b, task_type)]

        # 按优先级排序
        active_bindings.sort(key=lambda b: b.get("priority", 100))

        # 加载并组合指令
        fragments = []
        total_tokens = 0
        skill_hashes = []

        for binding in active_bindings:
            if total_tokens >= max_total_tokens:
                break

            skill_slug = binding.get("skill_slug")
            version = binding.get("version")
            allowed_refs = binding.get("allowed_reference_paths", [])
            max_tokens = binding.get("max_instruction_tokens", 3000)

            # 加载 skill
            if not self._skill_loader:
                continue

            skill = await self._skill_loader.load_skill(
                skill_slug=skill_slug,
                version=version,
                allowed_references=allowed_refs,
            )

            if not skill:
                continue

            # 计算 token 数
            instruction_tokens = self._count_tokens(skill.content)
            ref_tokens = sum(self._count_tokens(content) for content in skill.references.values())

            # 应用单个 skill 的 token 限制
            remaining_budget = max_total_tokens - total_tokens
            effective_max = min(max_tokens, remaining_budget)

            if instruction_tokens > effective_max:
                # 截断指令
                logger.warning(
                    f"Skill {skill_slug} instruction truncated: {instruction_tokens} > {effective_max} tokens"
                )
                continue  # 跳过超出预算的 skill

            fragment = SkillInstructionFragment(
                skill_slug=skill.slug,
                skill_version=skill.version,
                content_hash=skill.content_hash,
                instruction=skill.content,
                references=skill.references,
                token_count=instruction_tokens + ref_tokens,
            )

            fragments.append(fragment)
            total_tokens += fragment.token_count
            skill_hashes.append(f"{skill.slug}@{skill.version}:{skill.content_hash}")

        if not fragments:
            return None

        # 组合文本
        combined_parts = []
        for frag in fragments:
            combined_parts.append(f"<!-- Skill: {frag.skill_slug}@{frag.skill_version} -->")
            combined_parts.append(frag.instruction)
            for ref_name, ref_content in frag.references.items():
                combined_parts.append(f"\n### Reference: {ref_name}\n")
                combined_parts.append(ref_content)

        combined_text = "\n\n".join(combined_parts)

        composed = ComposedInstructions(
            fragments=fragments,
            combined_text=combined_text,
            total_tokens=total_tokens,
            skill_hashes=skill_hashes,
        )

        logger.info(
            f"Composed {len(fragments)} skill instructions for entity={entity_id}, "
            f"total_tokens={total_tokens}, hash={composed.hash}"
        )

        return composed

    def _is_binding_active_for_task(
        self,
        binding: dict[str, Any],
        task_type: str,
    ) -> bool:
        """检查 binding 是否对指定任务类型激活"""
        activation_policy = binding.get("activation_policy", "task_routed")

        if activation_policy == "always":
            return True

        if activation_policy == "manual":
            return False

        # task_routed
        allowed_types = binding.get("allowed_task_types", [])
        if not allowed_types:
            return True  # 空列表表示允许所有类型
        return task_type in allowed_types

    def _count_tokens(self, text: str) -> int:
        """估算 token 数"""
        if self._token_counter:
            return self._token_counter.count_tokens(text)
        # 简单估算：1 token ≈ 4 字符（英文）或 1 字符（中文）
        ascii_chars = sum(1 for c in text if ord(c) < 128)
        non_ascii_chars = len(text) - ascii_chars
        return (ascii_chars // 4) + non_ascii_chars


__all__ = [
    "ComposedInstructions",
    "SkillInstructionComposer",
    "SkillInstructionFragment",
]
