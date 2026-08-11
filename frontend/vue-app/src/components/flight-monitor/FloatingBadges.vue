<script setup lang="ts">
defineProps<{
  anomalyCount: number;
  anomalySeverity: 'high' | 'medium' | 'low';
  updateMessages: readonly string[];
  updatePanelOpen: boolean;
  notificationCount: number;
}>();

const emit = defineEmits<{
  (e: 'toggle-anomaly-pool'): void;
  (e: 'toggle-update-panel'): void;
  (e: 'close-update-panel'): void;
  (e: 'open-dispatch'): void;
  (e: 'open-chat'): void;
  (e: 'open-insight'): void;
}>();
</script>

<template>
  <div class="floating-badge-stack" :class="{ 'is-update-panel-open': updatePanelOpen }" aria-label="浮动状态徽章">
    <button
      id="openAnomalyBadgeBtn"
      type="button"
      :hidden="anomalyCount <= 0"
      aria-label="打开异常告警池"
      :data-severity="anomalySeverity"
      @click="emit('toggle-anomaly-pool')"
    >
      <span class="anomaly-fab-text">异常告警</span>
      <span id="openAnomalyBadgeCount">{{ anomalyCount }}</span>
    </button>

    <button
      v-if="!updatePanelOpen"
      id="updateBadge"
      type="button"
      aria-label="查看待处理更新"
      :aria-expanded="updatePanelOpen"
      aria-controls="updatePanel"
      class="mb-badge"
      @click="emit('toggle-update-panel')"
    >
      <span class="anomaly-fab-text">待处理更新</span>
      <span id="updateCount" class="mb-badge-count">{{ updateMessages.length }}</span>
    </button>
    
    <button
      id="dispatchChatBadgeBtn"
      type="button"
      aria-label="打开协同群聊"
      class="mb-badge"
      @click="emit('open-chat')"
    >
      <span class="anomaly-fab-text">协同群聊</span>
      <span class="mb-badge-count">💬</span>
    </button>
    
    <button
      id="dispatchNotifyBadgeBtn"
      type="button"
      aria-label="打开调度通报"
      class="mb-badge"
      @click="emit('open-dispatch')"
    >
      <span class="anomaly-fab-text">调度网关</span>
      <span class="mb-badge-count" :style="notificationCount > 0 ? 'background-color: var(--system-red); color: white;' : ''">{{ notificationCount > 0 ? notificationCount : '0' }}</span>
    </button>
    
    <button
      id="flightInsightBadgeBtn"
      type="button"
      aria-label="打开全景洞察"
      class="mb-badge"
      @click="emit('open-insight')"
    >
      <span class="anomaly-fab-text">AI 洞察</span>
      <span class="mb-badge-count">+</span>
    </button>
  </div>

  <section
    v-if="updatePanelOpen"
    id="updatePanel"
    role="dialog"
    aria-modal="false"
    aria-labelledby="updatePanelTitle"
  >
    <div class="update-panel-header">
      <div id="updatePanelTitle" class="update-panel-title">
        更新面板
      </div>
      <button
        type="button"
        class="update-panel-close"
        aria-label="关闭更新面板"
        @click="emit('close-update-panel')"
      >
        &times;
      </button>
    </div>
    <div class="update-panel-content">
      <template v-if="updateMessages.length">
        <p
          v-for="message in updateMessages"
          :key="message"
          class="update-panel-empty"
          style="text-align: left; margin-bottom: 12px;"
        >
          {{ message }}
        </p>
      </template>
      <p v-else class="update-panel-empty">
        暂无待处理更新。
      </p>
    </div>
  </section>
</template>

<style scoped>
.floating-badge-stack {
  position: fixed;
  right: 24px;
  bottom: 24px;
  display: flex;
  flex-direction: column-reverse;
  gap: 12px;
  z-index: var(--z-index-modal);
}

.floating-badge-stack button {
  background: var(--gradient-professional);
  border: 1px solid rgba(255, 255, 255, 0.18);
  backdrop-filter: var(--glass-blur);
  color: var(--text-inverse);
  border-radius: 999px;
  padding: 0 16px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  cursor: pointer;
  box-shadow: 0 8px 20px rgba(52, 58, 69, 0.2);
  transition: transform 0.2s ease, box-shadow 0.2s ease;
  font-family: inherit;
  font-weight: 700;
  font-size: 12px;
  min-width: 148px;
  letter-spacing: 0.02em;
}

.floating-badge-stack button:hover {
  transform: translateY(-2px) scale(1.02);
  box-shadow: 0 12px 28px rgba(52, 58, 69, 0.3);
  filter: brightness(1.05);
}

.floating-badge-stack.is-update-panel-open #openAnomalyBadgeBtn,
.floating-badge-stack.is-update-panel-open #dispatchNotifyBadgeBtn,
.floating-badge-stack.is-update-panel-open #flightInsightBadgeBtn {
  display: none;
}

#openAnomalyBadgeBtn {
  background: var(--gradient-anomaly);
  box-shadow: 0 10px 24px rgba(127, 29, 29, 0.24);
}

#openAnomalyBadgeBtn[data-severity="high"] {
  background: var(--gradient-anomaly-high);
}

#openAnomalyBadgeCount, #updateCount, .mb-badge-count {
  font-size: 18px;
  font-weight: 700;
  line-height: 1;
  color: var(--text-inverse);
  background: transparent;
  padding: 0;
  margin-left: 2px;
}

#updatePanel {
  position: fixed;
  bottom: 92px;
  right: 24px;
  width: min(380px, calc(100vw - 48px));
  background: var(--glass-bg);
  backdrop-filter: var(--glass-blur);
  -webkit-backdrop-filter: var(--glass-blur);
  border-radius: 14px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12), 0 0 0 1px rgba(0, 0, 0, 0.05);
  display: flex;
  flex-direction: column;
  z-index: var(--z-index-loader);
  overflow: hidden;
  border: 1px solid var(--glass-border);
}

#updatePanel[hidden] {
  display: none !important;
}

.update-panel-header {
  padding: 14px 16px;
  background: rgba(0, 0, 0, 0.03);
  border-bottom: 1px solid var(--border-light);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.update-panel-title {
  font-weight: 600;
  font-size: 15px;
  color: var(--text-primary);
}

.update-panel-close {
  background: none;
  border: none;
  color: var(--text-secondary);
  font-size: 22px;
  line-height: 1;
  padding: 0;
  cursor: pointer;
}

.update-panel-content {
  padding: 12px 16px;
  font-size: 14px;
  max-height: 340px;
  overflow-y: auto;
}
</style>
