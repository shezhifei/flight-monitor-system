<script setup lang="ts">
import type { OntologyObject, OntologyAction } from '../composables/useAiConfigCenter';
import SvgIcon from '../../../components/ui/SvgIcon.vue';

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

function getRiskBadgeClass(level: string): string {
  const map: Record<string, string> = {
    LOW: 'badge-low',
    NORMAL: 'badge-normal',
    MEDIUM: 'badge-medium',
    HIGH: 'badge-high',
    CRITICAL: 'badge-critical',
  };
  return map[level] || 'badge-normal';
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

    <div class="warning-banner">
      <SvgIcon src="/frontend/icons/forbidden.svg" />
      <strong>Ontology 只读视图：</strong> 本页展示 Rust API 当前生效的对象与动作定义；变更请通过受控后端配置流程完成。
    </div>

    <div class="content-body">
      <div class="section-toolbar">
        <div class="search-group">
          <span class="search-icon"><SvgIcon src="/frontend/icons/search.svg" :size="16" /></span>
          <input
            :value="searchQuery"
            type="text"
            class="search-input"
            :placeholder="activeTab === 'objects' ? '搜索对象 (名称/描述/标签)...' : '搜索动作 (名称/对象类型/描述)...'"
            @input="emit('update:searchQuery', ($event.target as HTMLInputElement).value)"
          >
        </div>
        <div class="toolbar-right">
          <button class="btn btn-secondary" @click="emit('refresh')">
            <SvgIcon src="/frontend/icons/refresh.svg" :size="14" style="vertical-align: -2px;" />
            刷新
          </button>
        </div>
      </div>

      <div v-if="loading" class="loading-container">
        <div class="spinner" />
        <p style="margin-top:16px;color:#64748b">
          正在加载数据...
        </p>
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
                  <span v-for="tag in obj.tags.slice(0, 2)" :key="tag" class="tag">{{ tag }}</span>
                  <span v-if="obj.tags.length > 2" class="tag tag-more">+{{ obj.tags.length - 2 }}</span>
                </div>
              </td>
              <td>
                <span :class="['badge', obj.is_active ? 'badge-green' : 'badge-gray']">
                  {{ obj.is_active ? '启用' : '禁用' }}
                </span>
              </td>
            </tr>
          </tbody>
        </table>
        <div v-if="filteredObjects.length === 0" class="empty-state">
          <p>暂无数据</p>
        </div>
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
                <span class="badge badge-blue">{{ action.object_type }}</span>
              </td>
              <td class="cell-desc">
                {{ action.description || '-' }}
              </td>
              <td class="cell-count">
                {{ action.parameters.length }}
              </td>
              <td>
                <span :class="['badge', getRiskBadgeClass(action.risk_level)]">
                  {{ action.risk_level }}
                </span>
              </td>
              <td>
                <span :class="['badge', action.requires_approval ? 'badge-orange' : 'badge-gray']">
                  {{ action.requires_approval ? '是' : '否' }}
                </span>
              </td>
              <td>
                <span :class="['badge', action.is_active ? 'badge-green' : 'badge-gray']">
                  {{ action.is_active ? '启用' : '禁用' }}
                </span>
              </td>
            </tr>
          </tbody>
        </table>
        <div v-if="filteredActions.length === 0" class="empty-state">
          <p>暂无数据</p>
        </div>
      </div>
    </div>
  </div>
</template>
