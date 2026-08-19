<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { useDispatchChat, type ChatGroup, type ChatMessage } from '@/composables/useDispatchChat';
import UiModal from '../ui/UiModal.vue';

const props = defineProps<{
  flightId?: string | null;
  groupId?: string;
  enabled?: boolean;
  isOpen?: boolean;
}>();

const emit = defineEmits<{
  (e: 'error', msg: string): void;
  (e: 'toast', msg: string): void;
  (e: 'close'): void;
}>();

const {
  chatGroups,
  chatMessages,
  chatSelectedGroupId,
  chatLoadingGroups,
  chatLoadingMessages,
  chatSending,
  chatMessagesHasMore,
  activeGroup,
  loadChatGroups,
  selectChatGroup,
  sendChatMessage,
  initChatSession,
  destroyChatSession,
  openGroupByFlightId,
  loadMoreMessages,
} = useDispatchChat();

function openGroup(groupId: string) {
  return selectChatGroup(groupId, { refreshMessages: true, markRead: true });
}

const inputDraft = ref('');
const atAll = ref(false);

const messageListRef = ref<HTMLElement | null>(null);
const inputRef = ref<HTMLTextAreaElement | null>(null);

const enabled = computed(() => props.enabled ?? true);
const selectedGroup = computed<ChatGroup | null>(() => activeGroup.value);

const isGroupArchived = (group: ChatGroup | null) => {
  if (!group) return false;
  return Boolean(group.read_only) || String(group.status || '').toLowerCase() === 'archived';
};

const composerDisabled = computed(() => {
  return !selectedGroup.value || isGroupArchived(selectedGroup.value) || chatSending.value;
});

const formatDateTime = (value: string | number | undefined): string => {
  if (!value) return '-';
  const ms = typeof value === 'number' ? value : Date.parse(value);
  if (Number.isNaN(ms)) return '-';
  const date = new Date(ms);
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  const hour = String(date.getHours()).padStart(2, '0');
  const minute = String(date.getMinutes()).padStart(2, '0');
  return `${month}-${day} ${hour}:${minute}`;
};

const truncateText = (text: string, limit = 180) => {
  if (!text) return '';
  const normalized = String(text).replace(/\s+/g, ' ').trim();
  if (!normalized) return '';
  if (normalized.length <= limit) return normalized;
  return `${normalized.slice(0, Math.max(0, limit - 1))}…`;
};

const formatMessageLines = (content?: string) => {
  return String(content ?? '').split(/\r?\n/);
};

const isMine = (msg: ChatMessage) => {
  const senderId = String(msg.sender_user_id || msg.sender_id || '').trim();
  if (!senderId) return false;
  return false;
};

const getMessageKey = (message: ChatMessage) => {
  const messageId = String(message.message_id || message.id || '').trim();
  if (messageId) return messageId;
  const groupId = String(message.group_id || '').trim();
  const seqNo = Number(message.seq_no || 0);
  if (groupId && seqNo > 0) return `${groupId}:${seqNo}`;
  return Math.random().toString(36).substr(2, 9);
};

const scrollToBottom = () => {
  nextTick(() => {
    if (messageListRef.value) {
      messageListRef.value.scrollTop = messageListRef.value.scrollHeight;
    }
  });
};

const onMessageScroll = () => {
  if (!messageListRef.value) return;
  if (messageListRef.value.scrollTop > 80) return;
  if (!chatMessagesHasMore.value) return;
  const prevHeight = messageListRef.value.scrollHeight;
  const prevTop = messageListRef.value.scrollTop;
  void loadMoreMessages().then(() => {
    nextTick(() => {
      if (!messageListRef.value) return;
      const nextHeight = messageListRef.value.scrollHeight;
      messageListRef.value.scrollTop = Math.max(0, nextHeight - prevHeight + prevTop);
    });
  });
};

const showToast = (msg: string) => {
  emit('toast', msg);
};

const initSession = async () => {
  if (!enabled.value) return;
  initChatSession();
  const loaded = await loadChatGroups({ silent: chatGroups.value.length > 0 });
  // Keep panel usable even when list is empty / first load fails — user may still open by flight.
  if (!loaded && chatGroups.value.length === 0) {
    // soft: still try flight-scoped open below
  }

  if (props.flightId) {
    const result = await openGroupByFlightId(props.flightId);
    if (!result.ok) {
      if (result.notMember) {
        showToast('你不在该航班群聊中，或该航班尚未建立群聊');
      } else if (result.status !== 0) {
        showToast(`打开航班群聊失败 (${result.status})`);
      }
      if (chatGroups.value.length > 0) {
        await openGroup(chatGroups.value[0].group_id);
      }
    } else {
      // Keep sidebar in sync after force-join / first open
      void loadChatGroups({ silent: true });
    }
  } else if (props.groupId) {
    await openGroup(props.groupId);
  } else if (chatGroups.value.length > 0) {
    await openGroup(chatGroups.value[0].group_id);
  }
};

watch(() => props.isOpen, (open) => {
  if (open && enabled.value) {
    void initSession();
  } else if (!open) {
    // keep stream when closed so unread can still update; only stop when disabled
  }
});

