<template>
  <div class="guide-and-legend-panel">
    <div class="panel-header">
      <div class="header-tabs">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          class="tab-btn"
          :class="{ active: activeTab === tab.id }"
          @click="activeTab = tab.id"
        >
          <span class="tab-icon">{{ tab.icon }}</span>
          <span class="tab-label">{{ tab.label }}</span>
        </button>
      </div>
      <button class="close-btn" @click="$emit('close')">
        ×
      </button>
    </div>

    <div class="panel-content">
      <!-- 引导 Tab -->
      <div v-if="activeTab === 'guide'" class="tab-content">
        <div class="guide-section">
          <h3>📋 操作指南</h3>
          <div class="guide-item">
            <span class="step">1</span>
            <div class="guide-text">
              <strong>查看任务</strong>
              <p>甘特图中每个方块代表一个任务，横向为时间轴</p>
            </div>
          </div>
          <div class="guide-item">
            <span class="step">2</span>
            <div class="guide-text">
              <strong>选择任务</strong>
              <p>点击任务进行选择，选中的任务会高亮显示</p>
            </div>
          </div>
          <div class="guide-item">
            <span class="step">3</span>
            <div class="guide-text">
              <strong>查看详情</strong>
              <p>双击任务方块打开详情面板，滚轮可缩放时间轴</p>
            </div>
          </div>
          <div class="guide-item">
            <span class="step">4</span>
            <div class="guide-text">
              <strong>批量操作</strong>
              <p>选中多个任务后，可以进行批量操作</p>
            </div>
          </div>
        </div>
      </div>

      <!-- 图例 Tab -->
      <div v-if="activeTab === 'legend'" class="tab-content">
        <div class="legend-section">
          <h3>🎨 任务状态</h3>
          <div class="legend-grid">
            <div class="legend-item">
              <span class="legend-color" style="background: var(--status-progress);" />
              <span class="legend-label">进行中</span>
            </div>
            <div class="legend-item">
              <span class="legend-color" style="background: var(--status-assigned);" />
              <span class="legend-label">已排程</span>
            </div>
            <div class="legend-item">
              <span class="legend-color" style="background: var(--status-pending);" />
              <span class="legend-label">延期</span>
            </div>
            <div class="legend-item">
              <span class="legend-color" style="background: var(--status-completed);" />
              <span class="legend-label">已完成</span>
            </div>
            <div class="legend-item">
              <span class="legend-color" style="background: var(--status-alert);" />
              <span class="legend-label">冲突</span>
            </div>
            <div class="legend-item">
              <span class="legend-color" style="background: var(--status-lock);" />
              <span class="legend-label">已锁定</span>
            </div>
          </div>
        </div>

        <div class="legend-section">
          <h3>🔧 图标说明</h3>
          <div class="legend-grid">
            <div class="legend-item">
              <span class="legend-icon">✈️</span>
              <span class="legend-label">航班</span>
            </div>
            <div class="legend-item">
              <span class="legend-icon">🚗</span>
              <span class="legend-label">地勤车辆</span>
            </div>
            <div class="legend-item">
              <span class="legend-icon">📦</span>
              <span class="legend-label">行李搬运</span>
            </div>
            <div class="legend-item">
              <span class="legend-icon">🔧</span>
              <span class="legend-label">维护</span>
            </div>
            <div class="legend-item">
              <span class="legend-icon">🚌</span>
              <span class="legend-label">摆渡车</span>
            </div>
            <div class="legend-item">
              <span class="legend-icon">⛽</span>
              <span class="legend-label">加油</span>
            </div>
          </div>
        </div>
      </div>

      <!-- 设置 Tab -->
      <div v-if="activeTab === 'settings'" class="tab-content">
        <div class="settings-section">
          <h3>⚙️ 视图设置</h3>
          <div class="setting-item">
            <label class="setting-label">自动刷新</label>
            <label class="switch-toggle">
              <input v-model="settings.autoRefresh" type="checkbox">
              <span class="switch-slider" />
            </label>
          </div>
          <div class="setting-item">
            <label class="setting-label">刷新间隔</label>
            <select v-model="settings.refreshInterval" class="setting-select">
              <option value="30000">
                30 秒
              </option>
              <option value="60000">
                1 分钟
              </option>
              <option value="300000">
                5 分钟
              </option>
            </select>
          </div>
          <div class="setting-item">
            <label class="setting-label">显示已完成任务</label>
            <label class="switch-toggle">
              <input v-model="settings.showCompleted" type="checkbox">
              <span class="switch-slider" />
            </label>
          </div>
          <div class="setting-item">
            <label class="setting-label">时间刻度</label>
            <select v-model="settings.timeScale" class="setting-select">
              <option value="15">
                15 分钟
              </option>
              <option value="30">
                30 分钟
              </option>
              <option value="60">
                1 小时
              </option>
            </select>
          </div>
        </div>

        <div class="settings-section">
          <h3>🎯 通知设置</h3>
          <div class="setting-item">
            <label class="setting-label">任务冲突通知</label>
            <label class="switch-toggle">
              <input v-model="settings.conflictNotification" type="checkbox">
              <span class="switch-slider" />
            </label>
          </div>
          <div class="setting-item">
            <label class="setting-label">任务完成通知</label>
            <label class="switch-toggle">
              <input v-model="settings.completeNotification" type="checkbox">
              <span class="switch-slider" />
            </label>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref, watch } from 'vue';

