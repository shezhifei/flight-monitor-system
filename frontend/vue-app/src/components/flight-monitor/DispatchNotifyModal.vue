<script setup lang="ts">
import { ref, watch, onUnmounted, computed } from 'vue';
import { useNotification, DispatchOnlineUserOption, NotificationResponse, SentReceiptGroupSummaryResponse } from '../../composables/useNotification';
import { useAuth } from '../../composables/useAuth';
import { useToast } from '../../composables/useToast';

const props = defineProps<{
  isOpen: boolean;
  /** 将内部 flight_id（ULID）解析为航班号；未提供或解析不到时回落显示原 id */
  flightNoResolver?: (flightId: string) => string | null | undefined;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const activeTab = ref('send');

// notification composable hooks
const notificationData = useNotification();
const auth = useAuth();
const { showToast } = useToast();

const errorState = ref<{ show: boolean; message: string; retryFn: (() => void) | null }>({ show: false, message: '', retryFn: null });

const announce = (msg: string, type: 'info' | 'success' | 'error' | 'warning' = 'info') => {
  showToast(type, msg);
};

/** 当前登录用户标识（id / sub / user_id / username），用于收件人列表排除自己 */
function getCurrentUserIdentity() {
  const user = auth.getUser();
  if (!user) {
    return { ids: new Set<string>(), username: '' };
  }
  const ids = new Set(
    [user.user_id, user.id, user.sub]
      .map((v) => String(v || '').trim())
      .filter(Boolean),
  );
  return {
    ids,
    username: String(user.username || user.name || user.display_name || '').trim().toLowerCase(),
  };
}

function isCurrentUserRecipient(user: DispatchOnlineUserOption): boolean {
  const { ids, username } = getCurrentUserIdentity();
  const userId = String(user.user_id || '').trim();
  if (userId && ids.has(userId)) return true;
  const uname = String(user.username || '').trim().toLowerCase();
  return Boolean(username && uname && username === uname);
}

/** 对齐 legacy renderOriginBadge：workflow → 流程，其余 → 人工 */
function resolveOriginKind(originType?: string | null, originLabel?: string | null): 'workflow' | 'manual' {
  const type = String(originType || '').trim().toLowerCase();
  if (type === 'workflow') return 'workflow';
  const label = String(originLabel || '').trim();
  if (label === '流程' || label.toLowerCase() === 'workflow') return 'workflow';
  return 'manual';
}

function getOriginBadgeLabel(originType?: string | null, originLabel?: string | null): string {
  return resolveOriginKind(originType, originLabel) === 'workflow' ? '流程' : '人工';
}

function getSeverityKind(severity?: string | null): 'info' | 'warning' | 'critical' {
  const s = String(severity || 'info').trim().toLowerCase();
  if (s === 'critical' || s === 'crit') return 'critical';
  if (s === 'warning' || s === 'warn') return 'warning';
  return 'info';
}

function getSeverityBadgeLabel(severity?: string | null): string {
  const kind = getSeverityKind(severity);
  if (kind === 'critical') return 'CRITICAL';
  if (kind === 'warning') return 'WARN';
  return 'INFO';
}

// --- SEND TAB STATE ---
const searchKeyword = ref('');
const onlineUsers = ref<DispatchOnlineUserOption[]>([]);
const selectedUserIds = ref<string[]>([]);
const sendTitle = ref('');
const sendBody = ref('');
const sendSeverity = ref<"info" | "warning" | "critical">('info');
const receiptRequired = ref(true);

let refreshInterval: number;

async function loadOnlineUsers() {
  const result = await notificationData.fetchOnlineUsers(searchKeyword.value);
  if (result.ok) {
    // 下发对象排除自己（不能给自己发调度通知）
    onlineUsers.value = result.items.filter((u) => !isCurrentUserRecipient(u));
    const validIds = new Set(onlineUsers.value.map((u) => u.user_id));
    selectedUserIds.value = selectedUserIds.value.filter((id) => validIds.has(id));
    errorState.value = { show: false, message: '', retryFn: null };
  } else {
    errorState.value = { show: true, message: '在线用户加载失败', retryFn: loadOnlineUsers };
  }
}

function toggleUserSelection(userId: string) {
  const idx = selectedUserIds.value.indexOf(userId);
  if (idx > -1) {
    selectedUserIds.value.splice(idx, 1);
  } else {
    selectedUserIds.value.push(userId);
  }
}

async function handleSend() {
  if (selectedUserIds.value.length === 0) {
    announce('请至少选择一个收件人');
    return;
  }
  if (!sendTitle.value.trim() || !sendBody.value.trim()) {
    announce('请填写标题和内容');
    return;
  }
  
  const success = await notificationData.sendDispatch({
    recipient_user_ids: selectedUserIds.value,
    title: sendTitle.value,
    body: sendBody.value,
    severity: sendSeverity.value,
    receipt_required: receiptRequired.value
  });
  
  if (success) {
    announce('通知下发成功！');
    sendTitle.value = '';
    sendBody.value = '';
    selectedUserIds.value = [];
    switchTab('history');
  } else {
    announce('发送失败，请重试');
  }
}

// --- INBOX TAB STATE ---
const inboxItems = ref<NotificationResponse[]>([]);
const inboxUnreadOnly = ref(false);

async function loadInbox() {
  const result = await notificationData.fetchInbox(inboxUnreadOnly.value, 50, 0);
  if (result.ok) {
    inboxItems.value = result.items;
    errorState.value = { show: false, message: '', retryFn: null };
  } else {
    errorState.value = { show: true, message: '收件箱加载失败', retryFn: loadInbox };
  }
}

async function markAsRead(id: string) {
  const success = await notificationData.markRead(id);
  if (success) {
    loadInbox();
  }
}

const ackNote = ref<{ [key: string]: string }>({});
async function handleAck(id: string, action: "acknowledged" | "rejected") {
  const success = await notificationData.acknowledge(id, action, ackNote.value[id]);
  if (success) {
    announce(action === 'acknowledged' ? '已确认' : '已拒绝');
    loadInbox();
  }
}

function formatDate(ds: string) {
  if (!ds) return '';
  const d = new Date(ds);
  return d.toLocaleString('zh-CN', { hour12: false });
}

// --- HISTORY TAB STATE ---
const historyItems = ref<SentReceiptGroupSummaryResponse[]>([]);
const historyOverdueCount = computed(() => historyItems.value.filter(h => h.is_overdue && h.pending_count > 0).length);

const selectedHistoryId = ref<string | null>(null);
const historyDetailData = ref<import('../../composables/useNotification').SentReceiptGroupDetailResponse | null>(null);
const historyDetailFailed = ref(false);

/** 明细中的航班显示：优先航班号，解析不到时回落原 flight_id */
const historyDetailFlightLabel = computed(() => {
  const flightId = historyDetailData.value?.flight_id;
  if (!flightId) return '';
  return props.flightNoResolver?.(flightId) || flightId;
});

async function loadHistory() {
  const result = await notificationData.fetchHistory(20, 0);
  if (result.ok) {
    historyItems.value = result.items;
    errorState.value = { show: false, message: '', retryFn: null };
    if (!selectedHistoryId.value && result.items.length > 0) {
      selectHistory(result.items[0].receipt_group_id);
    }
  } else {
    errorState.value = { show: true, message: '历史记录加载失败', retryFn: loadHistory };
  }
}

async function selectHistory(id: string) {
  selectedHistoryId.value = id;
  historyDetailData.value = null;
  historyDetailFailed.value = false;
  try {
    const detail = await notificationData.fetchHistoryDetail(id);
    // 防止慢响应覆盖后选中的批次
    if (selectedHistoryId.value !== id) return;
    if (detail) {
      historyDetailData.value = detail;
    } else {
      historyDetailFailed.value = true;
    }
  } catch {
    if (selectedHistoryId.value === id) {
      historyDetailFailed.value = true;
    }
  }
}

// Tab switcher & Lifecycle hooks
function switchTab(tab: string) {
  activeTab.value = tab;
  if (tab === 'send') loadOnlineUsers();
  if (tab === 'inbox') loadInbox();
  if (tab === 'history') loadHistory();
}

watch(() => props.isOpen, (newVal) => {
  if (newVal) {
    switchTab(activeTab.value);
    refreshInterval = window.setInterval(() => {
      if (activeTab.value === 'send') loadOnlineUsers();
      else if (activeTab.value === 'inbox') loadInbox();
      else if (activeTab.value === 'history') loadHistory();
    }, 10000); // 10秒后台刷新
  } else {
    clearInterval(refreshInterval);
  }
});

onUnmounted(() => {
  clearInterval(refreshInterval);
});

function handleOverlayClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-overlay')) {
    // [RESTORE_LOGIC] Disable overlay click to prevent accidental dismissal
  }
}
</script>

