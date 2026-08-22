<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useDispatchChat, type ChatGroup, type ChatMessage } from '@/composables/useDispatchChat';
import { useAuth } from '@/composables/useAuth';
import UiModal from '../ui/UiModal.vue';
import UiPill from '../ui/UiPill.vue';
import UiBanner from '../ui/UiBanner.vue';
import UiCheckChip from '../ui/UiCheckChip.vue';
import ChatMessageList from '../ui/ChatMessageList.vue';
import ChatSender from '../ui/ChatSender.vue';

/**
 * 协同群聊：左边一列群，右边一间房。
 * 气泡、Markdown、滚到底、翻旧话都是 ChatMessageList 的活；
 * 输入框、字数、发送钮是 ChatSender 的活。这里只做两件事：
 * 把群列表铺开，把服务端的消息形状翻成会话流认得的形状。
 */
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

const auth = useAuth();

function openGroup(groupId: string) {
  return selectChatGroup(groupId, { refreshMessages: true, markRead: true });
}

const inputDraft = ref('');
const atAll = ref(false);

const enabled = computed(() => props.enabled ?? true);
const selectedGroup = computed<ChatGroup | null>(() => activeGroup.value);

const groupTitle = (group: ChatGroup) => group.group_name || group.name || group.group_id;

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

/** 会话流认的一条话，外加本页要在气泡里显的两样（发话人、@全体）。 */
interface StreamMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  time?: string;
  sender: string;
  atAll: boolean;
}

/** 我的身份牌：服务端回填的发话人可能是 id、user_id 或 username 里的任一个。 */
function currentIdentity(): Set<string> {
  const user = auth.getUser();
  const ids = [user?.id, user?.sub, user?.user_id, user?.username]
    .map((value) => String(value ?? '').trim())
    .filter((value) => value !== '');
  return new Set(ids);
}

function isMine(msg: ChatMessage, me: Set<string>): boolean {
  return [msg.sender_user_id, msg.sender_id, msg.sender_username].some((value) => {
    const id = String(value ?? '').trim();
    return id !== '' && me.has(id);
  });
}

function senderLabel(msg: ChatMessage, mine: boolean): string {
  if (mine) return '我';
  const name = msg.sender_username || msg.sender_name || msg.sender_user_id || msg.sender_id;
  return String(name || '系统');
}

/**
 * 键必须稳定：会话流靠首尾两个键分辨「头上接了旧话」还是「尾上来了新话」，
 * 前者钉住视线、后者落到底。随机键会让每次刷新都被当成新话，视线被甩到底。
 */
function messageKey(msg: ChatMessage): string {
  const messageId = String(msg.message_id || msg.id || '').trim();
  if (messageId) return messageId;
  const groupId = String(msg.group_id || '').trim();
  const seqNo = Number(msg.seq_no || 0);
  if (groupId && seqNo > 0) return `${groupId}:${seqNo}`;
  const sender = String(msg.sender_user_id || msg.sender_id || '').trim();
  return `${msg.sent_at ?? ''}|${sender}|${msg.content}`;
}

const streamMessages = computed<StreamMessage[]>(() => {
  const me = currentIdentity();
  return chatMessages.value.map((msg) => {
    const mine = isMine(msg, me);
    return {
      id: messageKey(msg),
      role: msg.message_type === 'system' ? 'system' : (mine ? 'user' : 'assistant'),
      content: String(msg.content ?? ''),
      time: msg.sent_at ? formatDateTime(msg.sent_at) : undefined,
      sender: senderLabel(msg, mine),
      atAll: Boolean(msg.is_at_all || msg.at_all),
    };
  });
});

const streamEmptyText = computed(() => {
  if (chatLoadingMessages.value) return '消息加载中…';
  if (!selectedGroup.value) return '选择左侧群组开始沟通';
  return '暂无消息，发送第一条沟通信息';
});

/** 触顶就往头上接更早的话；没有更早的、或正在取，就不再叫。 */
function onReachStart(): void {
  if (!chatMessagesHasMore.value || chatLoadingMessages.value) return;
  void loadMoreMessages();
}

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
};
</script>

