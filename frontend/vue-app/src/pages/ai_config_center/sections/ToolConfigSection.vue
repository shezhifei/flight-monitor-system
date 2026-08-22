<script setup lang="ts">
import { computed } from 'vue';
import type { ModelsTabForm } from '../composables/useAiConfigCenter';
import type { McpServerDefinition, McpEntityBinding, SkillRegistryEntry, SkillEntityBinding } from '../aiConfigTypes';
import EmptyState from '@/components/ui/EmptyState.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiCheckChip from '@/components/ui/UiCheckChip.vue';
import UiField from '@/components/ui/UiField.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import UiSwitch from '@/components/ui/UiSwitch.vue';
import UiTable from '@/components/ui/UiTable.vue';

const props = defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
  toolSourceOptions: { value: string; label: string }[];
  categoryToolMap: Record<string, string[]>;
  deniedToolsSet: Set<string>;
  allowedToolCategoriesSet: Set<string>;
  mcpServers: McpServerDefinition[];
  mcpBindings: McpEntityBinding[];
  mcpLoading: boolean;
  skillRegistry: SkillRegistryEntry[];
  skillBindings: SkillEntityBinding[];
  skillsLoading: boolean;
  selectedEntityId: string;
}>();

const form = computed(() => props.modelsForm);
const emit = defineEmits<{
  toggleToolSource: [value: string, enabled: boolean];
  toggleAllowedToolCategory: [category: string, enabled: boolean];
  toggleDeniedTool: [toolName: string, allowed: boolean];
  saveMcpBinding: [serverId: string];
  probeMcpServer: [serverId: string];
  loadMcpData: [entityId: string];
  saveSkillBinding: [slug: string];
  deleteSkillBinding: [bindingId: string];
  loadSkillData: [entityId: string];
}>();

const writeActionPolicyOptions = [
  { value: 'proposal_only', label: 'proposal_only' },
];

function chipId(prefix: string, value: string): string {
  return `${prefix}-${value.replace(/[^a-zA-Z0-9_-]+/g, '_')}`;
}
</script>

