<script setup lang="ts">
import type { OntologyObject, OntologyAction } from '../composables/useAiConfigCenter';
import SvgIcon from '../../../components/ui/SvgIcon.vue';
import UiBanner from '../../../components/ui/UiBanner.vue';
import UiButton from '../../../components/ui/UiButton.vue';
import UiPill from '../../../components/ui/UiPill.vue';
import UiSearch from '../../../components/ui/UiSearch.vue';
import UiSkeleton from '../../../components/ui/UiSkeleton.vue';
import EmptyState from '../../../components/ui/EmptyState.vue';

defineProps<{
  activeTab: 'objects' | 'actions';
  searchQuery: string;
  loading: boolean;
  filteredObjects: OntologyObject[];
  filteredActions: OntologyAction[];
}>();
const emit = defineEmits<{
  'update:searchQuery': [value: string];
  refresh: [];
}>();

type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';

/* 风险等级 → 四声：低=ok，常规=mute，中=warn，高/严重=danger */
function getRiskTone(level: string): PillTone {
  const map: Record<string, PillTone> = {
    LOW: 'ok',
    NORMAL: 'mute',
    MEDIUM: 'warn',
    HIGH: 'danger',
    CRITICAL: 'danger',
  };
  return map[level] || 'mute';
}
</script>

<template>
  <div>
    <div class="content-header">
      <div class="content-heading">
        <div class="content-title">
          {{ activeTab === 'objects' ? '对象定义' : '动作定义' }}
        </div>
        <div class="content-subtitle">
          {{ activeTab === 'objects'
            ? '浏览当前生效 Ontology 对象类型定义、字段、关系与动作'
            : '浏览当前生效 Ontology 动作定义、参数、风险和审批策略' }}
        </div>
      </div>
    </div>

    <UiBanner tone="warn" class="readonly-banner">
      <SvgIcon src="/frontend/icons/forbidden.svg" />
      <span><strong>Ontology 只读视图：</strong> 本页展示 Rust API 当前生效的对象与动作定义；变更请通过受控后端配置流程完成。</span>
    </UiBanner>

    <div class="content-body">
      <div class="section-toolbar">
        <UiSearch
          :model-value="searchQuery"
          label="搜索对象或动作"
          :placeholder="activeTab === 'objects' ? '搜索对象 (名称/描述/标签)...' : '搜索动作 (名称/对象类型/描述)...'"
          @update:model-value="emit('update:searchQuery', $event)"
        />
        <div class="toolbar-right">
          <UiButton variant="ghost" @click="emit('refresh')">
            <SvgIcon src="/frontend/icons/refresh.svg" :size="14" />
            刷新
          </UiButton>
        </div>
      </div>

      <div
        v-if="loading"
        class="loading-skeleton"
        role="status"
        aria-busy="true"
        aria-label="正在加载数据"
      >
        <UiSkeleton v-for="i in 6" :key="i" height="36px" />
      </div>

      <div v-else-if="activeTab === 'objects'" class="table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>描述</th>
              <th>属性</th>
              <th>关系</th>
              <th>标签</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="obj in filteredObjects" :key="obj.id">
              <td>
                <div class="cell-primary">
                  {{ obj.name }}
                </div>
                <div class="cell-secondary">
                  {{ obj.plural_name }}
                </div>
              </td>
              <td class="cell-desc">
                {{ obj.description || '-' }}
              </td>
              <td class="cell-count">
                {{ obj.properties.length }}
              </td>
              <td class="cell-count">
                {{ obj.relationships.length }}
              </td>
              <td>
                <div class="tags-cell">
                  <UiPill v-for="tag in obj.tags.slice(0, 2)" :key="tag" tone="act">
                    {{ tag }}
                  </UiPill>
                  <UiPill v-if="obj.tags.length > 2" tone="mute">
                    +{{ obj.tags.length - 2 }}
                  </UiPill>
                </div>
              </td>
              <td>
                <UiPill :tone="obj.is_active ? 'ok' : 'mute'">
                  {{ obj.is_active ? '启用' : '禁用' }}
                </UiPill>
              </td>
            </tr>
          </tbody>
        </table>
        <EmptyState v-if="filteredObjects.length === 0" icon="search" title="暂无数据" />
      </div>

      <div v-else class="table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>动作名称</th>
              <th>对象类型</th>
              <th>描述</th>
              <th>参数</th>
              <th>风险等级</th>
              <th>需审批</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="action in filteredActions" :key="action.id">
              <td>
                <div class="cell-primary">
                  {{ action.name }}
                </div>
              </td>
              <td>
                <UiPill tone="act">
                  {{ action.object_type }}
                </UiPill>
              </td>
              <td class="cell-desc">
                {{ action.description || '-' }}
              </td>
              <td class="cell-count">
                {{ action.parameters.length }}
              </td>
              <td>
                <UiPill :tone="getRiskTone(action.risk_level)">
                  {{ action.risk_level }}
                </UiPill>
              </td>
              <td>
                <UiPill :tone="action.requires_approval ? 'warn' : 'mute'">
                  {{ action.requires_approval ? '是' : '否' }}
                </UiPill>
              </td>
              <td>
                <UiPill :tone="action.is_active ? 'ok' : 'mute'">
                  {{ action.is_active ? '启用' : '禁用' }}
                </UiPill>
              </td>
            </tr>
          </tbody>
        </table>
        <EmptyState v-if="filteredActions.length === 0" icon="search" title="暂无数据" />
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 私造件归库后的页面余量：只读横幅与骨架行的排版，形都在库件里 */
.readonly-banner {
  margin-bottom: var(--s3);
}

.loading-skeleton {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
}
</style>
