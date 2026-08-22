<template>
  <div class="capability-status-panel">
    <!-- Loading -->
    <div v-if="loading" class="cap-loading">
      加载中...
    </div>

    <!-- Error -->
    <div v-else-if="error" class="cap-error">
      {{ error }}
    </div>

    <!-- Content -->
    <template v-else-if="vm">
      <!-- Validation Errors -->
      <div v-if="validationErrors.length > 0" class="cap-validation">
        <div
          v-for="(err, i) in validationErrors"
          :key="i"
          :class="['cap-validation-item', err.level === 'error' ? 'cap-validation-error' : 'cap-validation-warning']"
        >
          <span class="cap-validation-code">{{ err.label }}</span>
          <span v-if="err.detail" class="cap-validation-detail">{{ err.detail }}</span>
        </div>
      </div>

      <!-- 1. Basic Info -->
      <details class="cap-section" open>
        <summary class="cap-section-title">
          基础信息
        </summary>
        <div class="cap-kv-grid">
          <div v-for="row in vm.basicInfo" :key="row.key" class="cap-kv">
            <span class="cap-kv-label">{{ row.label }}</span>
            <code class="cap-kv-value">{{ row.value }}</code>
          </div>
        </div>
      </details>

      <!-- 2. Tools -->
      <details class="cap-section" open>
        <summary class="cap-section-title">
          工具
          <UiPill :tone="vm.tools.total === 0 ? 'warn' : 'ok'">
            {{ vm.tools.total }}
          </UiPill>
        </summary>
        <div class="cap-kv-grid cap-kv-grid-3">
          <div class="cap-kv">
            <span class="cap-kv-label">Builtin</span>
            <code class="cap-kv-value">{{ vm.tools.builtin }}</code>
          </div>
          <div class="cap-kv">
            <span class="cap-kv-label">MCP</span>
            <code class="cap-kv-value">{{ vm.tools.mcp }}</code>
          </div>
          <div class="cap-kv">
            <span class="cap-kv-label">Subagent Tool</span>
            <UiPill :tone="vm.tools.subagentToolEnabled ? 'ok' : 'mute'">
              {{ vm.tools.subagentToolEnabled ? '启用' : '禁用' }}
            </UiPill>
          </div>
        </div>
        <div v-if="vm.tools.emptyWarning" class="cap-alert cap-alert-warning">
          {{ vm.tools.emptyWarning.label }}
        </div>
      </details>

      <!-- 3. MCP -->
      <details class="cap-section">
        <summary class="cap-section-title">
          MCP
          <UiPill :tone="vm.mcp.enabled ? 'ok' : 'mute'">
            {{ vm.mcp.enabled ? '启用' : '禁用' }}
          </UiPill>
          <span v-if="vm.mcp.enabled" class="cap-section-sub">{{ vm.mcp.serverCount }} 服务器</span>
        </summary>
        <div v-if="vm.mcp.noServersWarning" class="cap-alert cap-alert-warning">
          {{ vm.mcp.noServersWarning.label }}
        </div>
        <div v-if="vm.mcp.enabled" class="cap-kv-grid">
          <div class="cap-kv">
            <span class="cap-kv-label">Allowlist 配置</span>
            <UiPill :tone="vm.mcp.allowlistConfigured ? 'ok' : 'warn'">
              {{ vm.mcp.allowlistConfigured ? '已配置' : '未配置' }}
            </UiPill>
          </div>
        </div>
        <table v-if="vm.mcp.servers.length > 0" class="cap-table">
          <thead>
            <tr>
              <th>Server</th>
              <th>Transport</th>
              <th>Command Ref</th>
              <th>发现状态</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="srv in vm.mcp.servers" :key="srv.serverId">
              <td>{{ srv.displayName }}</td>
              <td><code>{{ srv.transport }}</code></td>
              <td>
                <UiPill v-if="!srv.commandRefPresent" tone="mute">
                  无
                </UiPill>
                <UiPill v-else-if="srv.commandAllowlisted === false" tone="danger">
                  未授权
                </UiPill>
                <UiPill v-else tone="ok">
                  已授权
                </UiPill>
              </td>
              <td>
                <UiPill :tone="statusBadgeTone(srv.statusBadge)">
                  {{ srv.statusBadge.label }}
                </UiPill>
                <span v-if="srv.statusBadge.detail" class="cap-detail-hint" :title="srv.statusBadge.detail">ⓘ</span>
              </td>
            </tr>
          </tbody>
        </table>
      </details>

      <!-- 4. Skills -->
      <details class="cap-section">
        <summary class="cap-section-title">
          Skills
          <UiPill :tone="vm.skills.enabled ? 'ok' : 'mute'">
            {{ vm.skills.enabled ? '启用' : '禁用' }}
          </UiPill>
          <span v-if="vm.skills.enabled" class="cap-section-sub">{{ vm.skills.skillCount }} 个</span>
        </summary>
        <div v-if="vm.skills.noBindingsWarning" class="cap-alert cap-alert-warning">
          {{ vm.skills.noBindingsWarning.label }}
        </div>
        <div v-if="vm.skills.enabled" class="cap-kv-grid cap-kv-grid-3">
          <div class="cap-kv">
            <span class="cap-kv-label">指令数</span>
            <code class="cap-kv-value">{{ vm.skills.instructionCount }}</code>
          </div>
          <div class="cap-kv">
            <span class="cap-kv-label">Fail Closed</span>
            <UiPill :tone="vm.skills.failClosed ? 'warn' : 'mute'">
              {{ vm.skills.failClosed ? '是' : '否' }}
            </UiPill>
          </div>
        </div>
        <table v-if="vm.skills.bindings.length > 0" class="cap-table">
          <thead>
            <tr>
              <th>Skill</th>
              <th>版本</th>
              <th>策略</th>
              <th>优先级</th>
              <th>最大指令 Token</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="b in vm.skills.bindings" :key="b.skillSlug">
              <td>{{ b.skillSlug }}</td>
              <td><code>{{ b.version }}</code></td>
              <td>{{ b.activationPolicy }}</td>
              <td>{{ b.priority }}</td>
              <td>{{ b.maxInstructionTokens }}</td>
            </tr>
          </tbody>
        </table>
      </details>

      <!-- 5. Subagents -->
      <details class="cap-section">
        <summary class="cap-section-title">
          Subagents
          <UiPill :tone="vm.subagents.enabled ? 'ok' : 'mute'">
            {{ vm.subagents.enabled ? '启用' : '禁用' }}
          </UiPill>
        </summary>
        <div v-if="vm.subagents.riskBadge" :class="['cap-alert', vm.subagents.riskBadge.level === 'error' ? 'cap-alert-error' : 'cap-alert-warning']">
          {{ vm.subagents.riskBadge.label }}
          <span v-if="vm.subagents.riskBadge.detail" class="cap-alert-detail">— {{ vm.subagents.riskBadge.detail }}</span>
        </div>
        <div v-if="vm.subagents.enabled" class="cap-kv-grid">
          <div class="cap-kv">
            <span class="cap-kv-label">允许实体</span>
            <code class="cap-kv-value">{{ vm.subagents.allowedEntityIds.length > 0 ? vm.subagents.allowedEntityIds.join(', ') : '（空）' }}</code>
          </div>
          <div class="cap-kv">
            <span class="cap-kv-label">最大深度</span>
            <code class="cap-kv-value">{{ vm.subagents.maxDepth }}</code>
          </div>
          <div class="cap-kv">
            <span class="cap-kv-label">最大并发</span>
            <code class="cap-kv-value">{{ vm.subagents.maxConcurrency }}</code>
          </div>
          <div class="cap-kv">
            <span class="cap-kv-label">继承上下文</span>
            <UiPill :tone="vm.subagents.inheritParentContext ? 'ok' : 'mute'">
              {{ vm.subagents.inheritParentContext ? '是' : '否' }}
            </UiPill>
          </div>
        </div>
      </details>

      <!-- 6. Cache -->
      <details class="cap-section">
        <summary class="cap-section-title">
          缓存
          <UiPill :tone="vm.cache.enabled ? 'ok' : 'mute'">
            {{ vm.cache.enabled ? '启用' : '禁用' }}
          </UiPill>
        </summary>
        <table class="cap-table">
          <thead>
            <tr>
              <th>后端</th>
              <th>状态</th>
              <th>详情</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="b in vm.cache.backends" :key="b.key">
              <td>{{ b.label }}</td>
              <td>
                <UiPill :tone="cacheBackendBadgeTone(b)">
                  {{ b.note }}
                </UiPill>
              </td>
              <td class="cap-detail-cell">
                {{ b.detail || '—' }}
              </td>
            </tr>
          </tbody>
        </table>
        <div v-if="vm.cache.metrics.length > 0" class="cap-metrics-title">
          24h 缓存指标
        </div>
        <table v-if="vm.cache.metrics.length > 0" class="cap-table">
          <thead>
            <tr>
              <th>类型</th>
              <th>事件</th>
              <th>命中</th>
              <th>未命中</th>
              <th>命中率</th>
              <th>Cached Tokens</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="m in vm.cache.metrics" :key="m.cacheType">
              <td>{{ m.cacheType }}</td>
              <td>{{ m.events }}</td>
              <td>{{ m.hits }}</td>
              <td>{{ m.misses }}</td>
              <td>{{ m.hitRate }}</td>
              <td>{{ m.cachedTokens }}</td>
            </tr>
          </tbody>
        </table>
        <div v-else-if="vm.cache.enabled" class="cap-muted">
          暂无缓存指标数据
        </div>
      </details>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import UiPill from '@/components/ui/UiPill.vue';