<template>
  <!-- Parent owns modelsForm; nested writes are the existing contract. -->
  <!-- eslint-disable vue/no-mutating-props -->
  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>工具权限</legend>
    <p class="models-section-hint">
      实体只声明工具来源和权限策略。Builtin 工具由后端目录提供，MCP 工具来自绑定的 MCP server，Skill 默认只注入指令上下文。
    </p>
    <div class="capability-grid">
      <div>
        <div class="tool-category-title">
          工具执行策略
        </div>
        <div class="checkbox-grid">
          <div class="tool-config__switch">
            <span class="tool-config__switch-label">启用工具调用</span>
            <UiSwitch
              v-model:checked="form.tooling_enabled"
              label="启用工具调用"
            />
          </div>
          <div class="tool-config__switch">
            <span class="tool-config__switch-label">允许并行工具</span>
            <UiSwitch
              v-model:checked="form.tooling_allow_parallel"
              label="允许并行工具"
            />
          </div>
        </div>
        <div class="form-row compact-row">
          <UiField label="最大轮数" for-id="tooling-max-rounds">
            <input
              id="tooling-max-rounds"
              v-model.number="form.tooling_max_rounds"
              type="number"
              min="0"
              max="20"
              step="1"
            >
          </UiField>
          <UiField label="写动作策略">
            <UiSelect
              id="write-action-policy"
              v-model="form.write_action_policy"
              :options="writeActionPolicyOptions"
              label="写动作策略"
              min-width="100%"
            />
          </UiField>
        </div>
      </div>
      <div>
        <div class="tool-category-title">
          工具来源
        </div>
        <div class="tool-config__chips" role="group" aria-label="工具来源">
          <UiCheckChip
            v-for="source in toolSourceOptions"
            :id="chipId('tool-source', source.value)"
            :key="source.value"
            :label="source.label"
            :checked="form.allowed_tool_sources.includes(source.value)"
            @update:checked="emit('toggleToolSource', source.value, $event)"
          />
        </div>
      </div>
    </div>

    <div v-if="Object.keys(categoryToolMap).length > 0" class="tool-category">
      <div class="tool-category-title">
        允许工具类别
      </div>
      <div class="tool-config__chips" role="group" aria-label="允许工具类别">
        <UiCheckChip
          v-for="(_, category) in categoryToolMap"
          :id="chipId('tool-cat', category)"
          :key="`category-${category}`"
          :label="category"
          :checked="allowedToolCategoriesSet.has(category)"
          @update:checked="emit('toggleAllowedToolCategory', category, $event)"
        />
      </div>
    </div>

    <p class="models-section-hint">
      取消勾选具体工具即列入 denied_tools，禁止该实体调用对应工具。已禁用：{{ form.denied_tools.length }}
    </p>
    <EmptyState
      v-if="Object.keys(categoryToolMap).length === 0"
      icon="data"
      title="暂无工具分类"
    />
    <div
      v-for="(toolNames, category) in categoryToolMap"
      :key="category"
      class="tool-category"
    >
      <div class="tool-category-title">
        {{ category }}
      </div>
      <div class="tool-config__chips" role="group" :aria-label="`${category} 工具`">
        <UiCheckChip
          v-for="toolName in toolNames"
          :id="chipId('tool', `${category}-${toolName}`)"
          :key="toolName"
          :label="toolName"
          :checked="!deniedToolsSet.has(toolName)"
          @update:checked="emit('toggleDeniedTool', toolName, $event)"
        />
      </div>
    </div>
  </fieldset>

  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>Subagents 编排</legend>
    <p class="models-section-hint">
      Subagents 通过"可委派 AI 实体"实现。被委派实体各自使用自己的模型、Prompt、MCP、Agent Skill 和工具能力。
    </p>
    <div class="form-row">
      <div class="tool-config__switch">
        <span class="tool-config__switch-label">启用 Subagents</span>
        <UiSwitch
          v-model:checked="form.subagents_enabled"
          label="启用 Subagents"
        />
      </div>
      <div class="tool-config__switch">
        <span class="tool-config__switch-label">继承父上下文摘要</span>
        <UiSwitch
          v-model:checked="form.subagents_inherit_parent_context"
          label="继承父上下文摘要"
        />
      </div>
      <div class="tool-config__switch">
        <span class="tool-config__switch-label">要求模型支持工具调用</span>
        <UiSwitch
          v-model:checked="form.subagents_require_tool_calling_capability"
          label="要求模型支持工具调用"
        />
      </div>
    </div>
    <div class="form-group">
      <UiField label="可委派实体 ID" for-id="subagents-entities">
        <textarea
          id="subagents-entities"
          v-model="form.subagents_allowed_entity_ids"
          class="tool-config__mono"
          rows="3"
          placeholder="flight_dispatcher, ops_researcher"
        />
      </UiField>
    </div>
    <div class="form-row">
      <UiField label="最大委派深度" for-id="subagents-max-depth">
        <input
          id="subagents-max-depth"
          v-model.number="form.subagents_max_depth"
          type="number"
          min="1"
          max="5"
          step="1"
        >
      </UiField>
      <UiField label="最大并发" for-id="subagents-max-concurrency">
        <input
          id="subagents-max-concurrency"
          v-model.number="form.subagents_max_concurrency"
          type="number"
          min="1"
          max="16"
          step="1"
        >
      </UiField>
    </div>
    <div class="form-group">
      <UiField label="委派 Prompt" for-id="subagents-handoff-prompt">
        <textarea
          id="subagents-handoff-prompt"
          v-model="form.subagents_handoff_prompt"
          class="tool-config__mono"
          rows="4"
          placeholder="描述何时委派、如何汇总结果、哪些动作仍需父实体审批"
        />
      </UiField>
    </div>
  </fieldset>

  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>MCP 与 Agent Skill</legend>
    <div class="form-row">
      <div class="tool-config__switch">
        <span class="tool-config__switch-label">启用 MCP 绑定</span>
        <UiSwitch
          v-model:checked="form.mcp_enabled"
          label="启用 MCP 绑定"
        />
      </div>
      <div class="tool-config__switch">
        <span class="tool-config__switch-label">MCP 失败时关闭工具</span>
        <UiSwitch
          v-model:checked="form.mcp_fail_closed"
          label="MCP 失败时关闭工具"
        />
      </div>
      <div class="tool-config__switch">
        <span class="tool-config__switch-label">启用 Agent Skill</span>
        <UiSwitch
          v-model:checked="form.skills_enabled"
          label="启用 Agent Skill"
        />
      </div>
    </div>
    <div class="form-row">
      <UiField label="MCP Binding IDs (config)" for-id="mcp-binding-ids">
        <textarea
          id="mcp-binding-ids"
          v-model="form.mcp_binding_ids"
          class="tool-config__mono"
          rows="2"
          placeholder="default:ops-docs, default:weather"
        />
      </UiField>
      <UiField label="Agent Skill Allowlist (config)" for-id="skills-allowlist">
        <textarea
          id="skills-allowlist"
          v-model="form.skills_allowlist"
          class="tool-config__mono"
          rows="2"
          placeholder="Code, architecture-designer"
        />
      </UiField>
    </div>
    <div class="form-row">
      <UiField label="MCP Tool Prefix" for-id="mcp-tool-prefix">
        <input
          id="mcp-tool-prefix"
          v-model="form.mcp_tool_name_prefix"
          type="text"
        >
      </UiField>
      <UiField label="Discovery TTL (秒)" for-id="mcp-discovery-ttl">
        <input
          id="mcp-discovery-ttl"
          v-model.number="form.mcp_discovery_cache_ttl_seconds"
          type="number"
          min="0"
          step="1"
        >
      </UiField>
    </div>

    <div class="tool-config__block">
      <div class="tool-config__block-head">
        <div class="tool-category-title">
          MCP Servers
        </div>
        <UiButton
          variant="ghost"
          :disabled="mcpLoading || !selectedEntityId"
          @click="emit('loadMcpData', selectedEntityId)"
        >
          {{ mcpLoading ? '加载中...' : '刷新' }}
        </UiButton>
      </div>
      <EmptyState
        v-if="mcpServers.length === 0 && !mcpLoading"
        icon="data"
        title="暂无 MCP Server"
      />
      <UiTable
        v-if="mcpServers.length > 0"
        class="tool-config__table"
        label="MCP Servers"
        :sticky-head="false"
      >
        <thead>
          <tr>
            <th data-mono>
              ID
            </th>
            <th>名称</th>
            <th>传输</th>
            <th data-mono>
              命令
            </th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="srv in mcpServers" :key="srv.id">
            <td data-mono>
              {{ srv.id }}
            </td>
            <td>{{ srv.display_name }}</td>
            <td>{{ srv.transport }}</td>
            <td data-mono>
              {{ srv.command_ref || srv.endpoint_url || '—' }}
            </td>
            <td>
              <div class="tool-config__row-actions">
                <UiButton variant="tonal" @click="emit('saveMcpBinding', srv.id)">
                  绑定
                </UiButton>
                <UiButton variant="ghost" @click="emit('probeMcpServer', srv.id)">
                  探测
                </UiButton>
              </div>
            </td>
          </tr>
        </tbody>
      </UiTable>
      <div v-if="mcpBindings.length > 0" class="tool-config__subblock">
        <div class="tool-category-title">
          当前实体 MCP Bindings
        </div>
        <UiTable label="当前实体 MCP Bindings" :sticky-head="false">
          <thead>
            <tr>
              <th data-mono>
                Binding ID
              </th>
              <th data-mono>
                Server
              </th>
              <th>启用</th>
              <th>Allowed Tools</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="b in mcpBindings" :key="b.binding_id">
              <td data-mono>
                {{ b.binding_id }}
              </td>
              <td data-mono>
                {{ b.server_id }}
              </td>
              <td>{{ b.enabled ? '是' : '否' }}</td>
              <td>{{ b.allowed_tools?.join(', ') || '—' }}</td>
            </tr>
          </tbody>
        </UiTable>
      </div>
    </div>

    <div class="tool-config__block tool-config__block--skills">
      <div class="tool-config__block-head">
        <div class="tool-category-title">
          Skill Registry
        </div>
        <UiButton
          variant="ghost"
          :disabled="skillsLoading || !selectedEntityId"
          @click="emit('loadSkillData', selectedEntityId)"
        >
          {{ skillsLoading ? '加载中...' : '刷新' }}
        </UiButton>
      </div>
      <EmptyState
        v-if="skillRegistry.length === 0 && !skillsLoading"
        icon="data"
        title="暂无注册 Skill"
      />
      <UiTable
        v-if="skillRegistry.length > 0"
        class="tool-config__table"
        label="Skill Registry"
        :sticky-head="false"
      >
        <thead>
          <tr>
            <th data-mono>
              Slug
            </th>
            <th>名称</th>
            <th data-mono>
              版本
            </th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="sk in skillRegistry" :key="sk.skill_slug">
            <td data-mono>
              {{ sk.skill_slug }}
            </td>
            <td>{{ sk.name }}</td>
            <td data-mono>
              {{ sk.version }}
            </td>
            <td>{{ sk.status }}</td>
            <td>
              <div class="tool-config__row-actions">
                <UiButton variant="tonal" @click="emit('saveSkillBinding', sk.skill_slug)">
                  绑定
                </UiButton>
              </div>
            </td>
          </tr>
        </tbody>
      </UiTable>
      <div v-if="skillBindings.length > 0" class="tool-config__subblock">
        <div class="tool-category-title">
          当前实体 Skill Bindings
        </div>
        <UiTable label="当前实体 Skill Bindings" :sticky-head="false">
          <thead>
            <tr>
              <th data-mono>
                Binding ID
              </th>
              <th data-mono>
                Skill
              </th>
              <th data-mono>
                版本
              </th>
              <th>启用</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="sb in skillBindings" :key="sb.binding_id">
              <td data-mono>
                {{ sb.binding_id }}
              </td>
              <td data-mono>
                {{ sb.skill_slug }}
              </td>
              <td data-mono>
                {{ sb.version }}
              </td>
              <td>{{ sb.enabled ? '是' : '否' }}</td>
              <td>
                <div class="tool-config__row-actions">
                  <UiButton
                    variant="danger"
                    @click="emit('deleteSkillBinding', sb.binding_id)"
                  >
                    删除
                  </UiButton>
                </div>
              </td>
            </tr>
          </tbody>
        </UiTable>
      </div>
    </div>
  </fieldset>
</template>

<style scoped>
.form-row :deep(.ui-field) {
  flex: 1;
  min-width: 0;
}

.form-row > .tool-config__switch {
  flex: 1;
  min-width: 0;
}

.tool-config__switch {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  min-width: 0;
}

.tool-config__switch-label {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
}

.tool-config__chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
}

.tool-config__block {
  margin-top: var(--s3);
}

.tool-config__block--skills {
  margin-top: var(--s4);
}

.tool-config__block-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.tool-config__subblock {
  margin-top: var(--s3);
}

.tool-config__table {
  margin-top: var(--s1);
}

.tool-config__row-actions {
  display: flex;
  align-items: center;
  gap: var(--s1);
}

textarea.tool-config__mono {
  font-family: var(--mono);
  font-size: var(--fs-label);
}
</style>
