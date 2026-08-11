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
          <span :class="['badge', vm.tools.total === 0 ? 'badge-orange' : 'badge-green']">
            {{ vm.tools.total }}
          </span>
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
            <span :class="['badge', vm.tools.subagentToolEnabled ? 'badge-green' : 'badge-gray']">
              {{ vm.tools.subagentToolEnabled ? '启用' : '禁用' }}
            </span>
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
          <span :class="['badge', vm.mcp.enabled ? 'badge-green' : 'badge-gray']">
            {{ vm.mcp.enabled ? '启用' : '禁用' }}
          </span>
          <span v-if="vm.mcp.enabled" class="cap-section-sub">{{ vm.mcp.serverCount }} 服务器</span>
        </summary>
        <div v-if="vm.mcp.noServersWarning" class="cap-alert cap-alert-warning">
          {{ vm.mcp.noServersWarning.label }}
        </div>
        <div v-if="vm.mcp.enabled" class="cap-kv-grid">
          <div class="cap-kv">
            <span class="cap-kv-label">Allowlist 配置</span>
            <span :class="['badge', vm.mcp.allowlistConfigured ? 'badge-green' : 'badge-orange']">
              {{ vm.mcp.allowlistConfigured ? '已配置' : '未配置' }}
            </span>
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
                <span v-if="!srv.commandRefPresent" class="badge badge-gray">无</span>
                <span v-else-if="srv.commandAllowlisted === false" class="badge badge-red">未授权</span>
                <span v-else class="badge badge-green">已授权</span>
              </td>
              <td>
                <span :class="['badge', statusBadgeClass(srv.statusBadge)]">
                  {{ srv.statusBadge.label }}
                </span>
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
          <span :class="['badge', vm.skills.enabled ? 'badge-green' : 'badge-gray']">
            {{ vm.skills.enabled ? '启用' : '禁用' }}
          </span>
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
            <span :class="['badge', vm.skills.failClosed ? 'badge-orange' : 'badge-gray']">
              {{ vm.skills.failClosed ? '是' : '否' }}
            </span>
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
          <span :class="['badge', vm.subagents.enabled ? 'badge-green' : 'badge-gray']">
            {{ vm.subagents.enabled ? '启用' : '禁用' }}
          </span>
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
            <span :class="['badge', vm.subagents.inheritParentContext ? 'badge-green' : 'badge-gray']">
              {{ vm.subagents.inheritParentContext ? '是' : '否' }}
            </span>
          </div>
        </div>
      </details>

      <!-- 6. Cache -->
      <details class="cap-section">
        <summary class="cap-section-title">
          缓存
          <span :class="['badge', vm.cache.enabled ? 'badge-green' : 'badge-gray']">
            {{ vm.cache.enabled ? '启用' : '禁用' }}
          </span>
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
                <span :class="['badge', cacheBackendBadgeClass(b)]">
                  {{ b.note }}
                </span>
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
import type {
  EnrichedCapabilitySnapshot,
  ValidationResult,
  CacheMetricsSummary,
} from './aiConfigTypesV2';
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

function statusBadgeClass(badge: StatusBadge): string {
  switch (badge.level) {
    case 'ok': return 'badge-green';
    case 'disabled': return 'badge-gray';
    case 'warning': return 'badge-orange';
    case 'error': return 'badge-red';
    default: return 'badge-gray';
  }
}

function cacheBackendBadgeClass(backend: CacheBackendRow): string {
  if (backend.note === 'adapter_available') return 'badge-blue';
  if (backend.enabled) return 'badge-green';
  return 'badge-gray';
}
</script>

<style scoped>
.capability-status-panel {
  font-size: 13px;
  line-height: 1.5;
}

.cap-loading {
  padding: 12px;
  color: var(--text-tertiary);
  text-align: center;
}

.cap-error {
  padding: 12px;
  color: var(--ws-danger);
  background: var(--dh-signal-critical-soft);
  border-radius: 4px;
}