watch(() => props.flightId, (newFlightId) => {
  if (props.isOpen && newFlightId) {
    void openGroupByFlightId(newFlightId).then((result) => {
      if (!result.ok && result.notMember) {
        showToast('你不在该航班群聊中，或该航班尚未建立群聊');
      }
    });
  }
});

watch(() => props.groupId, (newGroupId) => {
  if (props.isOpen && newGroupId) {
    void openGroup(newGroupId);
  }
});

watch(() => props.enabled, (newEnabled) => {
  if (!newEnabled) {
    destroyChatSession();
  } else if (props.isOpen) {
    void initSession();
  }
});

onMounted(() => {
  if (props.isOpen && enabled.value) {
    void initSession();
  }
});

onBeforeUnmount(() => {
  destroyChatSession();
});

const sendMessage = async () => {
  if (!enabled.value || chatSending.value) return;
  const group = selectedGroup.value;
  if (!group) return;
  if (isGroupArchived(group)) {
    showToast('群聊已归档，只读不可发送');
    return;
  }
  const content = inputDraft.value.trim();
  if (!content) return;

  const result = await sendChatMessage(content, Boolean(atAll.value));
  if (!result.ok) {
    if (result.reason === 'archived') {
      showToast('群聊已归档，只读不可发送');
    } else if (result.reason !== 'no-group') {
      showToast('消息发送失败');
    }
    return;
  }

  inputDraft.value = '';
  atAll.value = false;
  scrollToBottom();
};
</script>

<template>
  <UiModal :open="Boolean(isOpen && enabled)" title="协同群聊" :width="1000" @close="emit('close')">
    <div class="dispatch-chat-panel">
      <div class="chat-sidebar">
        <div class="sidebar-header">
          <span class="group-meta">{{ chatGroups.length }} 个群</span>
        </div>
        <div class="group-list">
          <div v-if="chatLoadingGroups && chatGroups.length === 0" class="empty-tip">
            群列表加载中...
          </div>
          <div v-else-if="chatGroups.length === 0" class="empty-tip">
            当前暂无可见群聊
          </div>
          <template v-else>
            <button
              v-for="group in chatGroups"
              :key="group.group_id"
              class="group-item"
              :aria-pressed="chatSelectedGroupId === group.group_id"
              @click="openGroup(group.group_id)"
            >
              <div class="group-main">
                <span class="group-title">{{ group.group_name || group.name || group.group_id }}</span>
                <span v-if="isGroupArchived(group)" class="group-status">已归档</span>
              </div>
              <div class="group-sub">
                {{ truncateText(group.last_message_preview || '暂无消息', 40) }}
              </div>
              <div class="group-meta-row">
                <span>{{ formatDateTime(group.last_message_at) }}</span>
                <span v-if="group.unread_count && group.unread_count > 0" class="group-unread">{{ group.unread_count > 99 ? '99+' : group.unread_count }}</span>
              </div>
            </button>
          </template>
        </div>
      </div>

      <div class="chat-main">
        <div class="chat-header">
          <div class="header-top">
            <template v-if="selectedGroup">
              <div class="active-title-area">
                <h3 class="active-title">
                  {{ selectedGroup.group_name || selectedGroup.name || selectedGroup.group_id }}
                </h3>
                <span v-if="isGroupArchived(selectedGroup)" class="archive-pill">已归档</span>
              </div>
            </template>
            <template v-else>
              <h3 class="active-title">
                请选择群组
              </h3>
            </template>
          </div>
          <div v-if="selectedGroup" class="active-subtitle">
            航班 {{ selectedGroup.flight_id || '-' }} · 成员 {{ selectedGroup.member_count || 0 }}
          </div>
        </div>

        <div ref="messageListRef" class="message-list" @scroll="onMessageScroll">
          <div v-if="chatLoadingMessages && chatMessages.length === 0" class="loading-tip">
            消息加载中...
          </div>
          <div v-else-if="!selectedGroup" class="empty-tip">
            选择左侧群组开始沟通
          </div>
          <div v-else-if="chatMessages.length === 0" class="empty-tip">
            暂无消息，发送第一条沟通信息
          </div>
          <template v-else>
            <div
              v-for="msg in chatMessages"
              :key="getMessageKey(msg)"
              class="message-row"
              :class="{ 'is-mine': isMine(msg), 'is-system': msg.message_type === 'system' }"
            >
              <template v-if="msg.message_type === 'system'">
                <div class="system-message">
                  <span>{{ msg.content }}</span>
                </div>
              </template>
              <template v-else>
                <div class="message-meta">
                  <span>{{ isMine(msg) ? '我' : (msg.sender_username || msg.sender_name || msg.sender_user_id || msg.sender_id || '系统') }}</span>
                  <span>{{ formatDateTime(msg.sent_at) }}</span>
                </div>
                <div class="message-bubble">
                  <span v-if="msg.is_at_all" class="message-atall">@全体</span>
                  <div class="message-content">
                    <template v-for="(line, lineIndex) in formatMessageLines(msg.content)" :key="lineIndex">
                      <br v-if="lineIndex > 0">
                      {{ line }}
                    </template>
                  </div>
                </div>
              </template>
            </div>
          </template>
        </div>

        <div class="chat-composer">
          <div v-if="selectedGroup && isGroupArchived(selectedGroup)" class="readonly-tip">
            群聊已归档，只读不可发送
          </div>
          <div class="composer-toolbar">
            <label class="at-all-label" :class="{ 'is-disabled': composerDisabled }">
              <input v-model="atAll" type="checkbox" :disabled="composerDisabled">
              @全体
            </label>
          </div>
          <div class="composer-input-area">
            <textarea
              ref="inputRef"
              v-model="inputDraft"
              class="composer-input"
              :disabled="composerDisabled"
              placeholder="输入消息..."
              maxlength="2000"
              @keydown.enter.exact.prevent="sendMessage"
            />
          </div>
          <div class="composer-footer">
            <span class="input-count">{{ inputDraft.length }}/2000</span>
            <button
              class="send-btn"
              :disabled="composerDisabled || !inputDraft.trim()"
              @click="sendMessage"
            >
              发送
            </button>
          </div>
        </div>
      </div>
    </div>
  </UiModal>
