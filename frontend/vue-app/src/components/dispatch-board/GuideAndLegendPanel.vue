<template>
  <div class="guide-and-legend-panel">
    <div class="panel-header">
      <div class="header-tabs">
        <UiButton
          v-for="tab in tabs"
          :key="tab.id"
          :pressed="activeTab === tab.id"
          @click="activeTab = tab.id"
        >
          <span class="tab-icon">{{ tab.icon }}</span>
          {{ tab.label }}
        </UiButton>
      </div>
      <button class="close-btn" aria-label="关闭面板" @click="$emit('close')">
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
              <span class="legend-color is-progress" />
              <span class="legend-label">进行中</span>
            </div>
            <div class="legend-item">
              <span class="legend-color is-assigned" />
              <span class="legend-label">已排程</span>
            </div>
            <div class="legend-item">
              <span class="legend-color is-pending" />
              <span class="legend-label">延期</span>
            </div>
            <div class="legend-item">
              <span class="legend-color is-completed" />
              <span class="legend-label">已完成</span>
            </div>
            <div class="legend-item">
              <span class="legend-color is-alert" />
              <span class="legend-label">冲突</span>
            </div>
            <div class="legend-item">
              <span class="legend-color is-lock" />
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
            <UiSwitch v-model:checked="settings.autoRefresh" label="自动刷新" />
          </div>
          <div class="setting-item">
            <label class="setting-label">刷新间隔</label>
            <UiSelect
              v-model="settings.refreshInterval"
              :options="refreshIntervalOptions"
              label="刷新间隔"
            />
          </div>
          <div class="setting-item">
            <label class="setting-label">显示已完成任务</label>
            <UiSwitch v-model:checked="settings.showCompleted" label="显示已完成任务" />
          </div>
          <div class="setting-item">
            <label class="setting-label">时间刻度</label>
            <UiSelect
              v-model="settings.timeScale"
              :options="timeScaleOptions"
              label="时间刻度"
            />
          </div>
        </div>

        <div class="settings-section">
          <h3>🎯 通知设置</h3>
          <div class="setting-item">
            <label class="setting-label">任务冲突通知</label>
            <UiSwitch v-model:checked="settings.conflictNotification" label="任务冲突通知" />
          </div>
          <div class="setting-item">
            <label class="setting-label">任务完成通知</label>
            <UiSwitch v-model:checked="settings.completeNotification" label="任务完成通知" />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref, watch } from 'vue';

import UiButton from '@/components/ui/UiButton.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import UiSwitch from '@/components/ui/UiSwitch.vue';

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

const refreshIntervalOptions = [
  { value: '30000', label: '30 秒' },
  { value: '60000', label: '1 分钟' },
  { value: '300000', label: '5 分钟' },
];

const timeScaleOptions = [
  { value: '15', label: '15 分钟' },
  { value: '30', label: '30 分钟' },
  { value: '60', label: '1 小时' },
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
  background: var(--face-raised);
  box-shadow: var(--shadow-md);
  display: flex;
  flex-direction: column;
  transition: right var(--t-slow) var(--ease);
  z-index: var(--z-float);
}

.guide-and-legend-panel.open {
  right: 0;
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
  background: var(--face-work);
}

.header-tabs {
  display: flex;
  gap: var(--s1);
}

.tab-icon {
  font-size: 14px;
}

.close-btn {
  background: none;
  border: none;
  color: var(--ink-subtle);
  font-size: 20px;
  cursor: pointer;
  padding: var(--s1) var(--s2);
  border-radius: 4px;
  transition: background var(--t-fast) var(--ease);
}

.close-btn:hover {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.close-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.panel-content {
  flex: 1;
  overflow-y: auto;
  padding: var(--s4);
}

.tab-content {
  display: flex;
  flex-direction: column;
  gap: var(--s5);
}

.guide-section,
.legend-section,
.settings-section {
  display: flex;
  flex-direction: column;
  gap: var(--s4);
}

.guide-section h3,
.legend-section h3,
.settings-section h3 {
  margin: 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.guide-item {
  display: flex;
  gap: var(--s3);
  align-items: flex-start;
}

.step {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: var(--act-soft);
  color: var(--act);
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  display: flex;
  align-items: center;
  justify-content: center;
}

.guide-text strong {
  display: block;
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  margin-bottom: var(--s1);
}

.guide-text p {
  margin: 0;
  font-size: var(--fs-body);
  color: var(--ink-subtle);
  line-height: 1.5;
}

.legend-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--s3);
}

.legend-item {
  display: flex;
  align-items: center;
  gap: var(--s3);
  padding: var(--s3);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  border-radius: var(--r-cell);
}

/* 图例色块引用派工板状态语义变量（--status-* 归 dispatch-board 域），色声走修饰类不落内联 */
.legend-color {
  width: 24px;
  height: 24px;
  border-radius: 4px;
}

.legend-color.is-progress { background: var(--status-progress); }
.legend-color.is-assigned { background: var(--status-assigned); }
.legend-color.is-pending { background: var(--status-pending); }
.legend-color.is-completed { background: var(--status-completed); }
.legend-color.is-alert { background: var(--status-alert); }
.legend-color.is-lock { background: var(--status-lock); }

.legend-icon {
  font-size: 20px;
  width: 24px;
  text-align: center;
}

.legend-label {
  font-size: var(--fs-body);
  color: var(--ink);
}

.setting-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--s3) 0;
  border-bottom: 1px solid var(--line);
}

.setting-item:last-child {
  border-bottom: none;
}

.setting-label {
  font-size: var(--fs-body);
  color: var(--ink);
}
</style>
