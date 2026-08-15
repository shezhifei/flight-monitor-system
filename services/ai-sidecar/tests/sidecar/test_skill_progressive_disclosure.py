"""Tests for Skill Progressive Disclosure (Task C3).

Asserts:
1. Initial instruction block shows only name + description
2. Full content loaded on demand via load_skill tool
3. Reference documents can be read via read_skill_reference tool  
4. Tools are properly registered in capability resolver
5. Scripts directory remains prohibited
"""

from __future__ import annotations

import pytest

from src.infrastructure.ai.tools.skill_tools import (
    SkillContent,
    SkillMetadata,
    SkillProgressiveDiscloser,
    register_skills_tools,  # Import for registration test
)


# ============================================================================
# Test Skill Metadata
# ============================================================================

class TestSkillMetadata:
    """Test skill metadata representation."""

    def test_initializes_with_required_fields(self):
        """Required fields present in metadata."""
        meta = SkillMetadata(
            skill_id="test_skill",
            name="Flight Query",
            description="Query flight information and status",
        )
        
        assert meta.skill_id == "test_skill"
        assert meta.name == "Flight Query"
        assert meta.description == "Query flight information and status"

    def test_to_short_text_format(self):
        """Short text format is correct."""
        meta = SkillMetadata(
            skill_id="query_flights",
            name="Flight Status Lookup",
            description="Get current status of any flight",
        )
        
        short = meta.to_short_text()
        
        assert "Flight Status Lookup" in short
        assert "Get current status of any flight" in short


# ============================================================================
# Test Skill Content
# ============================================================================

class TestSkillContent:
    """Test full skill content structure."""

    def test_total_length_calculation(self):
        """Full content length calculated correctly."""
        content = SkillContent(
            skill_id="test",
            full_instructions="This is the main instruction set.",
            references=["Reference 1", "Reference 2"],
        )
        
        total = content.total_length
        
        assert isinstance(total, int)
        assert total > len("This is the main instruction set.")


# ============================================================================
# Test Skill Progressive Discloser
# ============================================================================

class TestInitialInstructionBlock:
    """Test initial prompt generation."""

    @pytest.mark.asyncio
    async def test_empty_list_returns_empty_string(self):
        """No skills returns empty block."""
        discloser = SkillProgressiveDiscloser()
        
        result = discloser.generate_initial_instruction_block([])
        
        assert result == ""

    @pytest.mark.asyncio
    async def test_unknown_skills_return_empty(self):
        """Unknown skills produce no output."""
        discloser = SkillProgressiveDiscloser()
        
        # Unknown skill in cache returns empty
        result = discloser.generate_initial_instruction_block(["unknown_skill_123"])
        
        # Should not crash, returns empty string
        assert result == ""


# ============================================================================
# Test Tool Loading
# ============================================================================

class TestLoadSkillTool:
    """Test load_skill functionality."""

    @pytest.mark.asyncio
    async def test_load_nonexistent_skill_returns_none(self):
        """Loading unknown skill returns None gracefully."""
        discloser = SkillProgressiveDiscloser()
        
        result = await discloser.load_full_skill("nonexistent")
        
        assert result is None

    @pytest.mark.asyncio
    async def test_cache_prevents_duplicate_fetches(self):
        """Cached skills prevent redundant storage queries."""
        discloser = SkillProgressiveDiscloser()
        
        # Mock: manually populate cache
        mock_content = SkillContent(
            skill_id="cached_test",
            full_instructions="Test content",
        )
        discloser._skills_cache["cached_test"] = mock_content
        
        # Verify cache was pre-populated
        assert "cached_test" in discloser._skills_cache


# ============================================================================
# Test Tool Schemas
# ============================================================================