</template>

<style scoped>
.dispatch-chat-panel {
  display: flex;
  min-height: 520px;
  height: min(620px, 68vh);
  margin: -16px -18px;
  color: var(--ink);
  overflow: hidden;
}
.chat-sidebar {
  width: 280px;
  border-right: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  background: var(--face-work);
  flex-shrink: 0;
}
.sidebar-header {
  padding: 10px 14px;
  border-bottom: 1px solid var(--line);
}
.group-meta {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  font-variant-numeric: tabular-nums;
}
.group-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.group-item {
  width: 100%;
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: var(--face-raised);
  text-align: left;
  display: flex;
  flex-direction: column;
  gap: 4px;
  cursor: pointer;
  color: var(--ink);
}
.group-item:hover {
  border-color: var(--line-strong);
}
.group-item[aria-pressed="true"] {
  border-color: var(--act);
  background: var(--act-soft);
  color: var(--act);
}
.group-item:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
.group-main {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.group-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}
.group-status,
.archive-pill {
  flex-shrink: 0;
  padding: 1px 6px;
  border-radius: var(--r-pill);
  border: 1px solid var(--warn);
  background: var(--warn-soft);
  color: var(--warn);
  font-size: 11px;
}
.group-sub {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}
.group-meta-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  color: var(--ink-muted);
}
.group-unread {
  min-width: 18px;
  padding: 0 5px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--r-pill);
  background: var(--danger-soft);
  color: var(--danger);
  font-variant-numeric: tabular-nums;
  font-size: 11px;
  font-weight: var(--fw-semibold);
}
.chat-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: var(--face-raised);
}
.chat-header {
  padding: 10px 16px;
  border-bottom: 1px solid var(--line);
}
.header-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.active-title-area {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.active-title {
  margin: 0;
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}
.active-subtitle {
  margin-top: 4px;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}
.message-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.empty-tip,
.loading-tip {
  padding: 24px 12px;
  text-align: center;
  font-size: var(--fs-body);
  color: var(--ink-muted);
}
.message-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-items: flex-start;
}
.message-row.is-mine {
  align-items: flex-end;
}
.message-meta {
  display: flex;
  gap: 8px;
  font-size: 11px;
  color: var(--ink-muted);
}
.message-bubble {
  max-width: 80%;
  padding: 8px 12px;
  border-radius: var(--r-control);
  background: var(--face-work);
  border: 1px solid var(--line);
  color: var(--ink);
  font-size: var(--fs-body);
  line-height: 1.45;
}
.message-row.is-mine .message-bubble {
  background: var(--act-soft);
  border-color: var(--act);
}
.message-atall {
  display: inline-block;
  margin-right: 6px;
  font-size: 11px;
  color: var(--act);
  font-weight: var(--fw-semibold);
}
.system-message {
  width: 100%;
  text-align: center;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}
.chat-composer {
  border-top: 1px solid var(--line);
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.readonly-tip {
  font-size: var(--fs-label);
  color: var(--warn);
}
.composer-toolbar {
  display: flex;
  align-items: center;
}
.at-all-label {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.at-all-label.is-disabled {
  color: var(--ink-muted);
}
.composer-input {
  width: 100%;
  min-height: 64px;
  resize: vertical;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  padding: 8px 10px;
  background: var(--face-work);
  color: var(--ink);
  font-size: var(--fs-body);
  box-sizing: border-box;
}
.composer-input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
.composer-input:disabled {
  color: var(--ink-muted);
}
.composer-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.input-count {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  font-variant-numeric: tabular-nums;
}
.send-btn {
  height: var(--h-sm);
  padding: 0 14px;
  border: 1px solid var(--act);
  border-radius: var(--r-control);
  background: var(--act);
  color: var(--act-on);
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  cursor: pointer;
}
.send-btn:disabled {
  background: var(--ink-muted);
  border-color: var(--ink-muted);
  cursor: not-allowed;
}
.send-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}
</style>
