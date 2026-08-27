<script setup lang="ts">
import { ref, watch, onUnmounted, computed } from 'vue';
import { useNotification, DispatchOnlineUserOption, NotificationResponse, SentReceiptGroupSummaryResponse } from '../../composables/useNotification';
import { chatTargetFromNotification, type ChatNotificationTarget } from '../../composables/chatTargetFromNotification';
import { useAuth } from '../../composables/useAuth';
import { useToast } from '../../composables/useToast';
import UiModal from '../ui/UiModal.vue';
import UiSegment from '../ui/UiSegment.vue';
import UiSwitch from '../ui/UiSwitch.vue';
import UiField from '../ui/UiField.vue';
import UiButton from '../ui/UiButton.vue';
import UiBanner from '../ui/UiBanner.vue';
import UiPill from '../ui/UiPill.vue';
import UiAvatar from '../ui/UiAvatar.vue';
import UiSearch from '../ui/UiSearch.vue';
import UiTable from '../ui/UiTable.vue';
import UiSkeleton from '../ui/UiSkeleton.vue';
import SkeletonTableRow from '../ui/SkeletonTableRow.vue';


const props = defineProps<{
  isOpen: boolean;
  /** 将内部 flight_id（ULID）解析为航班号；未提供或解析不到时回落显示原 id */
  flightNoResolver?: (flightId: string) => string | null | undefined;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'open-chat', target: ChatNotificationTarget): void;
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
  const occupantId = String(user.occupant_user_id || '').trim();
  if (occupantId && ids.has(occupantId)) return true;
  const uname = String(user.username || user.display_name || '').trim().toLowerCase();
  return Boolean(username && uname && username === uname);
}

function recipientPrimaryLabel(user: DispatchOnlineUserOption): string {
  const label = String(user.label || '').trim();
  if (label) return label;
  if (user.account_type === 'position') {
    return String(user.display_name || user.username || user.user_id).trim();
  }
  const assignments = Array.isArray(user.assignments) ? user.assignments : [];
  if (assignments.length > 0) {
    const first = assignments[0];
    const flight = String(first?.flight_no || '').trim();
    const task = String(first?.task_type_name || first?.task_type || '').trim();
    const slot = String(first?.slot_name || first?.slot_code || '').trim();
    return [flight, task, slot].filter(Boolean).join('-');
  }
  return String(user.display_name || user.username || user.user_id).trim();
}

function recipientSecondaryLabel(user: DispatchOnlineUserOption): string {
  const meta = String(user.meta || '').trim();
  if (meta) return meta;
  if (user.account_type === 'position') {
    const occupant = String(user.occupant_display_name || '').trim();
    const department = String(user.department || '').trim() || '未设置科室';
    return occupant ? `${department} · ${occupant}` : department;
  }
  const department = String(user.department || '').trim();
  const name = String(user.display_name || user.username || '').trim();
  return [name, department].filter(Boolean).join(' · ') || '一线';
}

/** 对齐 legacy renderOriginBadge：workflow → 流程，其余 → 人工 */
function resolveOriginKind(originType?: string | null, originLabel?: string | null): 'workflow' | 'manual' {
  const type = String(originType || '').trim().toLowerCase();
  if (type === 'workflow') return 'workflow';
  const label = String(originLabel || '').trim();
  if (label === '流程' || label.toLowerCase() === 'workflow') return 'workflow';
  return 'manual';
}

/** 来源是分类不是事态：只报字，不给声（声只有四类，都是事态的出口）。 */
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
  if (kind === 'critical') return '紧急';
  if (kind === 'warning') return '警告';
  return '常规';
}

function severityTone(severity?: string | null): 'act' | 'ok' | 'warn' | 'danger' | 'mute' {
  const kind = getSeverityKind(severity);
  if (kind === 'critical') return 'danger';
  if (kind === 'warning') return 'warn';
  return 'mute';
}

/** 回执事态：已收到=安，已拒绝=危，超时未回=危，待签收=警。 */
function receiptTone(ackStatus: string, isOverdue: boolean): 'ok' | 'warn' | 'danger' {
  if (ackStatus === 'acknowledged') return 'ok';
  if (ackStatus === 'rejected') return 'danger';
  return isOverdue ? 'danger' : 'warn';
}

function receiptLabel(ackStatus: string, isOverdue: boolean): string {
  if (ackStatus === 'acknowledged') return '已收到';
  if (ackStatus === 'rejected') return '已拒绝';
  return isOverdue ? '超时未回' : '待签收';
}