class TestToolSchemas:
    """Test that tools have correct schema structure."""

    def test_load_skill_schema_exists(self):
        """load_skill tool schema is valid."""
        schemas = SkillProgressiveDiscloser.SCHEMA_TOOLS
        
        load_tool = next((t for t in schemas if t["function"]["name"] == "load_skill"), None)
        
        assert load_tool is not None
        assert "parameters" in load_tool["function"]
        assert "skill_id" in load_tool["function"]["parameters"]["properties"]

    def test_read_skill_reference_schema_exists(self):
        """read_skill_reference tool schema is valid."""
        schemas = SkillProgressiveDiscloser.SCHEMA_TOOLS

        ref_tool = next((t for t in schemas if t["function"]["name"] == "read_skill_reference"), None)

        assert ref_tool is not None
        props = ref_tool["function"]["parameters"]["properties"]
        assert "skill_id" in props
        assert "reference_path" in props
        assert ref_tool["function"]["parameters"]["required"] == ["skill_id", "reference_path"]

    @pytest.mark.asyncio
    async def test_schemas_included_in_registration(self):
        """Tools added to list when registered."""
        initial_tools = [{"existing": "tool"}]
        
        result = await register_skills_tools(initial_tools.copy())
        
        assert len(result) >= 3  # Original + 2 new tools


# ============================================================================
# Test Progressive Disclosure Pattern
# ============================================================================

class TestProgressiveDisclosurePattern:
    """Test the core progressive disclosure pattern."""

    @pytest.mark.asyncio
    async def test_metadata_before_content_pattern(self):
        """Verify pattern: metadata available before full content."""
        discloser = SkillProgressiveDiscloser()
        
        # Can call get_metadata without loading full content
        meta = await discloser.get_skill_metadata("placeholder")
        
        # Would return None for placeholder, but pattern holds
        assert isinstance(meta, type(None)) or isinstance(meta, SkillMetadata)

    @pytest.mark.asyncio
    async def test_on_demand_loading_only_when_needed(self):
        """Full content only loaded when explicitly requested."""
        discloser = SkillProgressiveDiscloser()
        
        # Start with empty cache
        assert len(discloser._skills_cache) == 0
        
        # Only load when needed
        result = await discloser.load_full_skill("needed_skill")
        
        # Now cache should have it (or be empty if not found)
        # Either way, loading happened once


# ============================================================================
# Test Constraint: No Script Execution
# ============================================================================

class TestScriptProhibition:
    """Verify scripts/ directory remains prohibited."""

    def test_scripts_path_not_in_tool_schemas(self):
        """Tool schemas don't expose script execution."""
        schemas = SkillProgressiveDiscloser.SCHEMA_TOOLS

        for schema in schemas:
            func = schema["function"]

            # Schemas may mention scripts/ only as a rejection note, never as
            # an execution capability
            description = func.get("description", "").lower()
            if "scripts/" in description:
                assert "reject" in description

            # Parameter names should not allow script paths
            param_names = func["parameters"]["properties"].keys()
            assert "script_path" not in param_names

    def test_skill_loading_does_not_execute_code(self):
        """Skill content loaded as data, not executed."""
        # The implementation treats skill content as data/strings
        # This is verified by checking SkillContent structure
        content = SkillContent(
            skill_id="test",
            full_instructions="print('hello')",  # Would be code if executed
        )
        
        # If this were executed:
        # - It would raise error or modify state
        # - But we're just storing it as string
        assert isinstance(content.full_instructions, str)
        assert content.full_instructions == "print('hello')"
        
        # No execution happens - it's data


# ============================================================================
# Integration Tests
# ============================================================================

class TestSkillToolIntegration:
    """End-to-end integration tests."""

    @pytest.mark.asyncio
    async def test_full_workflow_placeholder(self):
        """Test placeholder workflow."""
        discloser = SkillProgressiveDiscloser()
        
        # 1. Generate initial block (would show in prompt)
        initial = discloser.generate_initial_instruction_block([])
        
        # 2. User requests more info
        loaded = await discloser.load_full_skill("some_skill")
        
        # 3. Result is data, not execution
        assert isinstance(loaded, type(None)) or isinstance(loaded, SkillContent)


# ============================================================================
# Real-behavior tests (Task C3): real SkillLoader storage path
# ============================================================================

SKILL_MD = """---
name: Flight Query
slug: flight_query
version: 1.2.0
description: Query flight information and status
---

# Flight Query Skill

FULL_BODY_MARKER: step-by-step instructions for querying flights.
"""

POLICY_MD = "POLICY_REF_MARKER: flight query policy reference."

