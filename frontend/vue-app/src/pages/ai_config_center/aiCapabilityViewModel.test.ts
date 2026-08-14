/**
 * Tests for aiCapabilityViewModel.ts pure functions
 */

import { describe, it, expect } from 'vitest';
import {
  snapshotToViewModel,
  buildBasicInfo,
  buildToolSection,
  buildMcpSection,
  buildSkillSection,
  buildSubagentSection,
  buildCacheSection,
  mcpDiscoveryBadge,
  validationErrorBadge,
  validationBadges,
  summarizeValidation,
} from './aiCapabilityViewModel';
import type { EnrichedCapabilitySnapshot, CacheMetricsSummary } from './aiConfigTypes';
import { computeHitRate } from './aiConfigTypes';

function makeSnapshot(overrides: Partial<EnrichedCapabilitySnapshot> = {}): EnrichedCapabilitySnapshot {
  return {
    entity_id: 'test-entity',
    config_version: 2,
    config_revision: 1,
    model_id: 'gpt-4o',
    provider_model: 'gpt-4o',
    api_format: 'chat_completions',
    input_modalities: ['text'],
    output_modalities: ['text'],
    tool_count: 5,
    mcp_enabled: false,
    skills_enabled: false,
    subagents_enabled: false,
    cache_enabled: true,
    context_strategy: 'hybrid',
    snapshot_hash: 'abc123',
    builtin_tool_count: 3,
    mcp_tool_count: 2,
    subagent_tool_enabled: false,
    skill_instruction_count: 0,
    mcp_servers: [],
    mcp_server_count: 0,
    mcp_binding_count: 0,
    mcp_allowlist_configured: false,
    skill_bindings: [],
    subagent_risk: { enabled_no_allowlist: false, high_depth: false },
    cache_backends: {
      provider_prompt_cache: false,
      provider_prompt_cache_retention: null,
      provider_prompt_cache_namespace: null,
      tool_result_cache: false,
      tool_result_cache_ttl: 0,
      tool_result_cacheable_tools: [],
      mcp_resource_cache: false,
      mcp_resource_cache_note: 'disabled',
      context_cache: false,
      context_cache_note: 'disabled',
      context_cache_ttl: 0,
    },
    ...overrides,
  };
}

describe('computeHitRate', () => {
  it('returns — for zero total', () => {
    expect(computeHitRate(0, 0)).toBe('—');
  });

  it('computes correct percentage', () => {
    expect(computeHitRate(3, 1)).toBe('75.0%');
  });

  it('handles 100% hit rate', () => {
    expect(computeHitRate(10, 0)).toBe('100.0%');
  });

  it('handles 0% hit rate', () => {
    expect(computeHitRate(0, 5)).toBe('0.0%');
  });
});

describe('buildBasicInfo', () => {
  it('returns all required fields', () => {
    const snapshot = makeSnapshot();
    const rows = buildBasicInfo(snapshot);
    const keys = rows.map(r => r.key);
    expect(keys).toContain('entity_id');
    expect(keys).toContain('model_id');
    expect(keys).toContain('api_format');
    expect(keys).toContain('snapshot_hash');
    expect(keys).toContain('config_revision');
  });
});

describe('buildToolSection', () => {
  it('shows warning when tool_count is 0', () => {
    const snapshot = makeSnapshot({ tool_count: 0, builtin_tool_count: 0, mcp_tool_count: 0 });
    const section = buildToolSection(snapshot);
    expect(section.total).toBe(0);
    expect(section.emptyWarning).not.toBeNull();
    expect(section.emptyWarning!.level).toBe('warning');
  });

  it('no warning when tools exist', () => {
    const section = buildToolSection(makeSnapshot());
    expect(section.total).toBe(5);
    expect(section.emptyWarning).toBeNull();
  });

  it('reflects builtin and mcp counts', () => {
    const section = buildToolSection(makeSnapshot());
    expect(section.builtin).toBe(3);
    expect(section.mcp).toBe(2);
  });
});

describe('buildMcpSection', () => {
  it('shows warning when mcp enabled but no servers', () => {
    const snapshot = makeSnapshot({ mcp_enabled: true, mcp_server_count: 0 });
    const section = buildMcpSection(snapshot);
    expect(section.enabled).toBe(true);
    expect(section.noServersWarning).not.toBeNull();
  });

  it('no warning when mcp disabled', () => {
    const section = buildMcpSection(makeSnapshot());
    expect(section.enabled).toBe(false);
    expect(section.noServersWarning).toBeNull();
  });

  it('maps server discovery entries', () => {
    const snapshot = makeSnapshot({
      mcp_enabled: true,
      mcp_server_count: 1,
      mcp_servers: [{
        server_id: 'srv-1',
        display_name: 'Test',
        enabled: true,
        transport: 'stdio',
        command_ref_present: true,
        command_allowlisted: true,
        allowlist_configured: true,
        server_status: 'active',
        discovery_status: 'discovered',
      }],
    });
    const section = buildMcpSection(snapshot);
    expect(section.servers).toHaveLength(1);
    expect(section.servers[0].statusBadge.level).toBe('ok');
    expect(section.servers[0].statusBadge.label).toBe('已发现');
  });
});

