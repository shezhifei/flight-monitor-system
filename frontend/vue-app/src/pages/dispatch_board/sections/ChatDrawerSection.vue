<script setup lang="ts">
import { computed } from 'vue';
import type { ChatGroup, ChatMessage } from '@/composables/useDispatchChat';

const props = defineProps<{
  isChatDrawerVisible: boolean;
  chatGroupList: ChatGroup[];
  chatActiveGroup: string | null;
  chatMessageList: ChatMessage[];
  chatMessagesError: string;
  chatInput: string;
}>();

const visibleChatMessages = computed(() => (props.chatMessageList ?? []).slice(-200));

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'selectChatGroup', groupId: string): void;
  (e: 'sendChatMessage'): void;
  (e: 'update:chatInput', val: string): void;
}>();
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
            <div v-for="msg in visibleChatMessages" :key="msg.id || msg.seq_no" class="chat-message"><div class="msg-header"><span class="msg-user">{{ msg.sender_name || '未知' }}</span><span class="msg-time">{{ msg.sent_at ? new Date(msg.sent_at).toLocaleTimeString() : '' }}</span></div><div class="msg-body">{{ msg.content }}</div></div>
            <div v-if="!chatMessagesError && chatMessageList.length === 0" class="chat-empty-state">暂无消息</div>
          </div>
          <div class="chat-composer">
            <textarea id="chatInput" :value="chatInput" maxlength="2000" placeholder="输入消息，Enter 发送，Ctrl+Enter 换行" @input="emit('update:chatInput', ($event.target as HTMLTextAreaElement).value)" @keyup.ctrl.enter="() => { if(chatInput.trim() && chatActiveGroup) emit('sendChatMessage'); }" />
          </div>
        </section>
      </div>
    </div>
  </aside>
</template>
