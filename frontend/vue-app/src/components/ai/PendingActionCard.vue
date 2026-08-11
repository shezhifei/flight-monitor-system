<script setup lang="ts">
defineProps<{
  actionId: string;
  toolName: string;
  message?: string;
  status?: string;
  loading?: boolean;
}>();

const emit = defineEmits<{
  approve: [actionId: string];
  reject: [actionId: string];
}>();
</script>

<template>
  <div class="pending-action-card" :class="{ 'is-loading': loading }">
    <div class="pa-header">
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      ><circle cx="12" cy="12" r="10" /><path d="M12 8v4l3 3" /></svg>
      <span class="pa-tool-name">{{ toolName }}</span>
      <span class="pa-badge">待审批</span>
    </div>
    <p v-if="message" class="pa-message">
      {{ message }}
    </p>
    <div class="pa-actions">
      <button class="pa-btn pa-btn-approve" :disabled="loading" @click="emit('approve', actionId)">
        批准
      </button>
      <button class="pa-btn pa-btn-reject" :disabled="loading" @click="emit('reject', actionId)">
        拒绝
      </button>
    </div>
  </div>
</template>

<style scoped>
.pending-action-card {
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 8px;
  padding: 10px 12px;
  background: var(--bg-card, #fff);
  margin-bottom: 8px;
}
.pa-header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}
.pa-tool-name { font-weight: 600; font-size: 13px; }
.pa-badge {
  margin-left: auto;
  font-size: 11px;
  color: var(--system-orange, #FF9500);
  background: var(--dh-signal-warn-soft);
  padding: 1px 6px;
  border-radius: 4px;
}
.pa-message { font-size: 12px; color: var(--text-secondary); margin: 0 0 8px; }
.pa-actions { display: flex; gap: 6px; }
.pa-btn {
  flex: 1;
  padding: 6px 12px;
  border-radius: 6px;
  border: 1px solid var(--border-light);
  cursor: pointer;
  font-size: 12px;
  font-weight: 500;
}
.pa-btn-approve { background: var(--system-green, #34C759); color: var(--text-inverse); border-color: transparent; }
.pa-btn-approve:hover { opacity: 0.9; }
.pa-btn-reject { background: var(--system-red, #FF3B30); color: var(--text-inverse); border-color: transparent; }
.pa-btn-reject:hover { opacity: 0.9; }
.pa-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.is-loading { opacity: 0.7; }
</style>
