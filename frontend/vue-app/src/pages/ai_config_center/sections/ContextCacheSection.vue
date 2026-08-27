<script setup lang="ts">
import { computed } from 'vue';
import type { ModelsTabForm } from '../composables/useAiConfigCenter';
import type { CacheMetricsSummary } from '../aiConfigTypes';
import UiButton from '../../../components/ui/UiButton.vue';
import UiTable from '../../../components/ui/UiTable.vue';

const props = defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
  selectedEntityId: string;
  cacheMetrics: CacheMetricsSummary | null;
  cacheLoading: boolean;
}>();

const form = computed(() => props.modelsForm);
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
          v-model="form.context_strategy"
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
          v-model.number="form.max_context_tokens"
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
          v-model.number="form.compression_threshold_tokens"
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
          v-model.number="form.preserve_recent_messages"
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
          v-model.number="form.summary_max_tokens"
          type="number"
          min="1"
          step="1"
          class="form-input"
        >
      </div>
      <label class="checkbox-label form-group">
        <input v-model="form.persist_summaries" type="checkbox">
        <span>持久化摘要</span>
      </label>
      <div class="form-group">
        <label for="risk-ceiling">信封风险上限</label>
        <select id="risk-ceiling" v-model="form.risk_ceiling" class="form-select">
          <option value="low">low</option>
          <option value="medium">medium</option>
          <option value="high">high</option>
          <option value="critical">critical</option>
        </select>
      </div>
    </div>
    <div class="capability-grid">
      <label class="checkbox-label">
        <input v-model="form.cache_enabled" type="checkbox">
        <span>启用缓存策略</span>
      </label>
      <label class="checkbox-label">
        <input v-model="form.provider_prompt_cache_enabled" type="checkbox">
        <span>Provider Prompt Cache</span>
      </label>
      <label class="checkbox-label">
        <input v-model="form.tool_result_cache_enabled" type="checkbox">
        <span>工具结果缓存</span>
      </label>
      <label class="checkbox-label">
        <input v-model="form.mcp_resource_cache_enabled" type="checkbox">
        <span>MCP Resource Cache</span>
      </label>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="provider-cache-retention">Prompt Cache Retention</label>
        <input
          id="provider-cache-retention"
          v-model="form.provider_prompt_cache_retention"
          type="text"
          class="form-input"
          placeholder="24h"
        >
      </div>
      <div class="form-group">
        <label for="context-cache-backend">Context Cache Backend</label>
        <select
          id="context-cache-backend"
          v-model="form.context_cache_backend"
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
          v-model.number="form.context_cache_ttl_seconds"
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
          v-model.number="form.tool_result_cache_ttl_seconds"
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
          v-model.number="form.mcp_resource_cache_ttl_seconds"
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
        v-model="form.cacheable_tools"
        class="form-textarea form-textarea-mono"
        rows="3"
        placeholder="get_flight_status, search_flights"
      />
    </div>

    <div class="cache-metrics">
      <div class="cache-metrics__head">
        <div class="tool-category-title">
          Cache Metrics (24h)
        </div>
        <div class="cache-metrics__verbs">
          <UiButton :disabled="cacheLoading || !selectedEntityId" @click="emit('loadCacheMetrics')">
            {{ cacheLoading ? '加载中...' : '刷新' }}
          </UiButton>
          <UiButton :disabled="!selectedEntityId" @click="emit('invalidateCache')">
            失效缓存
          </UiButton>
        </div>
      </div>
      <p v-if="!cacheMetrics && !cacheLoading" class="cache-metrics__void">
        暂无缓存指标
      </p>
      <UiTable v-if="cacheMetrics && cacheMetrics.by_cache_type.length > 0" label="缓存指标" :sticky-head="false">
        <thead>
          <tr>
            <th>Cache Type</th>
            <th data-align="end">
              Events
            </th>
            <th data-align="end">
              Hits
            </th>
            <th data-align="end">
              Misses
            </th>
            <th data-align="end">
              Hit Rate
            </th>
            <th data-align="end">
              Cached Tokens
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in cacheMetrics.by_cache_type" :key="row.cache_type">
            <td data-mono>
              {{ row.cache_type }}
            </td>
            <td data-align="end">
              {{ row.total_events }}
            </td>
            <td data-align="end">
              {{ row.hits }}
            </td>
            <td data-align="end">
              {{ row.misses }}
            </td>
            <td data-align="end">
              {{ row.total_events > 0 ? ((row.hits / row.total_events) * 100).toFixed(1) + '%' : '-' }}
            </td>
            <td data-align="end">
              {{ row.total_cached_tokens }}
            </td>
          </tr>
        </tbody>
      </UiTable>
    </div>
  </fieldset>
</template>

<style scoped>
/* 指标段的形：标题与动作各居一端，留白走梯度 */
.cache-metrics {
  margin-top: var(--s3);
}

.cache-metrics__head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--s2);
}

.cache-metrics__verbs {
  display: flex;
  gap: var(--s2);
}

.cache-metrics__void {
  margin: 0;
  padding: var(--s1) 0;
  color: var(--ink-muted);
  font-size: var(--fs-body);
}
</style>