interface GuideSettings {
  autoRefresh: boolean;
  refreshInterval: string;
  showCompleted: boolean;
  timeScale: string;
  conflictNotification: boolean;
  completeNotification: boolean;
  cornerFade: boolean;
}

const props = defineProps<{
  settings: GuideSettings;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'settings-change', value: GuideSettings): void;
}>();

const tabs = [
  { id: 'guide', label: '引导', icon: '📋' },
  { id: 'legend', label: '图例', icon: '🎨' },
  { id: 'settings', label: '设置', icon: '⚙️' },
];

const activeTab = ref('guide');

const settings = reactive<GuideSettings>({ ...props.settings });

watch(
  () => props.settings,
  (next) => Object.assign(settings, next),
  { deep: true },
);

watch(
  settings,
  (next) => emit('settings-change', { ...next }),
  { deep: true },
);
</script>

<style scoped>
.guide-and-legend-panel {
  position: fixed;
  right: -400px;
  top: 0;
  width: 400px;
  height: 100vh;
  background: var(--bg-card);
  box-shadow: -4px 0 20px rgba(0, 0, 0, 0.1);
  display: flex;
  flex-direction: column;
  transition: right 0.3s ease;
  z-index: 1000;
}

.guide-and-legend-panel.open {
  right: 0;
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-light);
  background: linear-gradient(135deg, var(--ws-primary) 0%, var(--system-blue) 100%);
}

.header-tabs {
  display: flex;
  gap: 4px;
}

.tab-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  background: rgba(255, 255, 255, 0.15);
  border: none;
  border-radius: 6px;
  font-size: 13px;
  color: white;
  cursor: pointer;
  transition: all 0.2s;
}

.tab-btn:hover {
  background: rgba(255, 255, 255, 0.25);
}

.tab-btn.active {
  background: var(--bg-card);
  color: var(--ws-primary);
}

.tab-icon {
  font-size: 14px;
}

.close-btn {
  background: none;
  border: none;
  color: white;
  font-size: 20px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  transition: background 0.2s;
}

.close-btn:hover {
  background: rgba(255, 255, 255, 0.2);
}

.panel-content {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
}

.tab-content {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.guide-section,
.legend-section,
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.guide-section h3,
.legend-section h3,
.settings-section h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--admin-text);
}

.guide-item {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}

.step {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--ws-primary) 0%, var(--system-blue) 100%);
  color: white;
  font-size: 14px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
}

.guide-text strong {
  display: block;
  font-size: 14px;
  color: var(--admin-text);
  margin-bottom: 4px;
}

.guide-text p {
  margin: 0;
  font-size: 13px;
  color: var(--admin-text-subtle);
  line-height: 1.5;
}

.legend-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
}

.legend-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background: var(--bg-sidebar);
  border-radius: 6px;
}

.legend-color {
  width: 24px;
  height: 24px;
  border-radius: 4px;
}

.legend-icon {
  font-size: 20px;
  width: 24px;
  text-align: center;
}

.legend-label {
  font-size: 13px;
  color: var(--admin-text);
}

.setting-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 0;
  border-bottom: 1px solid var(--border-light);
}

.setting-item:last-child {
  border-bottom: none;
}

.setting-label {
  font-size: 13px;
  color: var(--admin-text);
}

.setting-select {
  padding: 6px 12px;
  border: 1px solid var(--border-light);
  border-radius: 6px;
  font-size: 13px;
  color: var(--admin-text);
  background: var(--bg-card);
  cursor: pointer;
}

.switch-toggle {
  position: relative;
  display: inline-block;
  width: 48px;
  height: 26px;
}

.switch-toggle input {
  opacity: 0;
  width: 0;
  height: 0;
}

.switch-slider {
  position: absolute;
  cursor: pointer;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: var(--border-light);
  transition: 0.3s;
  border-radius: 26px;
}

.switch-slider:before {
  position: absolute;
  content: "";
  height: 20px;
  width: 20px;
  left: 3px;
  bottom: 3px;
  background-color: white;
  transition: 0.3s;
  border-radius: 50%;
}

input:checked + .switch-slider {
  background: linear-gradient(135deg, var(--ws-primary) 0%, var(--system-blue) 100%);
}

input:checked + .switch-slider:before {
  transform: translateX(22px);
}
</style>