<template>
  <teleport to="body">
    <transition name="modal-fade">
      <div
        v-if="isOpen"
        class="modal modal-overlay"
        role="dialog"
        aria-modal="true"
        aria-labelledby="dispatchNotifyModalTitle"
        @click="handleOverlayClick"
      >
        <div class="modal-container">
          <!-- Unified Header -->
          <div class="modal-header">
            <div class="header-left">
              <h3 id="dispatchNotifyModalTitle" class="premium-title">
                调度通知中心
              </h3>
            </div>
            
            <div class="header-center">
              <div class="dispatch-notify-tabs-ios">
                <button class="dispatch-notify-tab-ios" :class="{ active: activeTab === 'send' }" @click="switchTab('send')">
                  新建下发
                </button>
                <button class="dispatch-notify-tab-ios" :class="{ active: activeTab === 'inbox' }" @click="switchTab('inbox')">
                  收到通知
                  <span v-if="notificationData.unreadCount && notificationData.unreadCount.value > 0" class="tab-badge">{{ notificationData.unreadCount.value }}</span>
                </button>
                <button class="dispatch-notify-tab-ios" :class="{ active: activeTab === 'history' }" @click="switchTab('history')">
                  流转历史
                  <span v-if="historyOverdueCount > 0" class="tab-badge badge-warning">{{ historyOverdueCount }}</span>
                </button>
              </div>
            </div>
            
            <div class="header-right">
              <button
                class="close-modal"
                type="button"
                aria-label="关闭弹窗"
                @click="emit('close')"
              >
                <svg
                  width="20"
                  height="20"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                ><path d="M18 6L6 18M6 6l12 12" /></svg>
              </button>
            </div>
          </div>
          
          <div class="modal-body">
            <div v-if="errorState.show" class="error-banner">
              <span>{{ errorState.message }}</span>
              <button class="retry-btn" @click="errorState.retryFn?.()">
                重试
              </button>
            </div>
            <!-- SEND TAB -->
            <div v-if="activeTab === 'send'" class="tab-pane fade-in split-view">
              <!-- Left panel: Users (avoid global .sidebar padding bleed) -->
              <div class="dispatch-side-panel">
                <div class="dispatch-sidebar-header">
                  <div class="search-input-wrapper">
                    <svg
                      class="search-icon"
                      width="14"
                      height="14"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2.5"
                    ><circle cx="11" cy="11" r="8" /><path d="M21 21L16.65 16.65" /></svg>
                    <input
                      v-model="searchKeyword"
                      type="text"
                      placeholder="搜索姓名 / 部门..."
                      class="sidebar-search-input"
                      @input="loadOnlineUsers"
                    >
                  </div>
                </div>
                
                <div class="dispatch-side-list">
                  <div
                    v-for="user in onlineUsers"
                    :key="user.user_id" 
                    class="user-item"
                    :class="{ 'user-selected': selectedUserIds.includes(user.user_id) }"
                    @click="toggleUserSelection(user.user_id)"
                  >
                    <div class="user-avatar" :class="user.status">
                      {{ user.username.charAt(0).toUpperCase() }}
                    </div>
                    <div class="user-info">
                      <div class="user-name">
                        {{ user.username }}
                      </div>
                      <div class="user-meta">
                        {{ user.department || '中心' }} · {{ user.job_title || '调度员' }}
                      </div>
                    </div>
                    <div class="custom-checkbox" :class="{ checked: selectedUserIds.includes(user.user_id) }">
                      <svg
                        v-if="selectedUserIds.includes(user.user_id)"
                        width="10"
                        height="10"
                        viewBox="0 0 12 12"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                      ><path
                        d="M10 3L4.5 8.5L2 6"
                        stroke="white"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      /></svg>
                    </div>
                  </div>
                  <div v-if="onlineUsers.length === 0" class="empty-state">
                    <div>📭 无匹配人员</div>
                  </div>
                </div>
              </div>
              
              <!-- Right Area: Form -->
              <div class="main-content form-area">
                <div class="form-scroll-content">
                  <div class="form-row form-row-split">
                    <div class="form-group flex-1">
                      <label class="premium-label">通知级别 <span class="required">*</span></label>
                      <div class="severity-seg-control">
                        <button
                          type="button"
                          class="seg-btn"
                          :class="{ active: sendSeverity === 'info', 'info-active': sendSeverity === 'info' }"
                          @click="sendSeverity = 'info'"
                        >
                          常规 (Info)
                        </button>
                        <button
                          type="button"
                          class="seg-btn"
                          :class="{ active: sendSeverity === 'warning', 'warn-active': sendSeverity === 'warning' }"
                          @click="sendSeverity = 'warning'"
                        >
                          警告 (Warn)
                        </button>
                        <button
                          type="button"
                          class="seg-btn"
                          :class="{ active: sendSeverity === 'critical', 'crit-active': sendSeverity === 'critical' }"
                          @click="sendSeverity = 'critical'"
                        >
                          紧急 (Crit)
                        </button>
                      </div>
                    </div>
                  </div>
                  
                  <div class="form-row">
                    <div class="form-group receipt-group">
                      <div class="ios-switch-wrapper" @click="receiptRequired = !receiptRequired">
                        <div class="ios-switch-text">
                          <span class="receipt-title">需要接收方强制确认 (回执)</span>
                          <span class="receipt-desc">该指令下发后，对方屏幕中心将弹出强提醒，要求提供已阅签名。</span>
                        </div>
                        <div class="ios-switch" :class="{ active: receiptRequired }">
                          <div class="ios-switch-knob" />
                        </div>
                      </div>
                    </div>
                  </div>

                  <div class="form-row">
                    <div class="form-group">
                      <label class="premium-label">主旨概要 <span class="required">*</span></label>
                      <input
                        v-model="sendTitle"
                        type="text"
                        placeholder="输入精准、简短的主题概要"
                        class="premium-input"
                      >
                    </div>
                  </div>
                  
                  <div class="form-row form-row-body">
                    <div class="form-group">
                      <label class="premium-label">调度指令明细 <span class="required">*</span></label>
                      <textarea v-model="sendBody" placeholder="描述具体的协同动作、处置预案或是通报详情..." class="premium-textarea" rows="5" />
                    </div>
                  </div>
                </div>
                
                <div class="main-footer">
                  <div class="footer-meta">
                    已选择 <span class="highlight-num">{{ selectedUserIds.length }}</span> 位人员
                  </div>
                  <button
                    class="premium-submit-btn"
                    :class="{ disabled: selectedUserIds.length === 0 || !sendTitle || !sendBody }"
                    :disabled="selectedUserIds.length === 0 || !sendTitle || !sendBody"
                    @click="handleSend"
                  >
                    <svg
                      width="18"
                      height="18"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    ><line
                      x1="22"
                      y1="2"
                      x2="11"
                      y2="13"
                    /><polygon points="22 2 15 22 11 13 2 9 22 2" /></svg>
                    下发指令
                  </button>
                </div>
              </div>
            </div>

            <!-- INBOX TAB -->
            <div v-if="activeTab === 'inbox'" class="tab-pane fade-in single-view">
              <div class="content-header">
                <div class="ios-checkbox-wrap" @click="inboxUnreadOnly = !inboxUnreadOnly; loadInbox()">
                  <div class="custom-checkbox small-checkbox" :class="{ checked: inboxUnreadOnly }">
                    <svg
                      v-if="inboxUnreadOnly"
                      width="10"
                      height="10"
                      viewBox="0 0 12 12"
                      fill="none"
                    ><path
                      d="M10 3L4.5 8.5L2 6"
                      stroke="white"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    /></svg>
                  </div>
                  <span class="premium-label-inline">仅看未读消息</span>
                </div>
                <button class="premium-text-btn" @click="notificationData.markAllRead().then(() => loadInbox())">
                  全部标为已读
                </button>
              </div>
              
              <div class="list-container">
                <div v-if="inboxItems.length === 0" class="empty-state">
                  <div class="empty-icon">
                    🏖️
                  </div>
                  <div>您当前没有任何调度通知</div>
                </div>
                <div
                  v-for="item in inboxItems"
                  :key="item.notification_id"
                  class="inbox-card"
                  :class="{ unread: !item.is_read }"
                >
                  <div class="inbox-card-header">
                    <div class="inbox-title-row">
                      <div class="inbox-title" :class="`severity-${item.severity}`">
                        <span v-if="!item.is_read" class="unread-pulse" />
                        {{ item.title }}
                      </div>
                      <span
                        class="origin-badge"
                        :class="resolveOriginKind(item.origin_type, item.origin_label)"
                      >
                        {{ getOriginBadgeLabel(item.origin_type, item.origin_label) }}
                      </span>
                    </div>
                    <div class="inbox-time">
                      {{ formatDate(item.created_at) }}
                    </div>
                  </div>
                  <div class="inbox-body">
                    {{ item.body }}
                  </div>
                  <div class="inbox-meta">
                    <span class="sender-tag">发信人: {{ item.sender_username || '系统' }}</span>
                    <span class="source-tag">
                      类型: {{ getOriginBadgeLabel(item.origin_type, item.origin_label) }}通知
                    </span>
                  </div>
                  
                  <div v-if="!item.is_read || item.receipt_required" class="inbox-actions">
                    <button v-if="!item.is_read" class="premium-text-btn slim" @click="markAsRead(item.notification_id)">
                      标记已读
                    </button>
                    <div v-if="item.receipt_required && item.ack_status === 'pending'" class="ack-form-box">
                      <input
                        v-model="ackNote[item.notification_id]"
                        type="text"
                        class="premium-input inline-input"
                        placeholder="附加回执短讯 (选填)..."
                      >
                      <button class="premium-btn green-btn" @click="handleAck(item.notification_id, 'acknowledged')">
                        确认执行 (ACK)
                      </button>
                      <button class="premium-btn red-btn" @click="handleAck(item.notification_id, 'rejected')">
                        无法执行
                      </button>
                    </div>
                    <div v-else-if="item.receipt_required && item.ack_status !== 'pending'" class="ack-tag" :class="`ack-tag-${item.ack_status}`">
                      <svg
                        v-if="item.ack_status === 'acknowledged'"
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      ><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" /><polyline points="22 4 12 14.01 9 11.01" /></svg>
                      <svg
                        v-else
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      ><circle cx="12" cy="12" r="10" /><line
                        x1="15"
                        y1="9"
                        x2="9"
                        y2="15"
                      /><line
                        x1="9"
                        y1="9"
                        x2="15"
                        y2="15"
                      /></svg>
                      已{{ item.ack_status === 'acknowledged' ? '查收确认' : '报备拒绝' }}
                    </div>
                  </div>
                </div>
              </div>
            </div>

            <!-- HISTORY TAB -->
            <div v-if="activeTab === 'history'" class="tab-pane fade-in split-view">
              <!-- Left panel: History List -->
              <div class="dispatch-side-panel history-sidebar">
                <div class="dispatch-sidebar-header history-sidebar-title">
                  发信批次追踪
                </div>
                <div class="dispatch-side-list history-sidebar-list">
                  <div v-if="historyItems.length === 0" class="empty-state">
                    <div class="empty-icon">
                      🗄️
                    </div>
                    <div>暂无历史下发记录</div>
                  </div>
                  <div
                    v-for="history in historyItems"
                    :key="history.receipt_group_id" 
                    class="history-card" 
                    :class="{ 'history-selected': selectedHistoryId === history.receipt_group_id }"
                    style="cursor: pointer;"
                    @click="selectHistory(history.receipt_group_id)"
                  >
                    <div class="history-card-header">
                      <div class="history-title">
                        <span v-if="history.is_overdue && history.pending_count > 0" class="unread-pulse" />
                        {{ history.title || '无标题调度' }}
                      </div>
                      <div class="history-time">
                        {{ formatDate(history.created_at || '') }}
                      </div>
                    </div>
                    <div class="history-badge-row">
                      <span
                        class="severity-badge"
                        :class="getSeverityKind(history.severity)"
                      >
                        {{ getSeverityBadgeLabel(history.severity) }}
                      </span>
                      <span
                        class="origin-badge"
                        :class="resolveOriginKind(history.origin_type, history.origin_label)"
                      >
                        {{ getOriginBadgeLabel(history.origin_type, history.origin_label) }}
                      </span>
                    </div>
                    <div class="history-stats-modern">
                      <div class="stat-box neutral">
                        <div class="stat-val">
                          {{ history.total_count }}
                        </div>
                        <div class="stat-lbl">
                          群总数
                        </div>
                      </div>
                      <div class="stat-box pending-box">
                        <div class="stat-val">
                          {{ history.pending_count }}
                        </div>
                        <div class="stat-lbl">
                          未回执
                        </div>
                      </div>
                      <div class="stat-box success-box">
                        <div class="stat-val">
                          {{ history.acknowledged_count }}
                        </div>
                        <div class="stat-lbl">
                          已收到
                        </div>
                      </div>
                      <div class="stat-box danger-box">
                        <div class="stat-val">
                          {{ history.rejected_count }}
                        </div>
                        <div class="stat-lbl">
                          已拒绝
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <!-- Right Area: Detail Pane -->
              <div class="main-content history-detail-pane">
                <div v-if="historyDetailData" class="history-detail-inner">
                  <div class="history-detail-header">
                    <div class="history-detail-title-row">
                      <h3 class="history-detail-title">
                        {{ historyDetailData.title || '通知详情' }}
                      </h3>
                      <div class="history-badge-row history-badge-row-inline">
                        <span
                          class="severity-badge"
                          :class="getSeverityKind(historyDetailData.severity)"
                        >
                          {{ getSeverityBadgeLabel(historyDetailData.severity) }}
                        </span>
                        <span
                          class="origin-badge"
                          :class="resolveOriginKind(historyDetailData.origin_type, historyDetailData.origin_label)"
                        >
                          {{ getOriginBadgeLabel(historyDetailData.origin_type, historyDetailData.origin_label) }}
                        </span>
                      </div>
                    </div>
                    <div class="history-detail-meta">
                      <span class="history-detail-meta-item">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>
                        {{ formatDate(historyDetailData.created_at || '') }}
                      </span>
                      <span v-if="historyDetailData.flight_id" class="history-detail-meta-item">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.8 19.2 16 11l3.5-3.5C21 6 21.5 4 21 3c-1-.5-3 0-4.5 1.5L13 8 4.8 6.2c-.5-.1-.9.1-1.1.5l-.3.5c-.2.5-.1 1 .3 1.3L9 12l-2 3H4l-1 1 3 2 2 3 1-1v-3l3-2 3.5 5.3c.3.4.8.5 1.3.3l.5-.2c.4-.3.6-.7.5-1.2z" /></svg>
                        航班 {{ historyDetailFlightLabel }}
                      </span>
                    </div>
                  </div>
                  
                  <div class="history-detail-table-wrap">
                    <div class="history-detail-table-card">
                      <table class="history-receipt-table">
                        <thead>
                          <tr>
                            <th>收信人</th>
                            <th>部门/岗位</th>
                            <th>状态跟踪</th>
                            <th>操作时间</th>
                            <th>备注说明</th>
                          </tr>
                        </thead>
                        <tbody>
                          <tr
                            v-for="item in historyDetailData.items"
                            :key="item.receipt_id || item.notification_id || item.recipient_user_id"
                          >
                            <td class="cell-strong">
                              {{ item.recipient_username }}
                            </td>
                            <td>
                              {{ item.recipient_department || '公司' }} · <span class="cell-muted">{{ item.recipient_job_title || '员工' }}</span>
                            </td>
                            <td>
                              <span
                                class="ack-status-pill"
                                :class="{
                                  ok: item.ack_status === 'acknowledged',
                                  rejected: item.ack_status === 'rejected',
                                  overdue: item.ack_status !== 'acknowledged' && item.ack_status !== 'rejected' && item.is_overdue,
                                  pending: item.ack_status !== 'acknowledged' && item.ack_status !== 'rejected' && !item.is_overdue,
                                }"
                              >
                                {{
                                  item.ack_status === 'acknowledged'
                                    ? '已收到'
                                    : item.ack_status === 'rejected'
                                      ? '已拒绝'
                                      : item.is_overdue
                                        ? '超时未回'
                                        : '等待签收中'
                                }}
                              </span>
                            </td>
                            <td class="cell-mono">
                              {{ formatDate(item.ack_at || '') || '-' }}
                            </td>
                            <td class="cell-note" :title="item.ack_note || ''">
                              {{ item.ack_note || '-' }}
                            </td>
                          </tr>
                        </tbody>
                      </table>
                    </div>
                  </div>
                </div>
                <div v-else-if="historyDetailFailed" class="empty-state">
                  <div class="empty-icon">
                    ⚠️
                  </div>
                  <div>追踪明细加载失败</div>
                  <button
                    type="button"
                    class="flight-text-btn"
                    style="margin-top: 12px;"
                    @click="selectedHistoryId && selectHistory(selectedHistoryId)"
                  >
                    重试
                  </button>
                </div>
                <div v-else class="empty-state">
                  <div class="mb-skeleton spinner" style="width: 32px; height: 32px; border-radius: 50%;" />
                  <div style="margin-top: 16px;">
                    正在加载追踪明细...
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </transition>
  </teleport>
