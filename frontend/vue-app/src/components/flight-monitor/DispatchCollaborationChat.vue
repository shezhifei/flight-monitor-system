<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { useDispatchChat, type ChatGroup, type ChatMessage } from '@/composables/useDispatchChat';

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
  <!-- 仅在打开时挂载 Teleport，避免空壳/残留层干扰页面点击 -->
  <Teleport v-if="isOpen && enabled" to="body">
  <div class="dispatch-chat-modal-overlay" @click.self="emit('close')">
    <div class="dispatch-chat-panel" role="dialog" aria-modal="true" aria-label="协同群聊">
      <!-- Group List Pane -->
      <div class="chat-sidebar">
        <div class="sidebar-header">
          <div class="sidebar-header-row">
            <span class="sidebar-title">协同群聊</span>
            <button type="button" class="close-btn" aria-label="关闭群聊" @click="emit('close')">
              &times;
            </button>
          </div>
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
              :class="{ 'is-selected': chatSelectedGroupId === group.group_id }"
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

      <!-- Main Chat Pane -->
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
            <button class="close-btn" @click="emit('close')">
              &times;
            </button>
          </div>
          <div v-if="selectedGroup" class="active-subtitle">
            航班 {{ selectedGroup.flight_id || '-' }} | 成员 {{ selectedGroup.member_count || 0 }}
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
  </div>
  </Teleport>
</template>


<style scoped>
.dispatch-chat-modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  backdrop-filter: blur(4px);
  z-index: 12000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}
.dispatch-chat-panel {
  display: flex;
  height: min(800px, 90vh);
  width: min(1000px, 95vw);
  background: var(--admin-card-bg, var(--bg-card, #fff));
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 12px;
  overflow: hidden;
  font-family: var(--font-sans, "MiSans", system-ui, sans-serif);
  color: var(--admin-text, var(--text-primary));
  box-shadow: 0 20px 50px rgba(0,0,0,0.2);
}
.chat-sidebar {
  width: 280px;
  border-right: 1px solid var(--admin-border, var(--border-light));
  display: flex;
  flex-direction: column;
  background: var(--bg-page, var(--bg-app, #f8fafc));
}
.sidebar-header {
  padding: 12px 14px;
  border-bottom: 1px solid var(--admin-border, var(--border-light));
}
.sidebar-header-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 4px;
}
.sidebar-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--text-primary);
}
.group-meta {
  font-size: 11px;
  color: var(--text-secondary);
}
.close-btn {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 22px;
  line-height: 1;
  cursor: pointer;
  padding: 0 4px;
}
.close-btn:hover {
  color: var(--text-primary);
}
</style>