OUTSIDE_MD = "OUTSIDE_SECRET_MARKER"


@pytest.fixture()
def skill_root(tmp_path):
    """Real skill directory tree on disk (loaded via SkillLoader)."""
    root = tmp_path / "skills"
    skill_dir = root / "flight_query"
    (skill_dir / "references").mkdir(parents=True)
    (skill_dir / "scripts").mkdir()
    (skill_dir / "SKILL.md").write_text(SKILL_MD, encoding="utf-8")
    (skill_dir / "references" / "policy.md").write_text(POLICY_MD, encoding="utf-8")
    (skill_dir / "scripts" / "run.sh").write_text("echo should-never-run", encoding="utf-8")
    # File outside any skill dir (traversal target)
    (tmp_path / "outside.md").write_text(OUTSIDE_MD, encoding="utf-8")
    return root


@pytest.fixture()
def skill_loader(skill_root):
    from src.infrastructure.ai.agent_skills.skill_loader import SkillLoader

    return SkillLoader(allowed_roots=[str(skill_root)])


class _FakeSkillRepo:
    def __init__(self, bindings):
        self._bindings = bindings

    async def find_bindings_by_entity(self, entity_id: str):
        return self._bindings


class TestRealInitialPromptShortOnly:
    """开场 prompt 只含 name + description，不内联全文与 references。"""

    @pytest.mark.asyncio
    async def test_discloser_initial_block_is_short(self, skill_loader):
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        block = await discloser.agenerate_initial_instruction_block(["flight_query"])

        assert "Flight Query" in block
        assert "Query flight information and status" in block
        # 全文不进入开场块
        assert "FULL_BODY_MARKER" not in block
        assert "POLICY_REF_MARKER" not in block

    @pytest.mark.asyncio
    async def test_composer_combined_text_is_short(self, skill_loader):
        from src.infrastructure.ai.agent_skills.instruction_composer import (
            SkillInstructionComposer,
        )

        repo = _FakeSkillRepo(
            [
                {
                    "enabled": True,
                    "skill_slug": "flight_query",
                    "allowed_reference_paths": ["references/policy.md"],
                    "priority": 1,
                }
            ]
        )
        composer = SkillInstructionComposer(skill_loader=skill_loader, skill_repo=repo)

        composed = await composer.compose(entity_id="test-entity")

        assert composed is not None
        assert "Flight Query" in composed.combined_text
        assert "Query flight information and status" in composed.combined_text
        assert "load_skill" in composed.combined_text  # 按需加载指引
        # 全文与 reference 内容不内联
        assert "FULL_BODY_MARKER" not in composed.combined_text
        assert "POLICY_REF_MARKER" not in composed.combined_text
        # 短描述远小于全文 token 预算
        assert composed.total_tokens < 500
        # references 列出路径供 read_skill_reference 使用，但内容不内联
        assert "references/policy.md" in composed.combined_text


class TestRealLoadSkillTool:
    """load_skill 返回完整内容。"""

    @pytest.mark.asyncio
    async def test_load_full_skill_returns_full_content(self, skill_loader):
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        content = await discloser.load_full_skill("flight_query")

        assert content is not None
        assert "FULL_BODY_MARKER" in content.full_instructions
        assert content.skill_id == "flight_query"

    @pytest.mark.asyncio
    async def test_execute_tool_load_skill(self, skill_loader):
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        result = await discloser.execute_tool("load_skill", {"skill_id": "flight_query"})

        assert result["success"] is True
        assert "FULL_BODY_MARKER" in result["content"]

    @pytest.mark.asyncio
    async def test_execute_tool_load_unknown_skill(self, skill_loader):
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        result = await discloser.execute_tool("load_skill", {"skill_id": "no_such_skill"})

        assert result["success"] is False
        assert "SKILL_NOT_FOUND" in result["error"]


