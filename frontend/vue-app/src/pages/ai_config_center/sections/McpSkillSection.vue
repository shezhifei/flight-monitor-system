<script setup lang="ts">
import { computed } from 'vue';
import type { ModelsTabForm } from '../composables/useAiConfigCenter';
import type {
  McpServerDefinition,
  McpEntityBinding,
  SkillRegistryEntry,
  SkillEntityBinding,
} from '../aiConfigTypes';
import UiButton from '@/components/ui/UiButton.vue';
import UiSwitch from '@/components/ui/UiSwitch.vue';
import UiTable from '@/components/ui/UiTable.vue';

const props = defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
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
  saveMcpBinding: [serverId: string];
  probeMcpServer: [serverId: string];
  loadMcpData: [entityId: string];
  saveSkillBinding: [slug: string];
  deleteSkillBinding: [bindingId: string];
  loadSkillData: [entityId: string];
}>();
</script>

<template>
  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>MCP 与 Agent Skill</legend>
    <div class="form-row">
      <div class="mcp-skill-switch form-group">
        <span class="mcp-skill-switch-label">启用 MCP 绑定</span>
        <UiSwitch
          v-model:checked="form.mcp_enabled"
          label="启用 MCP 绑定"
        />
      </div>
      <div class="mcp-skill-switch form-group">
        <span class="mcp-skill-switch-label">MCP 失败时关闭工具</span>
        <UiSwitch
          v-model:checked="form.mcp_fail_closed"
          label="MCP 失败时关闭工具"
        />
      </div>
      <div class="mcp-skill-switch form-group">
        <span class="mcp-skill-switch-label">启用 Agent Skill</span>
        <UiSwitch
          v-model:checked="form.skills_enabled"
          label="启用 Agent Skill"
        />
      </div>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="mcp-binding-ids">MCP Binding IDs (config)</label>
        <textarea
          id="mcp-binding-ids"
          v-model="form.mcp_binding_ids"
          class="form-textarea form-textarea-mono"
          rows="2"
          placeholder="default:ops-docs, default:weather"
        />
      </div>
      <div class="form-group">
        <label for="skills-allowlist">Agent Skill Allowlist (config)</label>
        <textarea
          id="skills-allowlist"
          v-model="form.skills_allowlist"
          class="form-textarea form-textarea-mono"
          rows="2"
          placeholder="Code, architecture-designer"
        />
      </div>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="mcp-tool-prefix">MCP Tool Prefix</label>
        <input
          id="mcp-tool-prefix"
          v-model="form.mcp_tool_name_prefix"
          type="text"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="mcp-discovery-ttl">Discovery TTL (秒)</label>
        <input
          id="mcp-discovery-ttl"
          v-model.number="form.mcp_discovery_cache_ttl_seconds"
          type="number"
          min="0"
          step="1"
          class="form-input"
        >
      </div>
    </div>

    <div class="mcp-skill-block">
      <div class="mcp-skill-block-head">
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
      <p
        v-if="mcpServers.length === 0 && !mcpLoading"
        class="mcp-skill-empty"
      >
        暂无 MCP Server
      </p>
      <UiTable
        v-if="mcpServers.length > 0"
        class="mcp-skill-table"
        label="MCP Servers"
        :sticky-head="false"
      >
        <thead>
          <tr>
            <th>ID</th>
            <th>名称</th>
            <th>传输</th>
            <th>命令</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="srv in mcpServers" :key="srv.id">
            <td><code>{{ srv.id }}</code></td>
            <td>{{ srv.display_name }}</td>
            <td>{{ srv.transport }}</td>
            <td><code>{{ srv.command_ref || srv.endpoint_url || '-' }}</code></td>
            <td>
              <div class="mcp-skill-row-actions">
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
      <div v-if="mcpBindings.length > 0" class="mcp-skill-subblock">
        <div class="tool-category-title">
          当前实体 MCP Bindings
        </div>
        <UiTable label="当前实体 MCP Bindings" :sticky-head="false">
          <thead>
            <tr>
              <th>Binding ID</th>
              <th>Server</th>
              <th>启用</th>
              <th>Allowed Tools</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="b in mcpBindings" :key="b.binding_id">
              <td><code>{{ b.binding_id }}</code></td>
              <td><code>{{ b.server_id }}</code></td>
              <td>{{ b.enabled ? '是' : '否' }}</td>
              <td>{{ b.allowed_tools?.join(', ') || '-' }}</td>
            </tr>
          </tbody>
        </UiTable>
      </div>
    </div>

    <div class="mcp-skill-block mcp-skill-block--skills">
      <div class="mcp-skill-block-head">
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
      <p
        v-if="skillRegistry.length === 0 && !skillsLoading"
        class="mcp-skill-empty"
      >
        暂无注册 Skill
      </p>
      <UiTable
        v-if="skillRegistry.length > 0"
        class="mcp-skill-table"
        label="Skill Registry"
        :sticky-head="false"
      >
        <thead>
          <tr>
            <th>Slug</th>
            <th>名称</th>
            <th>版本</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="sk in skillRegistry" :key="sk.skill_slug">
            <td><code>{{ sk.skill_slug }}</code></td>
            <td>{{ sk.name }}</td>
            <td>{{ sk.version }}</td>
            <td>{{ sk.status }}</td>
            <td>
              <div class="mcp-skill-row-actions">
                <UiButton variant="tonal" @click="emit('saveSkillBinding', sk.skill_slug)">
                  绑定
                </UiButton>
              </div>
            </td>
          </tr>
        </tbody>
      </UiTable>
      <div v-if="skillBindings.length > 0" class="mcp-skill-subblock">
        <div class="tool-category-title">
          当前实体 Skill Bindings
        </div>
        <UiTable label="当前实体 Skill Bindings" :sticky-head="false">
          <thead>
            <tr>
              <th>Binding ID</th>
              <th>Skill</th>
              <th>版本</th>
              <th>启用</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="sb in skillBindings" :key="sb.binding_id">
              <td><code>{{ sb.binding_id }}</code></td>
              <td><code>{{ sb.skill_slug }}</code></td>
              <td>{{ sb.version }}</td>
              <td>{{ sb.enabled ? '是' : '否' }}</td>
              <td>
                <div class="mcp-skill-row-actions">
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
.mcp-skill-switch {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  min-width: 0;
}

.mcp-skill-switch-label {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
}

.mcp-skill-block {
  margin-top: var(--s3);
}

/* 16px / 8px 不在 --s1..--s5（4 / 6 / 12 / 20 / 28），不硬凑 */
.mcp-skill-block--skills {
  margin-top: 16px;
}

.mcp-skill-block-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.mcp-skill-empty {
  margin: 0;
  padding: var(--s1) 0;
  color: var(--ink-subtle);
  font-size: var(--fs-body);
}

.mcp-skill-table {
  margin-top: var(--s1);
}

.mcp-skill-subblock {
  margin-top: 8px;
}

.mcp-skill-row-actions {
  display: flex;
  align-items: center;
  gap: var(--s1);
}

code {
  font-family: var(--mono);
  font-size: inherit;
  font-variant-numeric: tabular-nums;
}
</style>
