<script setup lang="ts">
import { computed, ref, unref } from 'vue';
import type { ChatGroup, ChatMessage } from '@/composables/useDispatchChat';
import type { Stakeholder } from '@/composables/useMentionStakeholders';
import { useAuth } from '@/composables/useAuth';
import ChatSender from '@/components/ui/ChatSender.vue';
import ChatMentionBody from '@/components/ui/ChatMentionBody.vue';

const props = defineProps<{
  isChatDrawerVisible: boolean;
  chatGroupList: ChatGroup[];
  chatActiveGroup: string | null;
  chatMessageList: ChatMessage[];
  chatMessagesError: string;
  chatInput: string;
  chatGroupMembers: Stakeholder[];
}>();

const visibleChatMessages = computed(() => (props.chatMessageList ?? []).slice(-200));

const activeGroup = computed(() => {
  const groupId = String(props.chatActiveGroup || '').trim();
  return props.chatGroupList.find((group) => group.group_id === groupId) || null;
});

const isArchived = computed(() => {
  const group = activeGroup.value;
  if (!group) return false;
  return Boolean(group.read_only) || String(group.status || '').toLowerCase() === 'archived';
});

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'selectChatGroup', groupId: string): void;
  (e: 'sendChatMessage', payload: { mentionUserIds: string[]; atAll: boolean }): void;
  (e: 'update:chatInput', val: string): void;
}>();

const auth = useAuth();
const senderRef = ref<{ mentionIds: string[]; atAll: boolean; resetMentions: () => void } | null>(null);

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

function isMentioned(msg: ChatMessage): boolean {
  const me = currentIdentity();
  if (isMine(msg, me)) return false;
  if (Boolean(msg.at_all || msg.is_at_all)) return true;
  return (msg.mention_user_ids ?? []).some((id) => {
    const value = String(id ?? '').trim();
    return value !== '' && me.has(value);
  });
}

function onSend() {
  const sender = senderRef.value;
  emit('sendChatMessage', {
    mentionUserIds: [...(unref(sender?.mentionIds) ?? [])],
    atAll: Boolean(unref(sender?.atAll)),
  });
}
</script>

<template>
  <aside id="chatDrawer" class="drawer chat-drawer" :class="{ open: isChatDrawerVisible }" :aria-hidden="!isChatDrawerVisible">
    <div class="drawer-header"><h3 class="drawer-title">航班保障群聊</h3><button id="chatCloseBtn" class="panel-close-btn" aria-label="关闭群聊抽屉" @click="emit('close')">×</button></div>
    <div class="drawer-body">
      <div class="chat-layout">
        <section class="chat-group-panel">
          <div class="chat-group-panel-head"><span>群列表</span><span class="chat-group-meta">{{ chatGroupList.length }} 个群</span></div>
          <div id="chatGroupList" class="chat-group-list">
            <div v-for="group in chatGroupList" :key="group.group_id" class="chat-group-item" :class="{ active: chatActiveGroup === group.group_id }" @click="emit('selectChatGroup', group.group_id)"><span>{{ group.name }}</span><span v-if="(group.unread_count || 0) > 0" class="unread-badge">{{ group.unread_count }}</span></div>
            <div v-if="chatGroupList.length === 0" class="empty-list-tip">暂无群组</div>
          </div>
        </section>
        <section class="chat-message-panel">
          <div class="chat-message-head"><div><p class="chat-title">{{ chatActiveGroup ? '群聊' : '请选择群组' }}</p><p class="chat-subtitle">仅成员可见</p></div></div>
          <div id="chatMessageList" class="chat-message-list">
            <div v-if="chatMessagesError" class="chat-empty-state" role="alert">{{ chatMessagesError }}</div>
            <div
              v-for="msg in visibleChatMessages"
              :key="msg.id || msg.seq_no"
              class="chat-message"
              :data-mentioned="isMentioned(msg) ? 'true' : undefined"
            >
              <div class="msg-header">
                <span class="msg-user">{{ msg.sender_name || '未知' }}</span>
                <span class="msg-time">{{ msg.sent_at ? new Date(msg.sent_at).toLocaleTimeString() : '' }}</span>
              </div>
              <div class="msg-body">
                <ChatMentionBody :content="msg.content" />
              </div>
            </div>
            <div v-if="!chatMessagesError && chatMessageList.length === 0" class="chat-empty-state">暂无消息</div>
          </div>
          <div class="chat-composer">
            <p v-if="isArchived" class="chat-readonly-tip">群聊已归档，只读不可发送</p>
            <ChatSender
              ref="senderRef"
              :model-value="chatInput"
              :disabled="isArchived || !chatActiveGroup"
              :maxlength="2000"
              :stakeholders="chatGroupMembers"
              include-all-mention
              placeholder="输入消息，Enter 发送，Shift+Enter 换行"
              @update:model-value="emit('update:chatInput', $event)"
              @send="onSend"
            />
          </div>
        </section>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.chat-message[data-mentioned='true'] {
  box-shadow: inset 3px 0 0 var(--act);
}
</style>
