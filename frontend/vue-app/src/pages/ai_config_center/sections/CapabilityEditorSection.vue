<script setup lang="ts">
import type {
  EnrichedCapabilitySnapshot,
  ValidationResult,
  CacheMetricsSummary,
} from '../aiConfigTypesV2';
import type { NormalizedModelOption, ModelsTabForm } from '../composables/useAiConfigCenter';
import CapabilityOverviewSection from './CapabilityOverviewSection.vue';
import ModelBasicConfigSection from './ModelBasicConfigSection.vue';
import ModelRoutingSection from './ModelRoutingSection.vue';
import ModelAdvancedConfigSection from './ModelAdvancedConfigSection.vue';

defineProps<{
  selectedEntityId: string;
  entityDetail: unknown;
  modelsForm: ModelsTabForm;
  modelsLoading: boolean;
  modelsTesting: boolean;
  modelOptions: NormalizedModelOption[];
  modalityOptions: { value: string; label: string }[];
  providerRefOptions: string[];
  existingCapabilityRows: { name: string; current: string }[];
  entityPolicyRows: string[];
  capabilitySnapshot: EnrichedCapabilitySnapshot | null;
  capabilityValidation: ValidationResult | null;
  cacheMetrics: CacheMetricsSummary | null;
  capabilityLoading: boolean;
}>();
const emit = defineEmits<{
  save: [];
  testConnection: [];
  validateCapability: [];
  addProvider: [];
  removeProvider: [index: number];
  toggleInputModality: [value: string, enabled: boolean];
  toggleOutputModality: [value: string, enabled: boolean];
}>();
</script>

<template>
  <form
    class="models-form"
    @submit.prevent="emit('save')"
  >
    <div class="models-form-header">
      <h3 class="models-form-title">
        {{ selectedEntityId }}
      </h3>
      <span class="models-status-dot" :class="{ 'is-ok': !!entityDetail }" aria-hidden="true" />
    </div>

    <CapabilityOverviewSection
      :existing-capability-rows="existingCapabilityRows"
      :entity-policy-rows="entityPolicyRows"
    />

    <ModelBasicConfigSection
      :models-form="modelsForm"
      :models-loading="modelsLoading"
      @add-provider="emit('addProvider')"
      @remove-provider="emit('removeProvider', $event)"
    />

    <ModelRoutingSection
      :models-form="modelsForm"
      :models-loading="modelsLoading"
      :model-options="modelOptions"
      :provider-ref-options="providerRefOptions"
    />

    <ModelAdvancedConfigSection
      :models-form="modelsForm"
      :models-loading="modelsLoading"
      :models-testing="modelsTesting"
      :model-options="modelOptions"
      :modality-options="modalityOptions"
      :selected-entity-id="selectedEntityId"
      :capability-snapshot="capabilitySnapshot"
      :capability-validation="capabilityValidation"
      :cache-metrics="cacheMetrics"
      :capability-loading="capabilityLoading"
      @test-connection="emit('testConnection')"
      @validate-capability="emit('validateCapability')"
      @toggle-input-modality="(value, enabled) => emit('toggleInputModality', value, enabled)"
      @toggle-output-modality="(value, enabled) => emit('toggleOutputModality', value, enabled)"
    />
  </form>
</template>
