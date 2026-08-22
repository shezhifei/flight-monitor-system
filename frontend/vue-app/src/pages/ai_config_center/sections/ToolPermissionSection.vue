<script setup lang="ts">
import { computed } from 'vue';
import type { ModelsTabForm } from '../composables/useAiConfigCenter';

const props = defineProps<{
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
  toolSourceOptions: { value: string; label: string }[];
  categoryToolMap: Record<string, string[]>;
  deniedToolsSet: Set<string>;
  allowedToolCategoriesSet: Set<string>;
}>();

const form = computed(() => props.modelsForm);
const emit = defineEmits<{
  toggleToolSource: [value: string, enabled: boolean];
  toggleAllowedToolCategory: [category: string, enabled: boolean];
  toggleDeniedTool: [toolName: string, allowed: boolean];
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
            <input v-model="form.tooling_enabled" type="checkbox">
            <span>启用工具调用</span>
          </label>
          <label class="checkbox-label">
            <input v-model="form.tooling_allow_parallel" type="checkbox">
            <span>允许并行工具</span>
          </label>
        </div>
        <div class="form-row compact-row">
          <div class="form-group">
            <label for="tooling-max-rounds">最大轮数</label>
            <input
              id="tooling-max-rounds"
              v-model.number="form.tooling_max_rounds"
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
              v-model="form.write_action_policy"
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
              :checked="form.allowed_tool_sources.includes(source.value)"
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
      取消勾选具体工具即列入 denied_tools，禁止该实体调用对应工具。已禁用：{{ form.denied_tools.length }}
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
        <input v-model="form.subagents_enabled" type="checkbox">
        <span>启用 Subagents</span>
      </label>
      <label class="checkbox-label form-group">
        <input v-model="form.subagents_inherit_parent_context" type="checkbox">
        <span>继承父上下文摘要</span>
      </label>
      <label class="checkbox-label form-group">
        <input v-model="form.subagents_require_tool_calling_capability" type="checkbox">
        <span>要求模型支持工具调用</span>
      </label>
    </div>
    <div class="form-group">
      <label for="subagents-entities">可委派实体 ID</label>
      <textarea
        id="subagents-entities"
        v-model="form.subagents_allowed_entity_ids"
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
          v-model.number="form.subagents_max_depth"
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
          v-model.number="form.subagents_max_concurrency"
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
        v-model="form.subagents_handoff_prompt"
        class="form-textarea form-textarea-mono"
        rows="4"
        placeholder="描述何时委派、如何汇总结果、哪些动作仍需父实体审批"
      />
    </div>
  </fieldset>
</template>