</template>

<style scoped>
/* Animation */
.fade-in { animation: fadeIn 0.3s ease; }
@keyframes fadeIn { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: translateY(0); } }

.modal-overlay {
  position: fixed; top: 0; left: 0; right: 0; bottom: 0;
  background-color: rgba(10, 10, 14, 0.4);
  z-index: 10000; display: flex; align-items: center; justify-content: center;
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
}

.modal-container {
  background-color: var(--admin-card-bg);
  border-radius: 16px;
  width: 900px;
  height: 80vh;
  min-height: 600px;
  max-height: 850px;
  display: flex; flex-direction: column;
  box-shadow: 0 20px 60px rgba(0,0,0,0.12), 0 0 1px rgba(0,0,0,0.1);
  overflow: hidden;
}

/* Header */
.modal-header {
  padding: 12px 24px; display: flex; justify-content: space-between; align-items: center;
  border-bottom: 1px solid var(--admin-border); background: var(--admin-card-bg); z-index: 10;
  flex-shrink: 0;
  /* 覆盖全局 layout.css .modal-header 的 margin-bottom: 1.25rem（那是小表单弹窗的间距） */
  margin-bottom: 0;
}
.header-left { display: flex; flex-direction: column; width: 220px; }
.premium-title { margin: 0 0 4px 0; font-size: 16px; font-weight: 700; color: var(--admin-text); }
.dispatch-notify-header-badge {
  display: inline-flex; align-items: center; font-size: 12px; color: var(--system-green); font-weight: 600;
  background-color: var(--success-bg-subtle); padding: 4px 8px; border-radius: 12px; align-self: flex-start;
}
.dispatch-online-dot {
  width: 6px; height: 6px; background-color: var(--system-green); border-radius: 50%; margin-right: 6px;
  box-shadow: 0 0 0 2px rgba(52, 199, 89, 0.2);
}

