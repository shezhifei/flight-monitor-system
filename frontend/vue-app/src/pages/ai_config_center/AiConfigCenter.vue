<script setup lang="ts">
import { useAiConfigCenter } from './composables/useAiConfigCenter';
import SidebarSection from './sections/SidebarSection.vue';
import OntologyContentSection from './sections/OntologyContentSection.vue';
import EntityListSection from './sections/EntityListSection.vue';
import CapabilityEditorSection from './sections/CapabilityEditorSection.vue';
import PromptTemplateSection from './sections/PromptTemplateSection.vue';
import ToolPermissionSection from './sections/ToolPermissionSection.vue';
import McpSkillSection from './sections/McpSkillSection.vue';
import ContextCacheSection from './sections/ContextCacheSection.vue';
import MultimodalSecuritySection from './sections/MultimodalSecuritySection.vue';
import AudioTestSection from './sections/AudioTestSection.vue';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import UiButton from '@/components/ui/UiButton.vue';
import EmptyState from '@/components/ui/EmptyState.vue';
import './AiConfigCenter.css';

const {
  activeTab, searchQuery, loading, objects, actions,
  filteredObjects, filteredActions, fetchData,
  sidebarUser, handleLogout,
  capabilitySnapshot, capabilityLoading, capabilityValidation,
  mcpServers, mcpBindings, mcpLoading,
  skillRegistry, skillBindings, skillsLoading,
  cacheMetrics, cacheLoading,
  entities, selectedEntityId, entityDetail, modelOptions,
  modelsForm, modelsLoading, modelsSaving, modelsTesting,
  entitySearch, isModelsDirty,
  modalityOptions, toolSourceOptions, existingCapabilityRows, entityPolicyRows,
  providerRefOptions, addProviderEntry, removeProviderEntry,
  toggleModelInputModality, toggleModelOutputModality,
  toggleToolSource, toggleAllowedToolCategory,
  categoryToolMap, deniedToolsSet, allowedToolCategoriesSet,
  toggleDeniedTool,
  filteredEntities,
  refreshModelsTab, selectEntity,
  loadMcpData, loadSkillData, loadCacheMetrics,
  saveMcpBindingForEntity, probeMcpServerAndRefresh,
  saveSkillBindingForEntity, deleteSkillBindingById,
  runCacheInvalidate, runConnectionTestWithCapabilities,
  runCapabilityValidation,
  saveModelsForm, revertModelsForm,
  audioStatus, audioError, audioLogs, audioAsrText, audioSelectedFile,
  audioConnect, audioDisconnect, audioHandleFile,
  audioSendSelectedChunk, audioSendEnd,
} = useAiConfigCenter();
</script>