describe('buildSkillSection', () => {
  it('shows warning when skills enabled but no bindings', () => {
    const snapshot = makeSnapshot({ skills_enabled: true, skill_instruction_count: 0 });
    const section = buildSkillSection(snapshot);
    expect(section.enabled).toBe(true);
    expect(section.noBindingsWarning).not.toBeNull();
  });

  it('maps skill binding details', () => {
    const snapshot = makeSnapshot({
      skills_enabled: true,
      skill_instruction_count: 1,
      skill_bindings: [{
        binding_id: 'b1',
        skill_slug: 'test-skill',
        version: '1.0.0',
        enabled: true,
        activation_policy: 'always',
        priority: 10,
        max_instruction_tokens: 3000,
      }],
    });
    const section = buildSkillSection(snapshot);
    expect(section.bindings).toHaveLength(1);
    expect(section.bindings[0].skillSlug).toBe('test-skill');
  });
});

describe('buildSubagentSection', () => {
  it('shows risk when enabled with empty allowlist', () => {
    const snapshot = makeSnapshot({
      subagents_enabled: true,
      subagent_risk: { enabled_no_allowlist: true, high_depth: false },
    });
    const section = buildSubagentSection(snapshot);
    expect(section.riskBadge).not.toBeNull();
    expect(section.riskBadge!.level).toBe('warning');
  });

  it('shows risk when max_depth > 1', () => {
    const snapshot = makeSnapshot({
      subagents_enabled: true,
      subagent_risk: { enabled_no_allowlist: false, high_depth: true },
      subagents: { max_depth: 3, allowed_entity_ids: ['a'], max_concurrency: 2, inherit_parent_context: true },
    });
    const section = buildSubagentSection(snapshot);
    expect(section.riskBadge).not.toBeNull();
    expect(section.riskBadge!.detail).toContain('max_depth');
  });

  it('no risk when disabled', () => {
    const section = buildSubagentSection(makeSnapshot());
    expect(section.riskBadge).toBeNull();
  });
});

describe('buildCacheSection', () => {
  it('maps cache backends', () => {
    const snapshot = makeSnapshot({
      cache_backends: {
        provider_prompt_cache: true,
        provider_prompt_cache_retention: '24h',
        provider_prompt_cache_namespace: 'test',
        tool_result_cache: false,
        tool_result_cache_ttl: 60,
        tool_result_cacheable_tools: [],
        mcp_resource_cache: false,
        mcp_resource_cache_note: 'disabled',
        context_cache: false,
        context_cache_note: 'disabled',
        context_cache_ttl: 0,
      },
    });
    const section = buildCacheSection(snapshot);
    expect(section.backends).toHaveLength(4);
    const ppc = section.backends.find(b => b.key === 'provider_prompt_cache');
    expect(ppc!.enabled).toBe(true);
    expect(ppc!.note).toBe('active');
    expect(ppc!.detail).toContain('24h');
  });

  it('shows adapter_available note for mcp_resource_cache', () => {
    const snapshot = makeSnapshot({
      cache_backends: {
        provider_prompt_cache: false,
        provider_prompt_cache_retention: null,
        provider_prompt_cache_namespace: null,
        tool_result_cache: false,
        tool_result_cache_ttl: 0,
        tool_result_cacheable_tools: [],
        mcp_resource_cache: true,
        mcp_resource_cache_note: 'adapter_available',
        context_cache: false,
        context_cache_note: 'disabled',
        context_cache_ttl: 0,
      },
    });
    const section = buildCacheSection(snapshot);
    const mrc = section.backends.find(b => b.key === 'mcp_resource_cache');
    expect(mrc!.note).toBe('adapter_available');
  });

  it('maps cache metrics', () => {
    const metrics: CacheMetricsSummary = {
      period_hours: 24,
      by_cache_type: [{
        cache_type: 'prompt',
        total_events: 10,
        hits: 7,
        misses: 3,
        total_cached_tokens: 500,
        total_read_tokens: 0,
        total_write_tokens: 0,
      }],
    };
    const section = buildCacheSection(makeSnapshot(), metrics);
    expect(section.metrics).toHaveLength(1);
    expect(section.metrics[0].cacheType).toBe('prompt');
    expect(section.metrics[0].hitRate).toBe('70.0%');
  });
});

