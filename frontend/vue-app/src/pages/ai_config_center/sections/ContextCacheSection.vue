<script setup lang="ts">
import type { ModelsTabForm } from '../composables/useAiConfigCenter';
import type { CacheMetricsSummary } from '../aiConfigTypesV2';

defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
  selectedEntityId: string;
  cacheMetrics: CacheMetricsSummary | null;
  cacheLoading: boolean;
}>();
const emit = defineEmits<{
  loadCacheMetrics: [];
  invalidateCache: [];
}>();
</script>

<template>
  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>上下文压缩与缓存</legend>
    <div class="form-row">
      <div class="form-group">
        <label for="context-strategy">上下文策略</label>
        <select
          id="context-strategy"
          v-model="modelsForm.context_strategy"
          class="form-select"
        >
          <option value="sliding_window">
            sliding_window
          </option>
          <option value="summary">
            summary
          </option>
          <option value="hybrid">
            hybrid
          </option>
        </select>
      </div>
      <div class="form-group">
        <label for="max-context-tokens">最大上下文 Tokens</label>
        <input
          id="max-context-tokens"
          v-model.number="modelsForm.max_context_tokens"
          type="number"
          min="1"
          step="1"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="compression-threshold">压缩阈值 Tokens</label>
        <input
          id="compression-threshold"
          v-model.number="modelsForm.compression_threshold_tokens"
          type="number"
          min="1"
          step="1"
          class="form-input"
        >
      </div>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="preserve-recent-messages">保留最近消息数</label>
        <input
          id="preserve-recent-messages"
          v-model.number="modelsForm.preserve_recent_messages"
          type="number"
          min="0"
          step="1"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="summary-max-tokens">摘要最大 Tokens</label>
        <input
          id="summary-max-tokens"
          v-model.number="modelsForm.summary_max_tokens"
          type="number"
          min="1"
          step="1"
          class="form-input"
        >
      </div>
      <label class="checkbox-label form-group">
        <input v-model="modelsForm.persist_summaries" type="checkbox">
        <span>持久化摘要</span>
      </label>
    </div>
    <div class="capability-grid">
      <label class="checkbox-label">
        <input v-model="modelsForm.cache_enabled" type="checkbox">
        <span>启用缓存策略</span>
      </label>
      <label class="checkbox-label">
        <input v-model="modelsForm.provider_prompt_cache_enabled" type="checkbox">
        <span>Provider Prompt Cache</span>
      </label>
      <label class="checkbox-label">
        <input v-model="modelsForm.tool_result_cache_enabled" type="checkbox">
        <span>工具结果缓存</span>
      </label>
      <label class="checkbox-label">
        <input v-model="modelsForm.mcp_resource_cache_enabled" type="checkbox">
        <span>MCP Resource Cache</span>
      </label>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="provider-cache-retention">Prompt Cache Retention</label>
        <input
          id="provider-cache-retention"
          v-model="modelsForm.provider_prompt_cache_retention"
          type="text"
          class="form-input"
          placeholder="24h"
        >
      </div>
      <div class="form-group">
        <label for="context-cache-backend">Context Cache Backend</label>
        <select
          id="context-cache-backend"
          v-model="modelsForm.context_cache_backend"
          class="form-select"
        >
          <option value="memory">
            memory
          </option>
          <option value="redis">
            redis
          </option>
        </select>
      </div>
      <div class="form-group">
        <label for="context-cache-ttl">Context Cache TTL</label>
        <input
          id="context-cache-ttl"
          v-model.number="modelsForm.context_cache_ttl_seconds"
          type="number"
          min="0"
          step="1"
          class="form-input"
        >
      </div>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="tool-cache-ttl">Tool Cache TTL</label>
        <input
          id="tool-cache-ttl"
          v-model.number="modelsForm.tool_result_cache_ttl_seconds"
          type="number"
          min="0"
          step="1"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="mcp-resource-cache-ttl">MCP Resource Cache TTL</label>
        <input
          id="mcp-resource-cache-ttl"
          v-model.number="modelsForm.mcp_resource_cache_ttl_seconds"
          type="number"
          min="0"
          step="1"
          class="form-input"
        >
      </div>
    </div>
    <div class="form-group">
      <label for="cacheable-tools">可缓存工具</label>
      <textarea
        id="cacheable-tools"
        v-model="modelsForm.cacheable_tools"
        class="form-textarea form-textarea-mono"
        rows="3"
        placeholder="get_flight_status, search_flights"
      />
    </div>

    <div style="margin-top:12px;">
      <div style="display:flex;justify-content:space-between;align-items:center;">
        <div class="tool-category-title">
          Cache Metrics (24h)
        </div>
        <div style="display:flex;gap:6px;">
          <button
            type="button"
            class="btn btn-secondary btn-sm"
            :disabled="cacheLoading || !selectedEntityId"
            @click="emit('loadCacheMetrics')"
          >
            {{ cacheLoading ? '加载中...' : '刷新' }}
          </button>
          <button
            type="button"
            class="btn btn-secondary btn-sm"
            :disabled="!selectedEntityId"
            @click="emit('invalidateCache')"
          >
            失效缓存
          </button>
        </div>
      </div>
      <div v-if="!cacheMetrics && !cacheLoading" style="color:var(--text-secondary,#888);font-size:13px;padding:4px 0;">
        暂无缓存指标
      </div>
      <table v-if="cacheMetrics && cacheMetrics.by_cache_type.length > 0" style="width:100%;font-size:13px;border-collapse:collapse;margin-top:4px;">
        <thead>
          <tr style="text-align:left;border-bottom:1px solid var(--border-color,#ddd);">
            <th style="padding:4px 8px;">Cache Type</th>
            <th style="padding:4px 8px;">Events</th>
            <th style="padding:4px 8px;">Hits</th>
            <th style="padding:4px 8px;">Misses</th>
            <th style="padding:4px 8px;">Hit Rate</th>
            <th style="padding:4px 8px;">Cached Tokens</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in cacheMetrics.by_cache_type" :key="row.cache_type" style="border-bottom:1px solid var(--border-color,#eee);">
            <td style="padding:4px 8px;"><code>{{ row.cache_type }}</code></td>
            <td style="padding:4px 8px;">{{ row.total_events }}</td>
            <td style="padding:4px 8px;">{{ row.hits }}</td>
            <td style="padding:4px 8px;">{{ row.misses }}</td>
            <td style="padding:4px 8px;">{{ row.total_events > 0 ? ((row.hits / row.total_events) * 100).toFixed(1) + '%' : '-' }}</td>
            <td style="padding:4px 8px;">{{ row.total_cached_tokens }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </fieldset>
</template>