import type {
  EnrichedCapabilitySnapshot,
  ValidationResult,
  CacheMetricsSummary,
} from './aiConfigTypes';
import {
  snapshotToViewModel,
  validationBadges,
  type StatusBadge,
  type CacheBackendRow,
} from './aiCapabilityViewModel';

const props = defineProps<{
  snapshot: EnrichedCapabilitySnapshot | null;
  validation: ValidationResult | null;
  cacheMetrics: CacheMetricsSummary | null;
  loading: boolean;
  error?: string | null;
}>();

const vm = computed(() => {
  if (!props.snapshot) return null;
  return snapshotToViewModel(props.snapshot, props.cacheMetrics);
});

// Show every validation entry (error + warning), regardless of `valid`.
// Warning-only results return valid=true from the backend, but those warnings
// are real configuration risks and must not be hidden.
const validationErrors = computed(() => validationBadges(props.validation));

type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';

function statusBadgeTone(badge: StatusBadge): PillTone {
  switch (badge.level) {
    case 'ok': return 'ok';
    case 'disabled': return 'mute';
    case 'warning': return 'warn';
    case 'error': return 'danger';
    default: return 'mute';
  }
}

function cacheBackendBadgeTone(backend: CacheBackendRow): PillTone {
  if (backend.note === 'adapter_available') return 'act';
  if (backend.enabled) return 'ok';
  return 'mute';
}
</script>