<template>
  <div class="admin-container">
    <SidebarSection
      :active-tab="activeTab"
      :objects-count="objects.length"
      :actions-count="actions.length"
      :entities-count="entities.length"
      :sidebar-user="sidebarUser"
      @set-tab="activeTab = $event"
      @logout="handleLogout"
    />

    <main class="main-content">
      <template v-if="activeTab !== 'models'">
        <OntologyContentSection
          :active-tab="activeTab"
          :search-query="searchQuery"
          :loading="loading"
          :filtered-objects="filteredObjects"
          :filtered-actions="filteredActions"
          @update:search-query="searchQuery = $event"
          @refresh="fetchData"
        />
      </template>

      <template v-else>
        <div class="models-tab">
          <div class="models-layout">
            <EntityListSection
              :entities="filteredEntities"
              :selected-entity-id="selectedEntityId"
              :entity-search="entitySearch"
              :models-loading="modelsLoading"
              @update:entity-search="entitySearch = $event"
              @select-entity="selectEntity"
              @refresh="refreshModelsTab"
            />

            <section class="models-main">
              <EmptyState
                v-if="!selectedEntityId"
                icon="data"
                title="请选择一个实体进行配置"
              />

              <CapabilityEditorSection
                v-if="selectedEntityId"
                :selected-entity-id="selectedEntityId"
                :entity-detail="entityDetail"
                :models-form="modelsForm"
                :models-loading="modelsLoading"
                :models-testing="modelsTesting"
                :model-options="modelOptions"
                :modality-options="modalityOptions"
                :provider-ref-options="providerRefOptions"
                :existing-capability-rows="existingCapabilityRows"
                :entity-policy-rows="entityPolicyRows"
                :capability-snapshot="capabilitySnapshot"
                :capability-validation="capabilityValidation"
                :cache-metrics="cacheMetrics"
                :capability-loading="capabilityLoading"
                @save="saveModelsForm"
                @test-connection="runConnectionTestWithCapabilities"
                @validate-capability="runCapabilityValidation"
                @add-provider="addProviderEntry"
                @remove-provider="removeProviderEntry"
                @toggle-input-modality="toggleModelInputModality"
                @toggle-output-modality="toggleModelOutputModality"
              />

              <PromptTemplateSection
                v-if="selectedEntityId"
                :models-form="modelsForm"
                :models-loading="modelsLoading"
              />

              <ToolPermissionSection
                v-if="selectedEntityId"
                :models-form="modelsForm"
                :models-loading="modelsLoading"
                :tool-source-options="toolSourceOptions"
                :category-tool-map="categoryToolMap"
                :denied-tools-set="deniedToolsSet"
                :allowed-tool-categories-set="allowedToolCategoriesSet"
                @toggle-tool-source="toggleToolSource"
                @toggle-allowed-tool-category="toggleAllowedToolCategory"
                @toggle-denied-tool="toggleDeniedTool"
              />

              <McpSkillSection
                v-if="selectedEntityId"
                :models-form="modelsForm"
                :models-loading="modelsLoading"
                :mcp-servers="mcpServers"
                :mcp-bindings="mcpBindings"
                :mcp-loading="mcpLoading"
                :skill-registry="skillRegistry"
                :skill-bindings="skillBindings"
                :skills-loading="skillsLoading"
                :selected-entity-id="selectedEntityId"
                @save-mcp-binding="saveMcpBindingForEntity"
                @probe-mcp-server="probeMcpServerAndRefresh"
                @load-mcp-data="loadMcpData"
                @save-skill-binding="saveSkillBindingForEntity"
                @delete-skill-binding="deleteSkillBindingById"
                @load-skill-data="loadSkillData"
              />

              <ContextCacheSection
                v-if="selectedEntityId"
                :models-form="modelsForm"
                :models-loading="modelsLoading"
                :selected-entity-id="selectedEntityId"
                :cache-metrics="cacheMetrics"
                :cache-loading="cacheLoading"
                @load-cache-metrics="loadCacheMetrics"
                @invalidate-cache="runCacheInvalidate"
              />

              <MultimodalSecuritySection
                v-if="selectedEntityId"
                :models-form="modelsForm"
                :models-loading="modelsLoading"
              />

              <div v-if="selectedEntityId" class="models-form-actions">
                <UiButton
                  variant="primary"
                  size="md"
                  :disabled="modelsSaving || !selectedEntityId || !isModelsDirty"
                  @click="saveModelsForm()"
                >
                  {{ modelsSaving ? '正在保存...' : '保存配置' }}
                </UiButton>
                <UiButton
                  variant="ghost"
                  size="md"
                  :disabled="!entityDetail"
                  @click="revertModelsForm()"
                >
                  还原
                </UiButton>
              </div>

              <AudioTestSection
                v-if="selectedEntityId"
                :audio-status="audioStatus"
                :audio-error="audioError"
                :audio-logs="audioLogs"
                :audio-asr-text="audioAsrText"
                :audio-selected-file="audioSelectedFile"
                @connect="audioConnect"
                @disconnect="audioDisconnect"
                @handle-file="audioHandleFile"
                @send-selected-chunk="audioSendSelectedChunk"
                @send-end="audioSendEnd"
              />
            </section>
          </div>
        </div>
      </template>
    </main>
    <ThemeToggle />
  </div>
</template>
