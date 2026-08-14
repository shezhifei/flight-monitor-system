<script setup lang="ts">
import type { ModelsTabForm } from '../composables/useAiConfigCenter';
import type { McpServerDefinition, McpEntityBinding, SkillRegistryEntry, SkillEntityBinding } from '../aiConfigTypes';

defineProps<{
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
</script>

<template>
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
          <label class="checkbox-label">
            <input v-model="modelsForm.tooling_enabled" type="checkbox">
            <span>启用工具调用</span>
          </label>
          <label class="checkbox-label">
            <input v-model="modelsForm.tooling_allow_parallel" type="checkbox">
            <span>允许并行工具</span>
          </label>
        </div>
        <div class="form-row compact-row">
          <div class="form-group">
            <label for="tooling-max-rounds">最大轮数</label>
            <input
              id="tooling-max-rounds"
              v-model.number="modelsForm.tooling_max_rounds"
              type="number"
              min="0"
              max="20"
              step="1"
              class="form-input"
            >
          </div>
          <div class="form-group">
            <label for="write-action-policy">写动作策略</label>
            <select
              id="write-action-policy"
              v-model="modelsForm.write_action_policy"
              class="form-select"
            >
              <option value="proposal_only">
                proposal_only
              </option>
            </select>
          </div>
        </div>
      </div>
      <div>
        <div class="tool-category-title">
          工具来源
        </div>
        <div class="checkbox-grid">
          <label
            v-for="source in toolSourceOptions"
            :key="source.value"
            class="checkbox-label"
          >
            <input
              type="checkbox"
              :checked="modelsForm.allowed_tool_sources.includes(source.value)"
              @change="emit('toggleToolSource', source.value, ($event.target as HTMLInputElement).checked)"
            >
            <span>{{ source.label }}</span>
          </label>
        </div>
      </div>
    </div>

    <div v-if="Object.keys(categoryToolMap).length > 0" class="tool-category">
      <div class="tool-category-title">
        允许工具类别
      </div>
      <div class="tool-category-grid">
        <label
          v-for="(_, category) in categoryToolMap"
          :key="`category-${category}`"
          class="checkbox-label"
        >
          <input
            type="checkbox"
            :checked="allowedToolCategoriesSet.has(category)"
            @change="emit('toggleAllowedToolCategory', category, ($event.target as HTMLInputElement).checked)"
          >
          <span>{{ category }}</span>
        </label>
      </div>
    </div>

    <p class="models-section-hint">
      取消勾选具体工具即列入 denied_tools，禁止该实体调用对应工具。已禁用：{{ modelsForm.denied_tools.length }}
    </p>
    <div v-if="Object.keys(categoryToolMap).length === 0" class="empty-state-inline">
      暂无工具分类
    </div>
    <div
      v-for="(toolNames, category) in categoryToolMap"
      :key="category"
      class="tool-category"
    >
      <div class="tool-category-title">
        {{ category }}
      </div>
      <div class="tool-category-grid">
        <label
          v-for="toolName in toolNames"
          :key="toolName"
          class="checkbox-label"
        >
          <input
            type="checkbox"
            :checked="!deniedToolsSet.has(toolName)"
            @change="emit('toggleDeniedTool', toolName, ($event.target as HTMLInputElement).checked)"
          >
          <span>{{ toolName }}</span>
        </label>
      </div>
    </div>
  </fieldset>

  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>Subagents 编排</legend>
    <p class="models-section-hint">
      Subagents 通过"可委派 AI 实体"实现。被委派实体各自使用自己的模型、Prompt、MCP、Agent Skill 和工具能力。
    </p>
    <div class="form-row">
      <label class="checkbox-label form-group">
        <input v-model="modelsForm.subagents_enabled" type="checkbox">
        <span>启用 Subagents</span>
      </label>
      <label class="checkbox-label form-group">
        <input v-model="modelsForm.subagents_inherit_parent_context" type="checkbox">
        <span>继承父上下文摘要</span>
      </label>
      <label class="checkbox-label form-group">
        <input v-model="modelsForm.subagents_require_tool_calling_capability" type="checkbox">
        <span>要求模型支持工具调用</span>
      </label>
    </div>
    <div class="form-group">
      <label for="subagents-entities">可委派实体 ID</label>
      <textarea
        id="subagents-entities"
        v-model="modelsForm.subagents_allowed_entity_ids"
        class="form-textarea form-textarea-mono"
        rows="3"
        placeholder="flight_dispatcher, ops_researcher"
      />
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="subagents-max-depth">最大委派深度</label>
        <input
          id="subagents-max-depth"
          v-model.number="modelsForm.subagents_max_depth"
          type="number"
          min="1"
          max="5"
          step="1"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="subagents-max-concurrency">最大并发</label>
        <input
          id="subagents-max-concurrency"
          v-model.number="modelsForm.subagents_max_concurrency"
          type="number"
          min="1"
          max="16"
          step="1"
          class="form-input"
        >
      </div>
    </div>
    <div class="form-group">
      <label for="subagents-handoff-prompt">委派 Prompt</label>
      <textarea
        id="subagents-handoff-prompt"
        v-model="modelsForm.subagents_handoff_prompt"
        class="form-textarea form-textarea-mono"
        rows="4"
        placeholder="描述何时委派、如何汇总结果、哪些动作仍需父实体审批"
      />
    </div>
  </fieldset>

  <fieldset class="models-section" :disabled="modelsLoading">
    <legend>MCP 与 Agent Skill</legend>
    <div class="form-row">
      <label class="checkbox-label form-group">
        <input v-model="modelsForm.mcp_enabled" type="checkbox">
        <span>启用 MCP 绑定</span>
      </label>
      <label class="checkbox-label form-group">
        <input v-model="modelsForm.mcp_fail_closed" type="checkbox">
        <span>MCP 失败时关闭工具</span>
      </label>
      <label class="checkbox-label form-group">
        <input v-model="modelsForm.skills_enabled" type="checkbox">
        <span>启用 Agent Skill</span>
      </label>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="mcp-binding-ids">MCP Binding IDs (config)</label>
        <textarea
          id="mcp-binding-ids"
          v-model="modelsForm.mcp_binding_ids"
          class="form-textarea form-textarea-mono"
          rows="2"
          placeholder="default:ops-docs, default:weather"
        />
      </div>
      <div class="form-group">
        <label for="skills-allowlist">Agent Skill Allowlist (config)</label>
        <textarea
          id="skills-allowlist"
          v-model="modelsForm.skills_allowlist"
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
          v-model="modelsForm.mcp_tool_name_prefix"
          type="text"
          class="form-input"
        >
      </div>
      <div class="form-group">
        <label for="mcp-discovery-ttl">Discovery TTL (秒)</label>
        <input
          id="mcp-discovery-ttl"
          v-model.number="modelsForm.mcp_discovery_cache_ttl_seconds"
          type="number"
          min="0"
          step="1"
          class="form-input"
        >
      </div>
    </div>

    <div style="margin-top:12px;">
      <div style="display:flex;justify-content:space-between;align-items:center;">
        <div class="tool-category-title">
          MCP Servers
        </div>
        <button
          type="button"
          class="btn btn-secondary btn-sm"
          :disabled="mcpLoading || !selectedEntityId"
          @click="emit('loadMcpData', selectedEntityId)"
        >
          {{ mcpLoading ? '加载中...' : '刷新' }}
        </button>
      </div>
      <div v-if="mcpServers.length === 0 && !mcpLoading" style="color:var(--text-secondary,#888);font-size:13px;padding:4px 0;">
        暂无 MCP Server
      </div>
      <table v-if="mcpServers.length > 0" style="width:100%;font-size:13px;border-collapse:collapse;margin-top:4px;">
        <thead>
          <tr style="text-align:left;border-bottom:1px solid var(--border-color,#ddd);">
            <th style="padding:4px 8px;">ID</th>
            <th style="padding:4px 8px;">名称</th>
            <th style="padding:4px 8px;">传输</th>
            <th style="padding:4px 8px;">命令</th>
            <th style="padding:4px 8px;">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="srv in mcpServers" :key="srv.id" style="border-bottom:1px solid var(--border-color,#eee);">
            <td style="padding:4px 8px;"><code>{{ srv.id }}</code></td>
            <td style="padding:4px 8px;">{{ srv.display_name }}</td>
            <td style="padding:4px 8px;">{{ srv.transport }}</td>
            <td style="padding:4px 8px;"><code>{{ srv.command_ref || srv.endpoint_url || '-' }}</code></td>
            <td style="padding:4px 8px;">
              <button type="button" class="btn btn-secondary btn-sm" @click="emit('saveMcpBinding', srv.id)">绑定</button>
              <button type="button" class="btn btn-outline btn-sm" style="margin-left:4px;" @click="emit('probeMcpServer', srv.id)">探测</button>
            </td>
          </tr>
        </tbody>
      </table>
      <div v-if="mcpBindings.length > 0" style="margin-top:8px;">
        <div class="tool-category-title">当前实体 MCP Bindings</div>
        <table style="width:100%;font-size:13px;border-collapse:collapse;">
          <thead>
            <tr style="text-align:left;border-bottom:1px solid var(--border-color,#ddd);">
              <th style="padding:4px 8px;">Binding ID</th>
              <th style="padding:4px 8px;">Server</th>
              <th style="padding:4px 8px;">启用</th>
              <th style="padding:4px 8px;">Allowed Tools</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="b in mcpBindings" :key="b.binding_id" style="border-bottom:1px solid var(--border-color,#eee);">
              <td style="padding:4px 8px;"><code>{{ b.binding_id }}</code></td>
              <td style="padding:4px 8px;"><code>{{ b.server_id }}</code></td>
              <td style="padding:4px 8px;">{{ b.enabled ? '是' : '否' }}</td>
              <td style="padding:4px 8px;">{{ b.allowed_tools?.join(', ') || '-' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div style="margin-top:16px;">
      <div style="display:flex;justify-content:space-between;align-items:center;">
        <div class="tool-category-title">Skill Registry</div>
        <button
          type="button"
          class="btn btn-secondary btn-sm"
          :disabled="skillsLoading || !selectedEntityId"
          @click="emit('loadSkillData', selectedEntityId)"
        >
          {{ skillsLoading ? '加载中...' : '刷新' }}
        </button>
      </div>
      <div v-if="skillRegistry.length === 0 && !skillsLoading" style="color:var(--text-secondary,#888);font-size:13px;padding:4px 0;">
        暂无注册 Skill
      </div>
      <table v-if="skillRegistry.length > 0" style="width:100%;font-size:13px;border-collapse:collapse;margin-top:4px;">
        <thead>
          <tr style="text-align:left;border-bottom:1px solid var(--border-color,#ddd);">
            <th style="padding:4px 8px;">Slug</th>
            <th style="padding:4px 8px;">名称</th>
            <th style="padding:4px 8px;">版本</th>
            <th style="padding:4px 8px;">状态</th>
            <th style="padding:4px 8px;">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="sk in skillRegistry" :key="sk.skill_slug" style="border-bottom:1px solid var(--border-color,#eee);">
            <td style="padding:4px 8px;"><code>{{ sk.skill_slug }}</code></td>
            <td style="padding:4px 8px;">{{ sk.name }}</td>
            <td style="padding:4px 8px;">{{ sk.version }}</td>
            <td style="padding:4px 8px;">{{ sk.status }}</td>
            <td style="padding:4px 8px;">
              <button type="button" class="btn btn-secondary btn-sm" @click="emit('saveSkillBinding', sk.skill_slug)">绑定</button>
            </td>
          </tr>
        </tbody>
      </table>
      <div v-if="skillBindings.length > 0" style="margin-top:8px;">
        <div class="tool-category-title">当前实体 Skill Bindings</div>
        <table style="width:100%;font-size:13px;border-collapse:collapse;">
          <thead>
            <tr style="text-align:left;border-bottom:1px solid var(--border-color,#ddd);">
              <th style="padding:4px 8px;">Binding ID</th>
              <th style="padding:4px 8px;">Skill</th>
              <th style="padding:4px 8px;">版本</th>
              <th style="padding:4px 8px;">启用</th>
              <th style="padding:4px 8px;">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="sb in skillBindings" :key="sb.binding_id" style="border-bottom:1px solid var(--border-color,#eee);">
              <td style="padding:4px 8px;"><code>{{ sb.binding_id }}</code></td>
              <td style="padding:4px 8px;"><code>{{ sb.skill_slug }}</code></td>
              <td style="padding:4px 8px;">{{ sb.version }}</td>
              <td style="padding:4px 8px;">{{ sb.enabled ? '是' : '否' }}</td>
              <td style="padding:4px 8px;">
                <button
                  type="button"
                  class="btn btn-secondary btn-sm"
                  style="color:var(--color-error,#d32f2f);"
                  @click="emit('deleteSkillBinding', sb.binding_id)"
                >删除</button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </fieldset>
</template>