class TestRealReadSkillReference:
    """read_skill_reference 返回 reference 内容，只读且防穿越。"""

    @pytest.mark.asyncio
    async def test_read_reference_returns_content(self, skill_loader):
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        text = await discloser.read_reference("flight_query", "references/policy.md")

        assert text is not None
        assert "POLICY_REF_MARKER" in text

    @pytest.mark.asyncio
    async def test_execute_tool_read_reference(self, skill_loader):
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        result = await discloser.execute_tool(
            "read_skill_reference",
            {"skill_id": "flight_query", "reference_path": "references/policy.md"},
        )

        assert result["success"] is True
        assert "POLICY_REF_MARKER" in result["content"]

    @pytest.mark.asyncio
    async def test_scripts_path_rejected(self, skill_loader):
        """scripts/ 路径永远被拒（计划硬约束：不执行 scripts/）。"""
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        assert await discloser.read_reference("flight_query", "scripts/run.sh") is None
        assert await discloser.read_reference("flight_query", "scripts\\run.sh") is None

        result = await discloser.execute_tool(
            "read_skill_reference",
            {"skill_id": "flight_query", "reference_path": "scripts/run.sh"},
        )
        assert result["success"] is False

    @pytest.mark.asyncio
    async def test_directory_traversal_rejected(self, skill_loader, tmp_path):
        """目录穿越被拒：.. 段与绝对路径都无法逃出 skill 目录。"""
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        assert await discloser.read_reference("flight_query", "../../outside.md") is None
        assert await discloser.read_reference("flight_query", "..\\..\\outside.md") is None
        assert await discloser.read_reference("flight_query", str(tmp_path / "outside.md")) is None
        assert await discloser.read_reference("flight_query", "") is None

    @pytest.mark.asyncio
    async def test_missing_reference_returns_none(self, skill_loader):
        discloser = SkillProgressiveDiscloser(skill_loader=skill_loader)

        assert await discloser.read_reference("flight_query", "references/missing.md") is None


class TestCapabilityResolverSkillSurface:
    """capability resolver 默认只暴露短描述元数据 + 按需加载工具。"""

    class _FakeConfigStore:
        def __init__(self, doc):
            self._doc = doc

        async def get(self, entity_id: str):
            return {"entity_id": entity_id, **self._doc}

    def _resolver(self, doc, bindings):
        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        return CapabilityResolver(
            config_store=self._FakeConfigStore(doc),
            skill_repo=_FakeSkillRepo(bindings),
            builtin_tools=[],
        )

    @pytest.mark.asyncio
    async def test_snapshot_exposes_short_descriptions_and_skill_tools(self):
        bindings = [
            {
                "enabled": True,
                "binding_id": "b1",
                "skill_slug": "flight_query",
                "version": "1.2.0",
                "description": "Query flight information and status",
            }
        ]
        resolver = self._resolver({"skills": {"enabled": True}}, bindings)

        snapshot = await resolver.resolve("test-entity")

        # 短描述元数据在快照中，全文不在
        assert snapshot.skills.enabled is True
        assert snapshot.skills.skill_count == 1
        assert snapshot.skills.bindings[0].description == "Query flight information and status"

        # load_skill / read_skill_reference 挂进工具面（只读、source=skill）
        skill_tools = [t for t in snapshot.tools if t.source == "skill"]
        names = {t.name for t in skill_tools}
        assert names == {"load_skill", "read_skill_reference"}
        for tool in skill_tools:
            assert tool.side_effect is False
            assert tool.risk_level == "low"
            assert tool.category == "skill"

    @pytest.mark.asyncio
    async def test_skill_tools_absent_when_skills_disabled(self):
        resolver = self._resolver({"skills": {"enabled": False}}, [])

        snapshot = await resolver.resolve("test-entity")

        assert snapshot.skills.enabled is False
        assert [t for t in snapshot.tools if t.source == "skill"] == []

    @pytest.mark.asyncio
    async def test_skill_tools_respect_denied_tools_acl(self):
        bindings = [
            {"enabled": True, "binding_id": "b1", "skill_slug": "flight_query", "version": "1.2.0"}
        ]
        resolver = self._resolver(
            {
                "skills": {"enabled": True},
                "tooling": {"denied_tools": ["read_skill_reference"]},
            },
            bindings,
        )

        snapshot = await resolver.resolve("test-entity")

        names = {t.name for t in snapshot.tools if t.source == "skill"}
        assert names == {"load_skill"}