.header-center { flex: 1; display: flex; justify-content: center; }
.dispatch-notify-tabs-ios {
  display: inline-flex; background: var(--bg-sidebar); border-radius: 8px; padding: 3px; gap: 2px;
}
.dispatch-notify-tab-ios {
  background: transparent; border: none; font-size: 13px; font-weight: 600; color: var(--admin-text-muted);
  padding: 6px 24px; border-radius: 6px; cursor: pointer; transition: all 0.2s; position: relative;
  display: inline-flex; align-items: center; gap: 6px;
}
.dispatch-notify-tab-ios:hover:not(.active) { color: var(--admin-text); }
.dispatch-notify-tab-ios.active {
  background: var(--admin-card-bg); color: var(--admin-text); box-shadow: 0 1px 4px rgba(0,0,0,0.06), 0 1px 1px rgba(0,0,0,0.04);
}

.tab-badge {
  background: var(--system-red); color: var(--text-inverse); font-size: 10px; font-weight: 800; padding: 2px 6px; 
  border-radius: 10px; line-height: 1; box-shadow: 0 1px 2px rgba(255, 59, 48, 0.4);
}
.tab-badge.badge-warning { background: var(--system-orange); box-shadow: 0 1px 2px rgba(255, 149, 0, 0.4); }

.header-right { width: 220px; display: flex; justify-content: flex-end; }
.close-modal {
  background: transparent; border: none; color: var(--admin-text-muted); border-radius: 50%; width: 32px; height: 32px;
  display: flex; align-items: center; justify-content: center; cursor: pointer; transition: all 0.2s;
}
.close-modal:hover { background: var(--bg-sidebar); color: var(--admin-text); }

