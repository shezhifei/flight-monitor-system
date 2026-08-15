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
    """Skill 指令组合器（渐进披露模式，Task C3）

    职责：
    1. 根据实体配置和任务类型选择需要加载的 skills
    2. 加载 skill 元数据（name/description）与 content hash
    3. 按优先级排序并组合为短描述指令片段
    4. 应用上下文预算限制
    5. 生成可用于开场 prompt 的组合指令

    渐进披露约束（docs/plans/2026-08-14-hybrid-agent-architecture.md Task C3）：
    - 开场 prompt 只含每个 skill 的 name + description（短文本）
    - skill 全文和 references 不内联，经 load_skill / read_skill_reference 只读工具按需加载
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
        """为实体组合 skill 指令（短描述模式）

        开场 prompt 只内联每个 skill 的 name + description 短文本；
        skill 全文与 references 由 load_skill / read_skill_reference
        只读工具按需加载（见 tools/skill_tools.py）。

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

        # 加载并组合短描述指令（渐进披露：全文不内联）
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

            # 加载 skill（仅需元数据与 content hash；全文不进入开场 prompt）
            if not self._skill_loader:
                continue

            skill = await self._skill_loader.load_skill(
                skill_slug=skill_slug,
                version=version,
                allowed_references=allowed_refs,
            )

            if not skill:
                continue

            # 短描述片段：name + description + 按需加载指引
            short_instruction = self._build_short_instruction(skill, allowed_refs)

            # 计算 token 数（短描述通常远小于预算）
            instruction_tokens = self._count_tokens(short_instruction)

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
                instruction=short_instruction,
                references={},  # references 不内联，按需经 read_skill_reference 加载
                token_count=instruction_tokens,
            )

            fragments.append(fragment)
            total_tokens += fragment.token_count
            skill_hashes.append(f"{skill.slug}@{skill.version}:{skill.content_hash}")

        if not fragments:
            return None

        # 组合文本：短描述列表 + 按需加载指引
        combined_parts = [
            "以下技能已启用（仅显示短描述）。需要完整指令时调用 load_skill 工具；"
            "需要参考文档时调用 read_skill_reference 工具。scripts/ 目录不可读取或执行。"
        ]
        for frag in fragments:
            combined_parts.append(f"<!-- Skill: {frag.skill_slug}@{frag.skill_version} -->")
            combined_parts.append(frag.instruction)

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

    def _build_short_instruction(self, skill: Any, allowed_refs: list[str]) -> str:
        """构建 skill 短描述片段（name + description + 按需加载指引）"""
        description = getattr(skill, "description", "") or "(no description)"
        lines = [f"- {skill.name} (slug={skill.slug}@{skill.version}): {description}"]
        if allowed_refs:
            lines.append(f"  references: {', '.join(allowed_refs)}")
        return "\n".join(lines)

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
