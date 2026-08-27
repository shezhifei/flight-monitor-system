<script setup lang="ts">
import { useOccupySeat } from '@/composables/useOccupySeat';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiField from '@/components/ui/UiField.vue';

const {
  open,
  loading,
  saving,
  seats,
  positionId,
  personalUsername,
  password,
  openModal,
  closeModal,
  occupy,
} = useOccupySeat();
</script>

<template>
  <UiButton variant="quiet" aria-label="换人占席" @click="openModal()">
    换人
  </UiButton>
  <UiModal :open="open" title="换人占席" :width="420" @close="closeModal">
    <p class="occupy-lead">
      输入要坐这席的个人账号和密码。JWT 永远是个人；席权限每次请求现查占用人。
    </p>
    <UiField label="岗位席位" for-id="occupy-seat">
      <select id="occupy-seat" v-model="positionId" :disabled="loading">
        <option value="" disabled>
          {{ loading ? '加载席位…' : '选择岗位' }}
        </option>
        <option v-for="seat in seats" :key="seat.id" :value="seat.id">
          {{ seat.display_name || seat.username }}
        </option>
      </select>
    </UiField>
    <UiField label="个人用户名" for-id="occupy-username">
      <input
        id="occupy-username"
        v-model="personalUsername"
        type="text"
        autocomplete="username"
      >
    </UiField>
    <UiField label="个人密码" for-id="occupy-password">
      <input
        id="occupy-password"
        v-model="password"
        type="password"
        autocomplete="current-password"
      >
    </UiField>
    <template #footer>
      <UiButton @click="closeModal">
        取消
      </UiButton>
      <UiButton variant="primary" :disabled="saving" @click="occupy">
        {{ saving ? '核验中…' : '占席' }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.occupy-lead {
  margin: 0 0 var(--s3);
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

select,
input {
  width: 100%;
  height: var(--h-sm);
  padding: 0 var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  background: var(--face-page);
  color: var(--ink);
  font: inherit;
}
</style>
