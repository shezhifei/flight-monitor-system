<script setup lang="ts">
import type { TerminalInfo } from '@/composables/useDispatchBoardData';
import TerminalSelector from '@/components/dispatch-board/TerminalSelector.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';

const props = defineProps<{
  searchQuery: string;
  searchResults: Array<{ id: string; label: string; sub: string }>;
  searchMetaLabel: string;
  activeViewMode: 'flight' | 'team' | 'employee' | 'equipment';
  terminals: TerminalInfo[];
  activeTerminal: string;
  isOpsMenuVisible: boolean;
  guideSettings: { autoRefresh: boolean; refreshInterval: string; showCompleted: boolean; timeScale: string; conflictNotification: boolean; completeNotification: boolean; cornerFade: boolean };
  settingRefreshInterval: string;
  settingSafetyGateFilter: string;
  isBatchToolbarVisible: boolean;
  selectedOrderIds: string[];
  resourceFocusText: string;
  chatUnreadTotal: number;
  terminalSelectorData: Array<{ id: string; name: string; count: number }>;
  currentTerminalId: string;
}>();

const emit = defineEmits<{
  (e: 'update:searchQuery', val: string): void;
  (e: 'search'): void;
  (e: 'searchNext'): void;
  (e: 'toggleAiDrawer'): void;
  (e: 'toggleStatusPanel'): void;
  (e: 'toggleChatDrawer'): void;
  (e: 'resetWindowToNow'): void;
  (e: 'toggleGuideAndLegendPanel'): void;
  (e: 'toggleOpsMenu'): void;
  (e: 'handleViewTabChange', tab: 'flight' | 'team' | 'employee' | 'equipment'): void;
  (e: 'switchTerminal', terminal: string): void;
  (e: 'refreshTimeline'): void;
  (e: 'closeOpsMenu'): void;
  (e: 'handleSettingsApply'): void;
  (e: 'toggleBatchToolbar'): void;
  (e: 'clearResourceFocus'): void;
  (e: 'handleTerminalChange', terminalId: string): void;
  (e: 'update:settingRefreshInterval', val: string): void;
  (e: 'update:settingSafetyGateFilter', val: string): void;
  (e: 'update:guideSettings', val: typeof props.guideSettings): void;
}>();
</script>