function onSegPointer(tab: string, event: MouseEvent): void {
  switchTab(tab);
  if (event.detail) (event.currentTarget as HTMLButtonElement).blur();
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

/** 搜索框里有字就是持守（§2.5）：值一变立刻重捞，清除也走这一条路。 */
function onSearchInput(value: string) {
  searchKeyword.value = value;
  loadOnlineUsers();
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

/** 仅看未读是可叠加的布尔过滤（§2.6）：开关式按钮，手离开还按着。 */
function toggleUnreadOnly() {
  inboxUnreadOnly.value = !inboxUnreadOnly.value;
  loadInbox();
}

async function markAllRead() {
  await notificationData.markAllRead();
  loadInbox();
}

async function markAsRead(id: string) {
  const success = await notificationData.markRead(id);
  if (success) {
    loadInbox();
  }
}

function openMentionChat(item: NotificationResponse): void {
  const target = chatTargetFromNotification(item);
  if (!target) return;
  emit('open-chat', target);
  if (!item.is_read) {
    void markAsRead(item.notification_id);
  }
  emit('close');
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
</script>

<template>
  <UiModal
    :open="isOpen"
    title="调度通知"
    :width="900"
    bleed
    @close="emit('close')"
  >
    <div class="notify">
      <!-- 三个页签换的是地点（下发 / 收信 / 追踪），不是同一张表上的过滤 -->
      <div class="notify__bar">
        <UiSegment label="调度通知页签" inset="work">
          <button type="button" :aria-checked="activeTab === 'send'" @click="onSegPointer('send', $event)">
            新建下发
          </button>
          <button type="button" :aria-checked="activeTab === 'inbox'" @click="onSegPointer('inbox', $event)">
            收到通知
            <span v-if="notificationData.unreadCount && notificationData.unreadCount.value > 0" class="notify__count">{{ notificationData.unreadCount.value }}</span>
          </button>
          <button type="button" :aria-checked="activeTab === 'history'" @click="onSegPointer('history', $event)">
            流转历史
            <span v-if="historyOverdueCount > 0" class="notify__count">{{ historyOverdueCount }}</span>
          </button>
        </UiSegment>
      </div>

      <div v-if="errorState.show" class="notify__alert">
        <UiBanner tone="danger">
          <span>{{ errorState.message }}</span>
          <UiButton variant="ghost" @click="errorState.retryFn?.()">
            重试
          </UiButton>
        </UiBanner>
      </div>

      <!-- 新建下发：左边挑人（旁路降一级），右边写指令 -->
      <div v-if="activeTab === 'send'" class="notify__pane is-split">
        <div class="rail">
          <div class="rail__head">
            <UiSearch
              :model-value="searchKeyword"
              label="搜索岗名、航班或槽位"
              placeholder="岗名 / 在岗人 / 航班号-任务-槽位"
              @update:model-value="onSearchInput"
              @submit="loadOnlineUsers"
            />
          </div>
          <div class="rail__list">
            <button
              v-for="user in onlineUsers"
              :key="user.user_id"
              type="button"
              class="rail__row who"
              :aria-pressed="selectedUserIds.includes(user.user_id)"
              @click="toggleUserSelection(user.user_id)"
            >
              <UiAvatar
                :text="recipientPrimaryLabel(user)"
                size="sm"
                :tone="user.status === 'online' || user.status === 'active' ? 'ok' : 'mute'"
                :label="`${recipientPrimaryLabel(user)}（${user.status === 'online' || user.status === 'active' ? '在线' : '离线'}）`"
              />
              <span class="who__id">
                <span class="who__name">{{ recipientPrimaryLabel(user) }}</span>
                <span class="who__meta">{{ recipientSecondaryLabel(user) }}</span>
              </span>
              <svg
                v-if="selectedUserIds.includes(user.user_id)"
                class="who__mark"
                width="12"
                height="12"
                viewBox="0 0 12 12"
                fill="none"
                aria-hidden="true"
              ><path
                d="M10 3L4.5 8.5L2 6"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              /></svg>
            </button>
            <p v-if="onlineUsers.length === 0" class="void">
              无匹配人员
            </p>
          </div>
        </div>

        <div class="work">
          <div class="work__form">
            <UiField label="通知级别">
              <UiSegment label="通知级别">
                <button type="button" :aria-checked="sendSeverity === 'info'" @click="sendSeverity = 'info'">
                  常规
                </button>
                <button type="button" :aria-checked="sendSeverity === 'warning'" @click="sendSeverity = 'warning'">
                  警告
                </button>
                <button type="button" :aria-checked="sendSeverity === 'critical'" @click="sendSeverity = 'critical'">
                  紧急
                </button>
              </UiSegment>
            </UiField>

            <div class="work__toggle">
              <span class="work__toggle-name">需要回执</span>
              <UiSwitch v-model:checked="receiptRequired" label="需要回执" />
            </div>

            <UiField label="主旨概要" for-id="notify-title">
              <input
                id="notify-title"
                v-model="sendTitle"
                type="text"
                placeholder="输入主题概要"
              >
            </UiField>
            <UiField label="调度指令明细" for-id="notify-body">
              <textarea
                id="notify-body"
                v-model="sendBody"
                placeholder="描述协同动作或通报详情"
                rows="5"
              />
            </UiField>
          </div>
        </div>
      </div>

      <!-- 收到通知：一列消息，行与行只用一根线分开（不做卡中卡） -->
      <div v-if="activeTab === 'inbox'" class="notify__pane is-single">
        <div class="notify__tools">
          <UiButton variant="ghost" :pressed="inboxUnreadOnly" @click="toggleUnreadOnly">
            仅看未读
          </UiButton>
          <UiButton variant="quiet" @click="markAllRead">
            全部标为已读
          </UiButton>
        </div>

        <div class="notify__scroll">
          <p v-if="inboxItems.length === 0" class="void">
            暂无调度通知
          </p>
          <article
            v-for="item in inboxItems"
            :key="item.notification_id"
            class="msg"
            :data-unread="!item.is_read ? 'true' : undefined"
          >
            <div class="msg__top">
              <span class="msg__title">{{ item.title }}</span>
              <UiPill :tone="severityTone(item.severity)">
                {{ getSeverityBadgeLabel(item.severity) }}
              </UiPill>
              <span class="msg__origin">{{ getOriginBadgeLabel(item.origin_type, item.origin_label) }}</span>
              <span class="msg__time">{{ formatDate(item.created_at) }}</span>
            </div>
            <p class="msg__body">
              {{ item.body }}
            </p>
            <div class="msg__foot">
              <span class="msg__from">发信人 {{ item.sender_username || '系统' }}</span>
              <UiButton
                v-if="chatTargetFromNotification(item)"
                variant="tonal"
                @click.stop="openMentionChat(item)"
              >
                打开群聊
              </UiButton>
              <UiButton v-if="!item.is_read" variant="quiet" @click="markAsRead(item.notification_id)">
                标记已读
              </UiButton>
              <template v-if="item.receipt_required && item.ack_status === 'pending'">
                <UiField class="msg__note">
                  <input
                    v-model="ackNote[item.notification_id]"
                    type="text"
                    placeholder="附加回执短讯（选填）"
                    aria-label="回执短讯"
                  >
                </UiField>
                <UiButton variant="tonal" @click="handleAck(item.notification_id, 'acknowledged')">
                  确认执行
                </UiButton>
                <UiButton variant="danger" @click="handleAck(item.notification_id, 'rejected')">
                  无法执行
                </UiButton>
              </template>
              <UiPill
                v-else-if="item.receipt_required"
                :tone="item.ack_status === 'acknowledged' ? 'ok' : 'danger'"
              >
                {{ item.ack_status === 'acknowledged' ? '已查收确认' : '已报备拒绝' }}
              </UiPill>
            </div>
          </article>
        </div>
      </div>

      <!-- 流转历史：左边批次（当前位置用 aria-current），右边回执明细 -->
      <div v-if="activeTab === 'history'" class="notify__pane is-split">
        <div class="rail is-wide">
          <div class="rail__head">
            <span class="rail__title">发信批次</span>
          </div>
          <div class="rail__list">
            <p v-if="historyItems.length === 0" class="void">
              暂无下发记录
            </p>
            <button
              v-for="history in historyItems"
              :key="history.receipt_group_id"
              type="button"
              class="rail__row batch"
              :aria-current="selectedHistoryId === history.receipt_group_id ? 'true' : undefined"
              @click="selectHistory(history.receipt_group_id)"
            >
              <span class="batch__top">
                <span class="batch__name">{{ history.title || '无标题调度' }}</span>
                <span class="batch__time">{{ formatDate(history.created_at || '') }}</span>
              </span>
              <span class="batch__meta">
                <UiPill :tone="severityTone(history.severity)">
                  {{ getSeverityBadgeLabel(history.severity) }}
                </UiPill>
                <span class="batch__origin">{{ getOriginBadgeLabel(history.origin_type, history.origin_label) }}</span>
                <span class="batch__readout">
                  {{ history.total_count }}
                  · 未回 <span :class="{ 'is-warn': history.pending_count > 0 }">{{ history.pending_count }}</span>
                  · 收到 {{ history.acknowledged_count }}
                  · 拒 <span :class="{ 'is-danger': history.rejected_count > 0 }">{{ history.rejected_count }}</span>
                </span>
              </span>
            </button>
          </div>
        </div>

        <div class="work">
          <div v-if="historyDetailFailed" class="void">
            <span>追踪明细加载失败</span>
            <UiButton variant="ghost" @click="selectedHistoryId && selectHistory(selectedHistoryId)">
              重试
            </UiButton>
          </div>
          <!-- 等明细的那几百毫秒画同构的版（§3.9），不丢一句「加载中」 -->
          <div
            v-else
            class="detail"
            :aria-busy="historyDetailData ? undefined : 'true'"
            :aria-label="historyDetailData ? undefined : '正在取回执明细'"
          >
            <div class="detail__head">
              <div class="detail__title-row">
                <h3 v-if="historyDetailData" class="detail__title">
                  {{ historyDetailData.title || '通知详情' }}
                </h3>
                <UiSkeleton v-else width="190px" height="18px" />
                <UiPill v-if="historyDetailData" :tone="severityTone(historyDetailData.severity)">
                  {{ getSeverityBadgeLabel(historyDetailData.severity) }}
                </UiPill>
                <UiSkeleton
                  v-else
                  shape="pill"
                  width="52px"
                  height="22px"
                />
                <span v-if="historyDetailData" class="detail__origin">
                  {{ getOriginBadgeLabel(historyDetailData.origin_type, historyDetailData.origin_label) }}
                </span>
              </div>
              <div class="detail__meta">
                <template v-if="historyDetailData">
                  <span>{{ formatDate(historyDetailData.created_at || '') }}</span>
                  <span v-if="historyDetailData.flight_id">航班 {{ historyDetailFlightLabel }}</span>
                </template>
                <UiSkeleton v-else width="150px" height="12px" />
              </div>
            </div>
            <div class="detail__table">
              <UiTable label="回执明细" density="default">
                <thead>
                  <tr>
                    <th>收信人</th>
                    <th>部门 / 岗位</th>
                    <th>状态</th>
                    <th>操作时间</th>
                    <th>备注</th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="item in historyDetailData?.items ?? []"
                    :key="item.receipt_id || item.notification_id || item.recipient_user_id"
                  >
                    <td class="cell-name">
                      {{ item.recipient_username }}
                    </td>
                    <td>
                      {{ item.recipient_department || '公司' }} · <span class="cell-dim">{{ item.recipient_job_title || '员工' }}</span>
                    </td>
                    <td>
                      <UiPill :tone="receiptTone(item.ack_status, item.is_overdue)">
                        {{ receiptLabel(item.ack_status, item.is_overdue) }}
                      </UiPill>
                    </td>
                    <td data-mono>
                      {{ formatDate(item.ack_at || '') || '—' }}
                    </td>
                    <td class="cell-note" :title="item.ack_note || ''">
                      {{ item.ack_note || '—' }}
                    </td>
                  </tr>
                  <SkeletonTableRow v-if="!historyDetailData" :count="4" :columns="5" />
                  <tr v-else-if="historyDetailData.items.length === 0">
                    <td class="ui-table__empty" colspan="5">
                      这一批没有收信人
                    </td>
                  </tr>
                </tbody>
              </UiTable>
            </div>
          </div>
        </div>
      </div>
    </div>

    <template v-if="activeTab === 'send'" #footer>
      <span class="foot__count">
        已选择 <span class="foot__num">{{ selectedUserIds.length }}</span> 个收件人
      </span>
      <UiButton
        variant="primary"
        size="md"
        :disabled="selectedUserIds.length === 0 || !sendTitle || !sendBody"
        @click="handleSend"
      >
        下发指令
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
/* 满幅的身：三个页签自己撑高、自己滚，所以帽下的工具条要自带内衬 */
.notify {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 480px;
}

.notify__bar {
  flex: none;
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
}

.notify__alert {
  flex: none;
  padding: var(--s3) var(--s4) 0;
}

.notify__count {
  font-variant-numeric: tabular-nums;
  color: var(--ink-subtle);
  font-weight: var(--fw-regular);
}

.notify__pane {
  display: flex;
  flex: 1;
  min-height: 0;
}

.notify__pane.is-split {
  flex-direction: row;
}

.notify__pane.is-single {
  flex-direction: column;
}

.notify__tools {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding: var(--s2) var(--s4);
  border-bottom: 1px solid var(--line);
}

.notify__scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

/* 一列可挑的东西（收件人、批次）：旁路降一级到页底，一根线与工作面分开 */
.rail {
  width: 260px;
  flex: none;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--face-page);
  border-right: 1px solid var(--line);
}

.rail.is-wide {
  width: 300px;
}

.rail__head {
  flex: none;
  padding: var(--s2) var(--s3);
  border-bottom: 1px solid var(--line);
}

.rail__title {
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.rail__list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.rail__row {
  display: flex;
  width: 100%;
  padding: var(--s2) var(--s3);
  border: 0;
  border-bottom: 1px solid var(--line);
  background: transparent;
  color: var(--ink);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.rail__row:hover {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.rail__row:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: -2px;
}

/* 持守：挑中的收件人（aria-pressed）与正在看的批次（aria-current）同一套变位 */
.rail__row[aria-pressed='true'],
.rail__row[aria-current='true'] {
  background: var(--act-soft);
  box-shadow: inset 3px 0 0 var(--act);
}

.who {
  align-items: center;
  gap: var(--s3);
}

.who__id {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.who__name {
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.who__meta {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.who__mark {
  flex: none;
  color: var(--act);
}

.batch {
  flex-direction: column;
  gap: var(--s2);
  padding: var(--s2) var(--s3);
}

.batch__top {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--s3);
}

.batch__name {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.batch__time,
.batch__origin {
  flex: none;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.batch__meta {
  display: flex;
  align-items: center;
  gap: var(--s3);
  flex-wrap: wrap;
}

.batch__readout {
  margin-left: auto;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  font-variant-numeric: tabular-nums;
}

.batch__readout .is-warn { color: var(--warn); }
.batch__readout .is-danger { color: var(--danger); }

/* 右边那一半：表单与明细都在弹窗自己那张抬起面上，不再铺第二层 */
.work {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.work__form {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--s3) var(--s4);
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.work__toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
}

.work__toggle-name {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
}

/* 一条消息：未读是首格内条那一套，不再另画发光小点 */
.msg {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
}

.msg[data-unread='true'] {
  box-shadow: inset 3px 0 0 var(--act);
}

.msg__top {
  display: flex;
  align-items: center;
  gap: var(--s3);
  flex-wrap: wrap;
}

.msg__title {
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  min-width: 0;
}

.msg__origin,
.msg__time,
.msg__from {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.msg__time {
  margin-left: auto;
}

.msg__body {
  margin: 0;
  font-size: var(--fs-body);
  color: var(--ink-subtle);
  line-height: 1.5;
  white-space: pre-wrap;
}

.msg__foot {
  display: flex;
  align-items: center;
  gap: var(--s3);
  flex-wrap: wrap;
}

.msg__note {
  flex: 1;
  min-width: 180px;
}

/* 回执明细：帽下一条说清是哪一批，表自己滚 */
.detail {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.detail__head {
  flex: none;
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
}

.detail__title-row {
  display: flex;
  align-items: center;
  gap: var(--s3);
  flex-wrap: wrap;
  margin-bottom: var(--s2);
}

.detail__title {
  margin: 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  min-width: 0;
}

.detail__meta,
.detail__origin {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.detail__meta {
  display: flex;
  gap: var(--s4);
  flex-wrap: wrap;
}

.detail__table {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.cell-name {
  font-weight: var(--fw-semibold);
  white-space: nowrap;
}

.cell-dim {
  color: var(--ink-muted);
}

.cell-note {
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 空态：一句话，居中淡墨；要给出路才挂一颗静谓词 */
.void {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--s3);
  margin: 0;
  padding: var(--s5) var(--s4);
  color: var(--ink-muted);
  font-size: var(--fs-body);
}

.foot__count {
  margin-right: auto;
  font-size: var(--fs-body);
  color: var(--ink-muted);
}

.foot__num {
  font-weight: var(--fw-semibold);
  color: var(--ink);
  font-variant-numeric: tabular-nums;
}
</style>