describe('mcpDiscoveryBadge', () => {
  it('returns ok for discovered', () => {
    const badge = mcpDiscoveryBadge({
      server_id: 's', display_name: 'S', enabled: true, transport: 'stdio',
      command_ref_present: true, command_allowlisted: true, allowlist_configured: true,
      server_status: 'active', discovery_status: 'discovered',
    });
    expect(badge.level).toBe('ok');
  });

  it('returns error for command_not_allowlisted', () => {
    const badge = mcpDiscoveryBadge({
      server_id: 's', display_name: 'S', enabled: true, transport: 'stdio',
      command_ref_present: true, command_allowlisted: false, allowlist_configured: true,
      server_status: 'active', discovery_status: 'command_not_allowlisted',
    });
    expect(badge.level).toBe('error');
    expect(badge.label).toBe('配置未授权');
  });

  it('returns warning for not_discovered', () => {
    const badge = mcpDiscoveryBadge({
      server_id: 's', display_name: 'S', enabled: true, transport: 'stdio',
      command_ref_present: true, command_allowlisted: true, allowlist_configured: true,
      server_status: 'active', discovery_status: 'not_discovered',
    });
    expect(badge.level).toBe('warning');
  });
});

describe('validationErrorBadge', () => {
  it('maps error severity', () => {
    const badge = validationErrorBadge({ code: 'TEST', message: 'msg', severity: 'error' });
    expect(badge.level).toBe('error');
    expect(badge.label).toBe('TEST');
    expect(badge.detail).toBe('msg');
  });

  it('maps warning severity', () => {
    const badge = validationErrorBadge({ code: 'WARN', message: 'warn msg', severity: 'warning' });
    expect(badge.level).toBe('warning');
  });
});

describe('validationBadges', () => {
  it('returns empty for null validation', () => {
    expect(validationBadges(null)).toEqual([]);
  });

  it('shows nothing when valid with no entries', () => {
    expect(validationBadges({ valid: true, errors: [] })).toEqual([]);
  });

  it('still shows warnings when valid=true (warning-only)', () => {
    const badges = validationBadges({
      valid: true,
      errors: [
        { code: 'MCP_ENABLED_NO_SERVERS', message: 'no servers', severity: 'warning' },
        { code: 'SKILLS_ENABLED_NO_BINDINGS', message: 'no bindings', severity: 'warning' },
      ],
    });
    expect(badges).toHaveLength(2);
    expect(badges.every((b) => b.level === 'warning')).toBe(true);
    expect(badges[0].label).toBe('MCP_ENABLED_NO_SERVERS');
  });

  it('shows both errors and warnings when invalid', () => {
    const badges = validationBadges({
      valid: false,
      errors: [
        { code: 'NO_MODEL_RESOLVED', message: 'no model', severity: 'error' },
        { code: 'MCP_ENABLED_NO_SERVERS', message: 'no servers', severity: 'warning' },
      ],
    });
    expect(badges.map((b) => b.level)).toEqual(['error', 'warning']);
  });
});

describe('summarizeValidation', () => {
  it('success when no entries', () => {
    const s = summarizeValidation({ valid: true, errors: [] });
    expect(s.level).toBe('success');
    expect(s.message).toBe('能力配置验证通过');
  });

  it('warning when valid=true but warnings present', () => {
    const s = summarizeValidation({
      valid: true,
      errors: [{ code: 'MCP_ENABLED_NO_SERVERS', message: 'x', severity: 'warning' }],
    });
    expect(s.level).toBe('warning');
    expect(s.warnCount).toBe(1);
    expect(s.message).toContain('验证通过');
    expect(s.message).toContain('1 个警告');
  });

  it('error (with warning count) when errors present', () => {
    const s = summarizeValidation({
      valid: false,
      errors: [
        { code: 'NO_MODEL_RESOLVED', message: 'x', severity: 'error' },
        { code: 'MCP_ENABLED_NO_SERVERS', message: 'y', severity: 'warning' },
      ],
    });
    expect(s.level).toBe('error');
    expect(s.message).toContain('1 个错误');
    expect(s.message).toContain('1 个警告');
  });

  it('handles null defensively', () => {
    const s = summarizeValidation(null);
    expect(s.level).toBe('success');
  });
});

describe('snapshotToViewModel', () => {
  it('produces all sections', () => {
    const vm = snapshotToViewModel(makeSnapshot());
    expect(vm.basicInfo).toBeDefined();
    expect(vm.tools).toBeDefined();
    expect(vm.mcp).toBeDefined();
    expect(vm.skills).toBeDefined();
    expect(vm.subagents).toBeDefined();
    expect(vm.cache).toBeDefined();
  });
});
