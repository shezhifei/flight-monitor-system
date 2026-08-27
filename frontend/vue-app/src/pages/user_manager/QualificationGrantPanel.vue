<script setup lang="ts">
import { computed } from 'vue';
import type {
  QualificationCatalogOption,
  QualificationGrant,
  QualificationGrantFormState,
  QualificationLevelOption,
} from '@/composables/useUserManager';
import UiButton from '@/components/ui/UiButton.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

const props = defineProps<{
  visible: boolean;
  hint: string;
  grants: QualificationGrant[];
  catalogs: QualificationCatalogOption[];
  levels: QualificationLevelOption[];
  form: QualificationGrantFormState;
  saving?: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:form', value: QualificationGrantFormState): void;
  (e: 'grant'): void;
  (e: 'revoke', grant: QualificationGrant): void;
}>();

const catalogOptions = computed(() => [
  { value: '', label: '选择资质' },
  ...props.catalogs
    .filter((item) => item.is_active !== false)
    .map((item) => ({
      value: item.qualification_code,
      label: `${item.qualification_name}（${item.qualification_code}）`,
    })),
]);

const levelOptions = computed(() => [
  { value: '', label: '选择等级' },
  ...props.levels.map((item) => ({
    value: item.level_code,
    label: `${item.level_name}（${item.level_code}）`,
  })),
]);

function patch<K extends keyof QualificationGrantFormState>(field: K, value: QualificationGrantFormState[K]) {
  const next = { ...props.form, [field]: value };
  if (field === 'qualification_code') next.level_code = '';
  emit('update:form', next);
}

function statusLabel(status: string): string {
  if (status === 'active') return '有效';
  if (status === 'suspended') return '已收回';
  if (status === 'expired') return '过期';
  return status || '—';
}
</script>

<template>
  <div v-if="visible" class="grant-panel">
    <h3 class="grant-title">作业资质</h3>
    <p v-if="hint" class="grant-hint">
      {{ hint }}
    </p>
    <template v-else>
      <table class="grant-table">
        <thead>
          <tr>
            <th>资质</th>
            <th>等级</th>
            <th>状态</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr v-if="grants.length === 0">
            <td colspan="4" class="grant-empty">
              尚未发放资质
            </td>
          </tr>
          <tr v-for="grant in grants" :key="grant.id">
            <td>{{ grant.qualification_code }}</td>
            <td>{{ grant.level_code }}</td>
            <td>{{ statusLabel(grant.status) }}</td>
            <td>
              <UiButton
                v-if="grant.status === 'active'"
                size="sm"
                variant="danger"
                :disabled="saving"
                @click="emit('revoke', grant)"
              >
                收回
              </UiButton>
            </td>
          </tr>
        </tbody>
      </table>
      <div class="grant-form">
        <UiSelect
          :model-value="form.qualification_code"
          :options="catalogOptions"
          label="资质"
          min-width="160px"
          @update:model-value="patch('qualification_code', $event)"
        />
        <UiSelect
          :model-value="form.level_code"
          :options="levelOptions"
          label="等级"
          min-width="140px"
          @update:model-value="patch('level_code', $event)"
        />
        <UiButton
          size="sm"
          variant="primary"
          :disabled="saving || !form.qualification_code || !form.level_code"
          @click="emit('grant')"
        >
          {{ saving ? '发放中...' : '发放' }}
        </UiButton>
      </div>
    </template>
  </div>
</template>

<style scoped>
.grant-panel {
  margin-top: var(--s4);
  padding-top: var(--s3);
  border-top: 1px solid var(--line);
}

.grant-title {
  margin: 0 0 var(--s2);
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
}

.grant-hint,
.grant-empty {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.grant-table {
  width: 100%;
  border-collapse: collapse;
  margin-bottom: var(--s3);
  font-size: var(--fs-label);
}

.grant-table th,
.grant-table td {
  text-align: left;
  padding: var(--s1) var(--s2);
  border-bottom: 1px solid var(--line);
}

.grant-form {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
  align-items: end;
}
</style>