<template>
  <UiModal
    :open="Boolean(isOpen && enabled)"
    title="协同群聊"
    :width="1000"
    bleed
    @close="emit('close')"
  >
    <div class="collab">
      <aside class="collab__groups">
        <div class="collab__groups-head">
          {{ chatGroups.length }} 个群
        </div>
        <div class="collab__groups-list">
          <p v-if="chatLoadingGroups && chatGroups.length === 0" class="collab__tip">
            群列表加载中…
          </p>
          <p v-else-if="chatGroups.length === 0" class="collab__tip">
            当前暂无可见群聊
          </p>
          <template v-else>
            <button
              v-for="group in chatGroups"
              :key="group.group_id"
              type="button"
              class="collab__group"
              :aria-pressed="chatSelectedGroupId === group.group_id"
              @click="openGroup(group.group_id)"
            >
              <span class="collab__group-title">
                <span class="collab__group-name">{{ groupTitle(group) }}</span>
                <UiPill v-if="isGroupArchived(group)">已归档</UiPill>
                <UiPill v-else-if="group.unread_count" tone="act">
                  {{ group.unread_count > 99 ? '99+' : group.unread_count }}
                </UiPill>
              </span>
              <span class="collab__group-preview">
                {{ truncateText(group.last_message_preview || '暂无消息', 40) }}
              </span>
              <span class="collab__group-time">{{ formatDateTime(group.last_message_at) }}</span>
            </button>
          </template>
        </div>
      </aside>

      <section class="collab__room">
        <header class="collab__room-head">
          <h3>
            {{ selectedGroup ? groupTitle(selectedGroup) : '请选择群组' }}
            <UiPill v-if="selectedGroup && isGroupArchived(selectedGroup)" tone="warn">
              只读
            </UiPill>
          </h3>
          <p v-if="selectedGroup">
            航班 {{ selectedGroup.flight_id || '-' }} · 成员 {{ selectedGroup.member_count || 0 }}
          </p>
        </header>

        <ChatMessageList
          :messages="streamMessages"
          :empty-text="streamEmptyText"
          class="collab__stream"
          @reach-start="onReachStart"
        >
          <template #body="{ msg }">
            <span v-if="msg.role !== 'system'" class="collab__sender">
              {{ msg.sender }}
              <UiPill v-if="msg.atAll" tone="warn">@全体</UiPill>
            </span>
            <span class="collab__text">{{ msg.content }}</span>
          </template>
        </ChatMessageList>

        <footer class="collab__composer">
          <UiBanner v-if="selectedGroup && isGroupArchived(selectedGroup)" tone="warn">
            群聊已归档，只读不可发送
          </UiBanner>
          <ChatSender
            v-model="inputDraft"
            :disabled="composerDisabled"
            :maxlength="2000"
            placeholder="输入消息，Enter 发送，Shift+Enter 换行"
            @send="sendMessage"
          >
            <template #tools>
              <UiCheckChip
                id="collabAtAll"
                v-model:checked="atAll"
                label="@全体"
                :disabled="composerDisabled"
              />
            </template>
          </ChatSender>
        </footer>
      </section>
    </div>
  </UiModal>
</template>

<style scoped>
/* 气泡、Markdown、滚动到底、翻旧话都在 ChatMessageList；
   输入框与发送钮在 ChatSender；胶囊在 UiPill。这里只剩「群列表 + 房间」这个二分。 */

.collab {
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
  min-height: 0;
  height: min(560px, calc(100vh - 220px));
}

/* 群列表是旁路：降一级到页底，一根线与房间分开 */
.collab__groups {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--face-page);
  border-right: 1px solid var(--line);
}

.collab__groups-head {
  flex: none;
  padding: var(--s2) var(--s3);
  border-bottom: 1px solid var(--line);
  color: var(--ink-subtle);
  font-size: var(--fs-label);
}

.collab__groups-list {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: var(--s1);
}

.collab__tip {
  margin: 0;
  padding: var(--s3);
  color: var(--ink-muted);
  font-size: var(--fs-label);
}

/* 一个群一行：常态无底，交感洗工作面，持守（当前群）落行动衬 + 首边一条 */
.collab__group {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  padding: var(--s2) var(--s3);
  border: 0;
  border-left: 2px solid transparent;
  border-radius: var(--r-cell);
  background: none;
  color: var(--ink);
  font-family: inherit;
  font-size: var(--fs-body);
  text-align: left;
  cursor: pointer;
}

.collab__group:hover {
  background: var(--face-work);
}

.collab__group[aria-pressed='true'] {
  border-left-color: var(--act);
  background: var(--act-soft);
}

.collab__group:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: -2px;
}

.collab__group-title {
  display: flex;
  align-items: center;
  gap: var(--s2);
  min-width: 0;
}

.collab__group-name {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: var(--fw-medium);
}

.collab__group-preview {
  overflow: hidden;
  color: var(--ink-subtle);
  font-size: var(--fs-label);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.collab__group-time {
  color: var(--ink-muted);
  font-family: var(--mono);
  font-size: var(--fs-label);
}

/* 房间：帽 / 会话流 / 发送器，三段，只有中间一段滚 */
.collab__room {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.collab__room-head {
  flex: none;
  padding: var(--s2) var(--s3);
  border-bottom: 1px solid var(--line);
}

.collab__room-head h3 {
  display: flex;
  align-items: center;
  gap: var(--s2);
  margin: 0;
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.collab__room-head p {
  margin: 2px 0 0;
  color: var(--ink-subtle);
  font-size: var(--fs-label);
}

.collab__stream {
  padding: var(--s3);
}

/* 发话人贴在气泡里第一行，比正文淡一档 */
.collab__sender {
  display: flex;
  align-items: center;
  gap: var(--s2);
  margin-bottom: 2px;
  color: var(--ink-subtle);
  font-size: var(--fs-label);
}

.collab__text {
  white-space: pre-wrap;
}

.collab__composer {
  flex: none;
  display: flex;
  flex-direction: column;
  gap: var(--s2);
  padding: var(--s2) var(--s3);
  border-top: 1px solid var(--line);
}
</style>