/* Body Layouts */
.modal-body {
  flex-grow: 1; display: flex; flex-direction: column; overflow: hidden; background: var(--admin-card-bg); min-height: 0;
}
.tab-pane { flex-grow: 1; width: 100%; display: flex; min-height: 0; }

/* Split View (Send & History) */
.split-view { flex-direction: row; }

/* Side panel — NEVER use global .sidebar (admin-layout adds padding: 20px 0 0) */
.dispatch-side-panel {
  width: 260px;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--admin-border);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 0;
  margin: 0;
  box-shadow: none;
}
.history-sidebar { width: 340px; }
.dispatch-sidebar-header {
  padding: 10px 12px;
  margin: 0;
  gap: 0;
  border-bottom: 1px solid var(--admin-border);
  background: var(--bg-sidebar);
  flex-shrink: 0;
}
.history-sidebar-title {
  font-weight: 600;
  font-size: 14px;
  color: var(--admin-text);
  line-height: 1.3;
}
.search-input-wrapper { position: relative; display: flex; align-items: center; }
.search-icon { position: absolute; left: 12px; color: var(--admin-text-muted); pointer-events: none; }
.sidebar-search-input {
  width: 100%; background: var(--admin-border); border: none; border-radius: 8px; padding: 8px 12px 8px 34px;
  font-size: 13px; color: var(--admin-text); transition: all 0.2s; outline: none;
  box-sizing: border-box;
}
.sidebar-search-input:focus { background: var(--admin-card-bg); box-shadow: 0 0 0 2px var(--focus-ring-blue); }