/* Validation */
.cap-validation {
  margin-bottom: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.cap-validation-item {
  padding: 6px 10px;
  border-radius: 4px;
  font-size: 12px;
  display: flex;
  gap: 8px;
  align-items: center;
}

.cap-validation-error {
  background: var(--dh-signal-critical-soft);
  color: var(--ws-danger);
}

.cap-validation-warning {
  background: #ffedd5;
  color: var(--ws-warn);
}

.cap-validation-code {
  font-weight: 600;
  font-family: monospace;
  white-space: nowrap;
}

.cap-validation-detail {
  color: inherit;
  opacity: 0.8;
}

/* Section (collapsible) */
.cap-section {
  border: 1px solid var(--border-light);
  border-radius: 6px;
  margin-bottom: 8px;
  background: var(--bg-card);
}

.cap-section-title {
  padding: 8px 12px;
  font-weight: 600;
  font-size: 13px;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 8px;
  user-select: none;
  list-style: none;
}

.cap-section-title::-webkit-details-marker {
  display: none;
}

.cap-section-title::before {
  content: '▸';
  font-size: 11px;
  transition: transform 0.15s;
}

details[open] > .cap-section-title::before {
  transform: rotate(90deg);
}

.cap-section-sub {
  font-weight: 400;
  font-size: 12px;
  color: var(--text-tertiary);
}

/* KV Grid */
.cap-kv-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px 16px;
  padding: 8px 12px;
  border-top: 1px solid #f1f5f9;
}

.cap-kv-grid-3 {
  grid-template-columns: 1fr 1fr 1fr;
}

.cap-kv {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0;
}

.cap-kv-label {
  color: var(--text-tertiary);
  font-size: 12px;
  white-space: nowrap;
}

.cap-kv-value {
  font-size: 12px;
  font-family: monospace;
  word-break: break-all;
}

/* Badge (matching parent page) */
.badge {
  display: inline-block;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
}

.badge-green { background: var(--dh-signal-ok-soft); color: var(--status-text-departed); }
.badge-red { background: var(--dh-signal-critical-soft); color: var(--ws-danger); }
.badge-blue { background: #dbeafe; color: #1e40af; }
.badge-gray { background: var(--ws-surface-muted); color: var(--text-tertiary); }
.badge-orange { background: #ffedd5; color: var(--ws-warn); }

/* Table */
.cap-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
  border-top: 1px solid #f1f5f9;
}

.cap-table th {
  text-align: left;
  padding: 6px 12px;
  font-weight: 600;
  color: var(--text-secondary);
  background: var(--bg-page);
  border-bottom: 1px solid var(--border-light);
}

.cap-table td {
  padding: 5px 12px;
  border-bottom: 1px solid #f1f5f9;
  vertical-align: middle;
}

.cap-table code {
  font-size: 11px;
  background: var(--ws-surface-muted);
  padding: 1px 4px;
  border-radius: 3px;
}

.cap-detail-cell {
  color: var(--text-tertiary);
  font-size: 11px;
}

.cap-detail-hint {
  cursor: help;
  color: var(--system-gray2);
  font-size: 11px;
  margin-left: 2px;
}

/* Alerts */
.cap-alert {
  padding: 6px 10px;
  border-radius: 4px;
  font-size: 12px;
  margin: 4px 12px 8px;
}

.cap-alert-warning {
  background: #ffedd5;
  color: var(--ws-warn);
}

.cap-alert-error {
  background: var(--dh-signal-critical-soft);
  color: var(--ws-danger);
}

.cap-alert-detail {
  font-weight: 400;
  opacity: 0.8;
}

.cap-metrics-title {
  padding: 8px 12px 4px;
  font-weight: 600;
  font-size: 12px;
  color: var(--text-secondary);
  border-top: 1px solid #f1f5f9;
}

.cap-muted {
  padding: 8px 12px;
  color: var(--system-gray2);
  font-size: 12px;
}
</style>