<style scoped>
/* 状态章归 UiPill；这里只留面板骨架与键值/表格排版。 */
.capability-status-panel {
  font-size: var(--fs-body);
  line-height: 1.5;
}

.cap-loading {
  padding: var(--s3);
  color: var(--ink-muted);
  text-align: center;
}

.cap-error {
  padding: var(--s3);
  color: var(--danger);
  background: var(--danger-soft);
  border-radius: var(--r-cell);
}

/* Validation */
.cap-validation {
  margin-bottom: var(--s3);
  display: flex;
  flex-direction: column;
  gap: var(--s1);
}

.cap-validation-item {
  padding: var(--s2) var(--s3);
  border-radius: var(--r-cell);
  font-size: var(--fs-label);
  display: flex;
  gap: var(--s2);
  align-items: center;
}

.cap-validation-error {
  background: var(--danger-soft);
  color: var(--danger);
}

.cap-validation-warning {
  background: var(--warn-soft);
  color: var(--warn);
}

.cap-validation-code {
  font-weight: var(--fw-semibold);
  font-family: var(--mono);
  white-space: nowrap;
}

.cap-validation-detail {
  color: inherit;
  opacity: 0.8;
}

/* Section (collapsible) */
.cap-section {
  border: 1px solid var(--line-strong);
  border-radius: var(--r-cell);
  margin-bottom: var(--s2);
  background: var(--face-work);
}