<template>
  <div class="legend-bar">
    <aside id="opsDock" class="ops-dock">
      <div class="ops-search-group">
        <div id="timelineSearchWrap" class="timeline-search-wrap">
          <div class="timeline-search">
            <input
              id="timelineSearchInput"
              :value="searchQuery"
              type="text"
              placeholder="搜索航班号/任务名"
              aria-label="搜索航班号或任务名"
              @input="emit('update:searchQuery', ($event.target as HTMLInputElement).value)"
              @keyup.enter="emit('search')"
            >
            <button
              id="timelineSearchBtn"
              class="search-mini-btn"
              aria-label="搜索并定位任务"
              @click="emit('search')"
            >
              定位
            </button>
            <button
              id="timelineSearchNextBtn"
              class="search-mini-btn"
              :disabled="searchResults.length === 0"
              aria-label="定位下一个匹配"
              @click="emit('searchNext')"
            >
              下一个
            </button>
          </div>
          <div
            id="timelineSearchResults"
            class="search-result-panel"
            :class="{ open: Boolean(searchQuery) }"
          >
            <div v-for="result in searchResults" :key="result.id" class="search-result-item">
              <strong>{{ result.label }}</strong>
              <span>{{ result.sub }}</span>
            </div>
            <div v-if="searchResults.length === 0 && searchQuery" class="search-empty">
              无匹配结果
            </div>
          </div>
        </div>
        <span id="timelineSearchMeta" class="search-meta">{{ searchMetaLabel }}</span>
      </div>

      <div id="quickActions" class="quick-actions">
        <button id="openAiFloatingBtn" class="quick-action-btn primary" @click="emit('toggleAiDrawer')">
          智能派工
        </button>
        <button id="openStatusFloatingBtn" class="quick-action-btn" @click="emit('toggleStatusPanel')">
          派工状态
        </button>
        <button
          id="openChatFloatingBtn"
          class="quick-action-btn"
          aria-label="打开群聊抽屉"
          @click="emit('toggleChatDrawer')"
        >
          群聊
          <span v-show="chatUnreadTotal > 0" id="chatUnreadBadge" class="chat-unread-badge">{{ chatUnreadTotal }}</span>
        </button>
        <button id="backToNowFloatingBtn" class="quick-action-btn" @click="emit('resetWindowToNow')">
          当前时间
        </button>
      </div>

      <TerminalSelector
        v-if="terminalSelectorData.length > 0"
        :terminals="terminalSelectorData"
        :current-terminal="currentTerminalId"
        @change="emit('handleTerminalChange', $event)"
      />

      <div class="ops-utility-group">
        <button id="openGuideBtn" class="hint-link-btn" @click="emit('toggleGuideAndLegendPanel')">
          引导/图例
        </button>
        <button
          id="opsMenuToggle"
          class="ops-menu-toggle"
          title="调度设置菜单"
          aria-label="调度设置菜单"
          @click="emit('toggleOpsMenu')"
        >
          <SvgIcon src="/frontend/icons/settings.svg" label="设置" class="ops-menu-icon" />
        </button>
      </div>

      <div
        id="opsMenu"
        class="ops-menu"
        :class="{ open: isOpsMenuVisible }"
        :aria-hidden="!isOpsMenuVisible"
      >
        <div class="ops-controls">
          <div id="viewTabGroup" class="view-tabs">
            <button
              class="chip-btn"
              :class="{ active: activeViewMode === 'flight' }"
              data-view="flight"
              @click="emit('handleViewTabChange', 'flight')"
            >
              航班
            </button>
            <button
              class="chip-btn"
              :class="{ active: activeViewMode === 'team' }"
              data-view="team"
              @click="emit('handleViewTabChange', 'team')"
            >
              班组
            </button>
            <button
              class="chip-btn"
              :class="{ active: activeViewMode === 'employee' }"
              data-view="employee"
              @click="emit('handleViewTabChange', 'employee')"
            >
              员工
            </button>
            <button
              class="chip-btn"
              :class="{ active: activeViewMode === 'equipment' }"
              data-view="equipment"
              @click="emit('handleViewTabChange', 'equipment')"
            >
              设备
            </button>
          </div>
          <div id="terminalGroup" class="terminal-tabs">
            <button
              v-for="term in terminals"
              :key="term.terminal"
              class="terminal-tab-btn"
              :class="{ active: term.active }"
              @click="emit('switchTerminal', term.terminal)"
            >
              {{ term.label }}
            </button>
          </div>
          <button id="backToNowBtn" class="action-btn" @click="emit('resetWindowToNow')">
            回到当前时间
          </button>
          <button id="refreshBtn" class="action-btn" @click="emit('refreshTimeline'); emit('closeOpsMenu')">
            刷新
          </button>
          <div class="ops-divider" />
          <div class="settings-row">
            <label for="settingRefreshInterval">自动刷新间隔</label>
            <select id="settingRefreshInterval" :value="settingRefreshInterval" @change="emit('update:settingRefreshInterval', ($event.target as HTMLSelectElement).value)">
              <option value="5000">
                5 秒
              </option>
              <option value="10000">
                10 秒
              </option>
              <option value="15000">
                15 秒
              </option>
              <option value="30000">
                30 秒
              </option>
              <option value="60000">
                60 秒
              </option>
            </select>
          </div>
          <div class="settings-row">
            <label for="settingCornerFade">角落信息自动淡出</label>
            <input
              id="settingCornerFade"
              :checked="guideSettings.cornerFade"
              type="checkbox"
              @change="emit('update:guideSettings', { ...guideSettings, cornerFade: ($event.target as HTMLInputElement).checked })"
            >
          </div>
          <div class="settings-row">
            <label for="settingSafetyGateFilter">安全门禁筛选</label>
            <select id="settingSafetyGateFilter" :value="settingSafetyGateFilter" @change="emit('update:settingSafetyGateFilter', ($event.target as HTMLSelectElement).value)">
              <option value="all">
                全部任务
              </option>
              <option value="blocked">
                仅清单阻断
              </option>
              <option value="pending">
                仅清单待补齐
              </option>
              <option value="ready">
                仅清单就绪
              </option>
            </select>
          </div>
          <div class="settings-actions">
            <button id="settingsApplyBtn" class="action-btn" @click="emit('handleSettingsApply')">
              应用设置
            </button>
          </div>
        </div>
      </div>
    </aside>
  </div>
  <div class="chart-hint-bar">
    <span id="viewModeHint">单击高亮，双击打开详情；滚轮缩放时间轴，拖拽平移。</span>
    <div id="resourceFocusBar" class="resource-focus-bar">
      <span id="resourceFocusText" class="resource-focus-text">{{ resourceFocusText || '未选择聚焦' }}</span>
      <button
        id="resourceFocusClearBtn"
        class="hint-link-btn resource-focus-clear"
        type="button"
        @click="emit('clearResourceFocus')"
      >
        清除聚焦
      </button>
      <button
        class="hint-link-btn"
        type="button"
        :class="{ 'is-bold': isBatchToolbarVisible, 'is-active': selectedOrderIds.length > 0 }"
        @click="emit('toggleBatchToolbar')"
      >
        批量操作{{ selectedOrderIds.length > 0 ? ` (${selectedOrderIds.length})` : '' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
/* 批量开关钮：开态加粗，有选中时着动蓝 —— 三目内联改修饰类 */
.hint-link-btn.is-bold {
  font-weight: var(--fw-semibold);
}

.hint-link-btn.is-active {
  color: var(--act);
}
</style>
