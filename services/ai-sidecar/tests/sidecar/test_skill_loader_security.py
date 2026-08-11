from __future__ import annotations

import pytest

from src.infrastructure.ai.agent_skills.skill_loader import SkillLoader


@pytest.mark.asyncio
async def test_skill_loader_rejects_reference_outside_current_skill_dir(tmp_path):
    skills_root = tmp_path / "skills"
    current_skill = skills_root / "current"
    other_skill = skills_root / "other"
    current_skill.mkdir(parents=True)
    other_skill.mkdir(parents=True)

    (current_skill / "SKILL.md").write_text(
        "---\nname: Current\nslug: current\nversion: 1.0.0\n---\nCurrent body\n",
        encoding="utf-8",
    )
    (other_skill / "secret.md").write_text("should not be loaded", encoding="utf-8")

    loader = SkillLoader(allowed_roots=[str(skills_root)])

    loaded = await loader.load_skill(
        "current",
        allowed_references=["../other/secret.md"],
    )

    assert loaded is not None
    assert loaded.references == {}


@pytest.mark.asyncio
async def test_skill_loader_allows_reference_inside_current_skill_dir(tmp_path):
    skills_root = tmp_path / "skills"
    current_skill = skills_root / "current"
    references_dir = current_skill / "references"
    references_dir.mkdir(parents=True)

    (current_skill / "SKILL.md").write_text(
        "---\nname: Current\nslug: current\nversion: 1.0.0\n---\nCurrent body\n",
        encoding="utf-8",
    )
    (references_dir / "guide.md").write_text("allowed reference", encoding="utf-8")

    loader = SkillLoader(allowed_roots=[str(skills_root)])

    loaded = await loader.load_skill(
        "current",
        allowed_references=["references/guide.md"],
    )

    assert loaded is not None
    assert loaded.references == {"references/guide.md": "allowed reference"}