.cap-section-title {
  padding: var(--s2) var(--s3);
  font-weight: var(--fw-semibold);
  font-size: var(--fs-body);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: var(--s2);
  user-select: none;
  list-style: none;
}

.cap-section-title::-webkit-details-marker {
  display: none;
}

.cap-section-title::before {
  content: '▸';
  /* 折叠指示符刻意小于字阶最小档 */
  font-size: 11px;
  transition: transform 0.15s;
}

details[open] > .cap-section-title::before {
  transform: rotate(90deg);
}

.cap-section-sub {
  font-weight: var(--fw-regular);
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

/* KV Grid */
.cap-kv-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--s1) var(--s4);
  padding: var(--s2) var(--s3);
  border-top: 1px solid var(--line);
}

.cap-kv-grid-3 {
  grid-template-columns: 1fr 1fr 1fr;
}

.cap-kv {
  display: flex;
  align-items: center;
  gap: var(--s2);
  padding: 2px 0;
}

.cap-kv-label {
  color: var(--ink-muted);
  font-size: var(--fs-label);
  white-space: nowrap;
}

.cap-kv-value {
  font-size: var(--fs-label);
  font-family: var(--mono);
  word-break: break-all;
}

/* Table */
.cap-table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--fs-label);
  border-top: 1px solid var(--line);
}

.cap-table th {
  text-align: left;
  padding: var(--s2) var(--s3);
  font-weight: var(--fw-semibold);
  color: var(--ink-subtle);
  background: var(--face-page);
  border-bottom: 1px solid var(--line-strong);
}

.cap-table td {
  padding: var(--s2) var(--s3);
  border-bottom: 1px solid var(--line);
  vertical-align: middle;
}

.cap-table code {
  font-size: var(--fs-label);
  font-family: var(--mono);
  background: var(--face-page);
  padding: 1px 4px;
  border-radius: var(--r-cell);
}

.cap-detail-cell {
  color: var(--ink-muted);
  font-size: var(--fs-label);
}

.cap-detail-hint {
  cursor: help;
  color: var(--ink-muted);
  font-size: var(--fs-label);
  margin-left: 2px;
}

/* Alerts */
.cap-alert {
  padding: var(--s2) var(--s3);
  border-radius: var(--r-cell);
  font-size: var(--fs-label);
  margin: var(--s1) var(--s3) var(--s2);
}

.cap-alert-warning {
  background: var(--warn-soft);
  color: var(--warn);
}

.cap-alert-error {
  background: var(--danger-soft);
  color: var(--danger);
}

.cap-alert-detail {
  font-weight: var(--fw-regular);
  opacity: 0.8;
}

.cap-metrics-title {
  padding: var(--s2) var(--s3) var(--s1);
  font-weight: var(--fw-semibold);
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  border-top: 1px solid var(--line);
}

.cap-muted {
  padding: var(--s2) var(--s3);
  color: var(--ink-muted);
  font-size: var(--fs-label);
}
</style>