.dispatch-side-list {
  flex-grow: 1;
  overflow-y: auto;
  padding: 8px;
  min-height: 0;
}
.history-sidebar-list {
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: var(--bg-sidebar);
}
.dispatch-side-list::-webkit-scrollbar { width: 6px; }
.dispatch-side-list::-webkit-scrollbar-thumb { background: var(--admin-text-muted); border-radius: 10px; }

.user-item {
  display: flex; align-items: center; padding: 10px; border-radius: 8px; cursor: pointer; transition: background 0.15s; margin-bottom: 2px;
}
.user-item:hover { background: var(--admin-border); }
.user-item.user-selected { background: var(--system-blue-subtle); }

.user-avatar {
  width: 32px; height: 32px; border-radius: 50%; display: flex; align-items: center; justify-content: center;
  font-weight: 700; color: var(--admin-card-bg); font-size: 14px; margin-right: 12px; flex-shrink: 0;
}
.user-avatar.online { background: linear-gradient(135deg, var(--system-green), #30D158); }
.user-avatar.offline { background: var(--admin-text-muted); }

.user-info { flex-grow: 1; overflow: hidden; }
.user-name { font-size: 14px; font-weight: 600; color: var(--admin-text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.user-meta { font-size: 11px; color: var(--admin-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

.custom-checkbox {
  width: 16px; height: 16px; border-radius: 4px; border: 1.5px solid var(--admin-text-muted); display: flex; align-items: center; justify-content: center;
  transition: all 0.2s; background: var(--admin-card-bg); flex-shrink: 0;
}
.custom-checkbox.checked { background: var(--ws-primary); border-color: var(--ws-primary); }

/* Main Content Area */
.main-content {
  flex-grow: 1; display: flex; flex-direction: column; background: var(--admin-card-bg); min-height: 0;
}
.form-scroll-content {
  flex-grow: 1; overflow-y: auto; padding: 14px 20px 16px; display: flex; flex-direction: column; gap: 12px;
}

.form-row { display: flex; gap: 12px; }
.form-row-split { display: flex; gap: 12px; }
.form-row-body { flex: 0 0 auto; }
.flex-1 { flex: 1; }
.flex-grow { flex-grow: 1; }
.h-full { height: 100%; }
.flex-col { display: flex; flex-direction: column; }

.form-group { display: flex; flex-direction: column; gap: 6px; width: 100%; }
.premium-label { font-size: 13px; font-weight: 600; color: var(--admin-text-subtle); }
.required { color: var(--system-red); }

.premium-input, .premium-textarea {
  background: var(--bg-sidebar); border: 1px solid var(--admin-border); border-radius: 8px; padding: 9px 12px;
  font-size: 14px; color: var(--admin-text); transition: all 0.2s; outline: none; font-family: inherit;
  width: 100%; box-sizing: border-box;
}
.premium-textarea {
  min-height: 110px;
  max-height: 220px;
  resize: vertical;
  line-height: 1.5;
  flex-grow: 0;
}
.premium-input:focus, .premium-textarea:focus { background: var(--admin-card-bg); border-color: var(--ws-primary); box-shadow: 0 0 0 3px var(--focus-ring-blue); }
.premium-input::placeholder, .premium-textarea::placeholder { color: var(--admin-text-muted); }

/* Segmented Control */
.severity-seg-control {
  display: flex; background: var(--bg-sidebar); border-radius: 8px; padding: 3px; gap: 2px;
}
.seg-btn {
  flex: 1; padding: 8px 0; border-radius: 6px; border: none; background: transparent; font-size: 13px; font-weight: 600;
  color: var(--admin-text-muted); cursor: pointer; transition: all 0.25s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.seg-btn.active.info-active { background: var(--admin-card-bg); color: var(--ws-primary); box-shadow: 0 1px 4px var(--focus-ring-blue); }
.seg-btn.active.warn-active { background: var(--admin-card-bg); color: var(--system-orange); box-shadow: 0 1px 4px rgba(255, 149, 0, 0.15); }
.seg-btn.active.crit-active { background: var(--admin-card-bg); color: var(--system-red); box-shadow: 0 1px 4px rgba(255, 59, 48, 0.15); }

/* iOS Toggle */
.receipt-group {
  background: var(--bg-sidebar); padding: 10px 12px; border-radius: 10px; border: 1px solid var(--admin-border);
}
.ios-switch-wrapper {
  display: flex; justify-content: space-between; align-items: center; cursor: pointer;
}
.ios-switch-text { display: flex; flex-direction: column; gap: 2px;}
.receipt-title { font-size: 14px; font-weight: 600; color: var(--admin-text); }
.receipt-desc { font-size: 12px; color: var(--admin-text-muted); }

.ios-switch {
  width: 44px; height: 26px; background: var(--admin-border); border-radius: 30px; position: relative; transition: background 0.3s; flex-shrink: 0;
}
.ios-switch.active { background: var(--system-green); }
.ios-switch-knob {
  width: 22px; height: 22px; background: var(--admin-card-bg); border-radius: 50%; position: absolute; top: 2px; left: 2px;
  box-shadow: 0 2px 4px rgba(0,0,0,0.2); transition: transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
}
.ios-switch.active .ios-switch-knob { transform: translateX(18px); }

/* Footer */
.main-footer {
  padding: 12px 20px; border-top: 1px solid var(--admin-border); background: var(--admin-card-bg);
  display: flex; justify-content: space-between; align-items: center;
}
.footer-meta { font-size: 13px; color: var(--admin-text-muted); }
.highlight-num { font-weight: 600; color: var(--admin-text); font-size: 15px; }

.premium-submit-btn {
  background: var(--ws-primary); color: var(--admin-card-bg); border: none; padding: 0 24px; height: 40px; border-radius: 8px; font-size: 14px; font-weight: 600;
  display: inline-flex; align-items: center; gap: 8px; cursor: pointer; transition: all 0.2s;
}
.premium-submit-btn:hover:not(.disabled) { background: var(--ws-primary); }
.premium-submit-btn.disabled { background: var(--admin-text-muted); cursor: not-allowed; }

/* Single View (Inbox & History) */
.single-view {
  flex-direction: column; background: var(--bg-sidebar); align-items: stretch; height: 100%;
}
.content-header {
  padding: 16px 32px; display: flex; justify-content: space-between; align-items: center;
}
.premium-label-inline { font-size: 13px; font-weight: 500; color: var(--admin-text-subtle); }
.premium-text-btn { background: none; border: none; font-size: 13px; font-weight: 600; color: var(--ws-primary); cursor: pointer; }
.premium-text-btn:hover { opacity: 0.8; }
.ios-checkbox-wrap { display: flex; align-items: center; gap: 8px; cursor: pointer; }
.small-checkbox { width: 14px; height: 14px; border-radius: 4px; }

.list-container {
  padding: 0 32px 32px; overflow-y: auto; flex-grow: 1; display: flex; flex-direction: column; gap: 12px;
}
.full-list { padding-top: 24px; }

.inbox-card, .history-card {
  background: var(--admin-card-bg); border-radius: 12px; padding: 16px; border: 1px solid var(--admin-border);
  box-shadow: 0 1px 3px rgba(0,0,0,0.02); position: relative; overflow: hidden;
}
.inbox-card.unread::before {
  content: ''; position: absolute; left: 0; top: 0; bottom: 0; width: 4px; background: var(--ws-primary);
}

.history-card.history-selected { border-color: var(--ws-primary); background: var(--dh-signal-accent-soft); }

.inbox-card-header, .history-card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 8px;
  gap: 8px;
}
.inbox-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  min-width: 0;
  flex: 1;
}
/* 级别(左) + 来源(右) 同行胶囊 */
.history-badge-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  margin: 0 0 10px;
}
.history-badge-row-inline {
  justify-content: flex-start;
  margin: 0;
  flex-shrink: 0;
}
.origin-badge,
.severity-badge {
  display: inline-flex;
  align-items: center;
  padding: 3px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.03em;
  flex-shrink: 0;
  white-space: nowrap;
  line-height: 1.2;
}
/* 默认（亮色）对比；深色主题再覆盖为更亮字色 */
/* 胶囊：亮色用深字；暗色用实心底 + 浅字，保证对比度 */
.origin-badge.manual {
  background: rgba(217, 119, 6, 0.14);
  color: #b45309;
  border: 1px solid rgba(217, 119, 6, 0.28);
}
.origin-badge.workflow {
  background: #6366f1;
  color: #ffffff;
  border: 1px solid #818cf8;
}
.severity-badge.info {
  background: rgba(37, 99, 235, 0.12);
  color: #1d4ed8;
  border: 1px solid rgba(37, 99, 235, 0.28);
}
.severity-badge.warning {
  background: rgba(217, 119, 6, 0.14);
  color: #b45309;
  border: 1px solid rgba(217, 119, 6, 0.3);
}
.severity-badge.critical {
  background: rgba(220, 38, 38, 0.12);
  color: #b91c1c;
  border: 1px solid rgba(220, 38, 38, 0.32);
}
:global([data-theme="dark"]) .origin-badge.manual {
  background: #d97706;
  color: #fff7ed;
  border-color: #f59e0b;
}
:global([data-theme="dark"]) .origin-badge.workflow {
  /* 实心亮紫 + 近白字，暗底上对比明确 */
  background: #8b5cf6;
  color: #ffffff;
  border-color: #a78bfa;
  box-shadow: 0 0 0 1px rgba(167, 139, 250, 0.35);
}
:global([data-theme="dark"]) .severity-badge.info {
  background: #2563eb;
  color: #eff6ff;
  border-color: #3b82f6;
}
:global([data-theme="dark"]) .severity-badge.warning {
  background: #d97706;
  color: #fff7ed;
  border-color: #f59e0b;
}
:global([data-theme="dark"]) .severity-badge.critical {
  background: #dc2626;
  color: #fef2f2;
  border-color: #f87171;
}
.inbox-title, .history-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--admin-text);
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.inbox-time, .history-time { font-size: 12px; color: var(--admin-text-muted); flex-shrink: 0; }

.severity-warning { color: var(--system-orange); }
.severity-critical { color: var(--system-red); }
.unread-pulse { width: 8px; height: 8px; background: var(--ws-primary); border-radius: 50%; box-shadow: 0 0 0 2px var(--focus-ring-blue); }

/* History detail pane */
.history-detail-pane {
  background: var(--admin-card-bg);
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.history-detail-inner {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.history-detail-header {
  background: var(--bg-card);
  padding: 18px 24px;
  border-bottom: 1px solid var(--admin-border);
  flex-shrink: 0;
}
.history-detail-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
  margin-bottom: 10px;
}
.history-detail-title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
  color: var(--admin-text);
  min-width: 0;
}
.history-detail-meta {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
  font-size: 13px;
  color: var(--admin-text-muted);
  font-weight: 500;
}
.history-detail-meta-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.history-detail-table-wrap {
  padding: 16px 20px;
  overflow-y: auto;
  flex-grow: 1;
  background: var(--bg-sidebar);
  min-height: 0;
}
.history-detail-table-card {
  background: var(--bg-card);
  border-radius: 12px;
  border: 1px solid var(--admin-border);
  overflow-x: auto;
}
.history-receipt-table {
  width: 100%;
  min-width: 640px;
  border-collapse: collapse;
}
.history-receipt-table thead {
  background: var(--bg-sidebar);
  border-bottom: 1px solid var(--admin-border);
}
.history-receipt-table th {
  text-align: center;
  vertical-align: middle;
  padding: 10px 14px;
  font-size: 13px;
  color: var(--admin-text-subtle);
  font-weight: 600;
  white-space: nowrap;
}
.history-receipt-table td {
  text-align: center;
  vertical-align: middle;
  padding: 12px 14px;
  font-size: 13px;
  color: var(--admin-text);
  border-bottom: 1px solid var(--admin-border);
}
.history-receipt-table tbody tr:last-child td {
  border-bottom: none;
}
.history-receipt-table tbody tr:hover {
  background: var(--bg-sidebar);
}
.history-receipt-table .cell-strong {
  font-weight: 600;
  white-space: nowrap;
}
.history-receipt-table .cell-muted {
  color: var(--admin-text-muted);
}
.history-receipt-table .cell-mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  white-space: nowrap;
}
.history-receipt-table .cell-note {
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ack-status-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 4px 10px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}
.ack-status-pill.ok {
  color: var(--system-green);
  background: var(--success-bg-subtle, rgba(52, 199, 89, 0.12));
}
.ack-status-pill.rejected {
  color: var(--system-red);
  background: var(--error-bg-subtle, rgba(255, 59, 48, 0.12));
}
.ack-status-pill.pending {
  color: var(--system-orange);
  background: rgba(255, 149, 0, 0.12);
}
.ack-status-pill.overdue {
  color: var(--system-red);
  background: var(--error-bg-subtle, rgba(255, 59, 48, 0.12));
}

.inbox-body { font-size: 14px; color: var(--admin-text-subtle); line-height: 1.5; margin-bottom: 12px; white-space: pre-wrap; }
.inbox-meta { display: flex; gap: 8px; margin-bottom: 12px; }
.sender-tag, .source-tag { background: var(--bg-sidebar); padding: 2px 8px; border-radius: 4px; font-size: 11px; font-weight: 500; color: var(--admin-text-subtle); }

.inbox-actions { display: flex; align-items: center; border-top: 1px solid var(--bg-sidebar); padding-top: 12px; gap: 12px; }
.ack-form-box { display: flex; gap: 8px; align-items: center; flex: 1; }
.inline-input { padding: 6px 10px; font-size: 13px; flex-grow: 1; }
.premium-btn { border: none; border-radius: 6px; padding: 6px 12px; font-size: 13px; font-weight: 600; cursor: pointer; color:var(--text-inverse);}
.green-btn { background: var(--system-green); }
.red-btn { background: var(--system-red); }

.ack-tag { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; font-weight: 600; padding: 4px 10px; border-radius: 6px; }
.ack-tag-acknowledged { background: var(--success-bg-subtle); color: var(--system-green); }
.ack-tag-rejected { background: var(--error-bg-subtle); color: var(--system-red); }

/* History stats */
.history-stats-modern { display: flex; gap: 12px; margin-top: 0; }
.stat-box { background: var(--bg-sidebar); padding: 10px; border-radius: 8px; flex: 1; display: flex; flex-direction: column; align-items: center; gap: 2px; border: 1px solid var(--admin-border); }
.stat-val { font-size: 18px; font-weight: 700; }
.stat-lbl { font-size: 11px; font-weight: 600; color: var(--admin-text-muted);}
.stat-box.neutral .stat-val { color: var(--admin-text); }
.stat-box.pending-box .stat-val { color: var(--system-orange); }
.stat-box.success-box .stat-val { color: var(--system-green); }
.stat-box.danger-box .stat-val { color: var(--system-red); }

.empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 60px 0; color: var(--admin-text-muted); font-size: 14px; font-weight: 500; height: 100%;}
.empty-icon { font-size: 32px; margin-bottom: 12px; opacity: 0.8; }

.error-banner {
  display: flex; align-items: center; justify-content: space-between; padding: 12px 24px;
  background: var(--error-bg-subtle); border-bottom: 1px solid var(--error-border-subtle); color: var(--system-red);
  font-size: 13px; font-weight: 600;
}
.retry-btn {
  background: var(--system-red); color: var(--text-inverse); border: none; border-radius: 6px; padding: 6px 16px;
  font-size: 12px; font-weight: 600; cursor: pointer; transition: opacity 0.2s;
}
.retry-btn:hover { opacity: 0.8; }
</style>
