import { ref, computed, onBeforeUnmount } from 'vue';
import { useApi } from '@/composables/useApi';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import { type DispatchChatSSEEvent, normalizeDispatchChatEvent } from '@/composables/sseEvents';
import { describeApiError } from '@/composables/useApiError';
import type { Stakeholder } from '@/composables/useMentionStakeholders';

export interface ChatGroup {
  group_id: string;
  // API uses `name`; some callers / older payloads used `group_name` — we
  // mirror the value into both fields so downstream code can read either.
  name: string;
  group_name?: string;
  flight_id?: string;
  status?: string;
  read_only?: boolean;
  member_count?: number;
  unread_count?: number;
  last_message_preview?: string;
  last_message_at?: string;
  last_message_seq?: number;
  archived_at?: string;
  [key: string]: unknown;
}

export interface ChatMessage {
  // Server uses `id` (camel) but the existing component / legacy payloads
  // reference `message_id`. We expose both for compatibility.
  id?: string;
  message_id?: string;
  group_id?: string;
  seq_no?: number;
  message_type?: string;
  sender_name?: string;
  sender_id?: string;
  sender_user_id?: string;
  sender_username?: string;
  content: string;
  sent_at?: string;
  at_all?: boolean;
  is_at_all?: boolean;
  mention_user_ids?: string[];
  [key: string]: unknown;
}

interface GroupMemberRaw {
  user_id?: string;
  username?: string;
  is_assignee?: boolean;
  is_dispatcher?: boolean;
  is_active?: boolean;
}

function asTrimmedStringIds(value: unknown): string[] | null {
  if (!Array.isArray(value)) return null;
  return value
    .filter((item): item is string => typeof item === 'string')
    .map((item) => item.trim())
    .filter((item) => item !== '');
}

/** Prefer top-level `mention_user_ids`; fall back to `metadata.mention_user_ids`. */
export function mentionUserIdsFromRaw(raw: Record<string, unknown>): string[] {
  const fromTop = asTrimmedStringIds(raw.mention_user_ids);
  if (fromTop !== null) return fromTop;
  const metadata = raw.metadata;
  if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) {
    const fromMeta = asTrimmedStringIds((metadata as Record<string, unknown>).mention_user_ids);
    if (fromMeta !== null) return fromMeta;
  }
  return [];
}

export type SendChatResult =
  | { ok: true; message: ChatMessage }
  | { ok: false; reason: 'archived' | 'no-group' | 'error' };

export type OpenGroupByFlightResult =
  | { ok: true; group: ChatGroup; status: 200 }
  | { ok: false; notMember: true; status: 404 }
  | { ok: false; notMember: false; status: number };

interface ChatGroupsResponse {
  items?: ChatGroup[];
  unread_total?: number;
}

interface ChatMessagesResponse {
  items?: ChatMessage[];
  has_more?: boolean;
  next_before_seq?: number;
}

interface ChatReadResponse {
  unread_count?: number;
  unread_total?: number;
}

function normalizeGroup(raw: Record<string, unknown>): ChatGroup {
  const name = (raw.name as string | undefined) ?? (raw.group_name as string | undefined) ?? '';
  return {
    ...(raw as ChatGroup),
    group_id: String(raw.group_id ?? ''),
    name,
    group_name: (raw.group_name as string | undefined) ?? name,
    flight_id: raw.flight_id as string | undefined,
    status: raw.status as string | undefined,
    read_only: raw.read_only as boolean | undefined,
    member_count: Number(raw.member_count ?? 0) || undefined,
    unread_count: Number(raw.unread_count ?? 0) || 0,
    last_message_preview: raw.last_message_preview as string | undefined,
    last_message_at: raw.last_message_at as string | undefined,
    last_message_seq: Number(raw.last_message_seq ?? 0) || undefined,
    archived_at: raw.archived_at as string | undefined,
  };
}

function normalizeMessage(raw: Record<string, unknown>): ChatMessage {
  const senderName = (raw.sender_name as string | undefined) ?? (raw.sender_username as string | undefined) ?? (raw.sender_user_id as string | undefined);
  const senderId = (raw.sender_id as string | undefined) ?? (raw.sender_user_id as string | undefined);
  return {
    ...(raw as ChatMessage),
    id: (raw.id as string | undefined) ?? (raw.message_id as string | undefined),
    message_id: (raw.message_id as string | undefined) ?? (raw.id as string | undefined),
    group_id: raw.group_id as string | undefined,
    seq_no: Number(raw.seq_no ?? 0) || undefined,
    message_type: (raw.message_type as string | undefined) ?? 'user',
    sender_name: senderName,
    sender_id: senderId,
    sender_user_id: senderId,
    sender_username: senderName,
    content: String(raw.content ?? ''),
    sent_at: raw.sent_at as string | undefined,
    at_all: Boolean(raw.at_all ?? raw.is_at_all),
    is_at_all: Boolean(raw.is_at_all ?? raw.at_all),
    mention_user_ids: mentionUserIdsFromRaw(raw),
  };
}

export function chatMessageIdentity(msg: ChatMessage): string {
  const messageId = String(msg.message_id || msg.id || '').trim();
  if (messageId) return `id:${messageId}`;
  const groupId = String(msg.group_id || '').trim();
  const seqNo = Number(msg.seq_no || 0);
  if (groupId && seqNo > 0) return `seq:${groupId}:${seqNo}`;
  const sender = String(msg.sender_user_id || msg.sender_id || '').trim();
  return `fallback:${msg.sent_at ?? ''}|${sender}|${msg.content}`;
}

export function appendUniqueChatMessage(list: ChatMessage[], incoming: ChatMessage): ChatMessage[] {
  const identity = chatMessageIdentity(incoming);
  if (list.some((msg) => chatMessageIdentity(msg) === identity)) {
    return list;
  }
  const next = [...list, incoming];
  next.sort((a, b) => (a.seq_no || 0) - (b.seq_no || 0));
  return next;
}

export function newChatClientMsgId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `c${Date.now().toString(36)}${Math.random().toString(36).slice(2, 12)}`;
}

export function useDispatchChat() {
  const { get, post } = useApi();
  const auth = useAuth();
  const toast = useToast();
  const CHAT_STREAM_URL = '/api/v2/sse/stream?topics=';

  const chatEnabled = ref(true);
  const chatGroups = ref<ChatGroup[]>([]);
  const chatMessages = ref<ChatMessage[]>([]);
  const chatSelectedGroupId = ref<string>('');
  const chatPanelVisible = ref(false);

  const chatLoadingGroups = ref(false);
  const chatLoadingMessages = ref(false);
  const chatMessagesError = ref('');
  const chatSending = ref(false);
  const chatMessagesHasMore = ref(false);
  const chatMessagesNextBeforeSeq = ref<number | null>(null);
  const chatUnreadTotal = ref(0);
  const chatGroupMembers = ref<Stakeholder[]>([]);
  let loadGeneration = 0;
  let memberLoadGeneration = 0;

  const activeGroup = computed(() => {
    return chatGroups.value.find(g => g.group_id === chatSelectedGroupId.value) || null;
  });

  function sortChatGroups() {
    chatGroups.value.sort((a, b) => {
      const timeA = a.last_message_at ? new Date(a.last_message_at).getTime() : 0;
      const timeB = b.last_message_at ? new Date(b.last_message_at).getTime() : 0;
      return timeB - timeA;
    });
  }

  function syncChatUnreadTotalFromGroups() {
    chatUnreadTotal.value = chatGroups.value.reduce((sum, g) => sum + Math.max(0, g.unread_count || 0), 0);
  }

  function upsertGroup(group: ChatGroup) {
    const normalized = normalizeGroup(group as unknown as Record<string, unknown>);
    const idx = chatGroups.value.findIndex(g => g.group_id === normalized.group_id);
    if (idx >= 0) {
      chatGroups.value[idx] = { ...chatGroups.value[idx], ...normalized };
    } else {
      chatGroups.value.push(normalized);
    }
    sortChatGroups();
  }

  async function loadChatGroups(options: { silent?: boolean } = {}) {
    if (!chatEnabled.value) return false;

    chatLoadingGroups.value = true;
    try {
      const response = await get<ChatGroupsResponse>('/api/v2/dispatch/collaboration/groups?status=all&limit=120&offset=0');
      if (response.ok && response.data) {
        const payload = response.data;
        chatGroups.value = Array.isArray(payload.items) ? payload.items.map(g => normalizeGroup(g as unknown as Record<string, unknown>)) : [];
        sortChatGroups();

        if (Number.isFinite(Number(payload.unread_total))) {
          chatUnreadTotal.value = Math.max(0, Number(payload.unread_total));
        } else {
          syncChatUnreadTotalFromGroups();
        }

        if (chatSelectedGroupId.value && !chatGroups.value.some(g => g.group_id === chatSelectedGroupId.value)) {
          chatSelectedGroupId.value = '';
          chatMessages.value = [];
          chatMessagesHasMore.value = false;
          chatMessagesNextBeforeSeq.value = null;
          memberLoadGeneration += 1;
          chatGroupMembers.value = [];
        }
        return true;
      }
      if (!options.silent) {
        toast.showToast('error', describeApiError(response.data, `群聊列表加载失败 (${response.status})`));
      }
      return false;
    } catch (error) {
      if (!options.silent) {
        console.error('Failed to load chat groups', error);
        toast.showToast('error', `群聊列表加载失败: ${error instanceof Error ? error.message : String(error)}`);
      }
      return false;
    } finally {
      chatLoadingGroups.value = false;
    }
  }

  async function loadChatMessages(
    groupId: string,
    options: { prepend?: boolean; beforeSeq?: number | null; afterSeq?: number | null } = {},
  ) {
    if (!groupId) return;
    const prepend = options.prepend === true;
    const afterSeq = options.afterSeq;
    if (prepend && chatLoadingMessages.value) return;

    const generation = ++loadGeneration;
    chatLoadingMessages.value = true;
    chatMessagesError.value = '';
    const beforeSeq = options.beforeSeq;

    try {
      const params = new URLSearchParams();
      params.set('limit', afterSeq != null ? '200' : prepend ? '40' : '50');
      if (afterSeq != null && afterSeq >= 0) {
        params.set('after_seq', String(afterSeq));
      } else if (beforeSeq && beforeSeq > 0) {
        params.set('before_seq', String(beforeSeq));
      }

      const response = await get<ChatMessagesResponse>(`/api/v2/dispatch/collaboration/groups/${encodeURIComponent(groupId)}/messages?${params.toString()}`);
      if (generation !== loadGeneration) return;
      if (!response.ok) {
        chatMessagesError.value = `聊天记录加载失败 (${response.status})`;
        toast.showToast('error', chatMessagesError.value);
        return;
      }
      if (response.data) {
        if (chatSelectedGroupId.value !== groupId) return;

        const payload = response.data;
        const items: ChatMessage[] = Array.isArray(payload.items) ? payload.items.map(m => normalizeMessage(m as unknown as Record<string, unknown>)) : [];

        if (afterSeq != null && afterSeq >= 0) {
          for (const item of items) {
            chatMessages.value = appendUniqueChatMessage(chatMessages.value, item);
          }
        } else if (prepend) {
          const existingIds = new Set(chatMessages.value.map(chatMessageIdentity));
          const newItems = items.filter(m => !existingIds.has(chatMessageIdentity(m)));
          chatMessages.value = [...newItems, ...chatMessages.value];
          chatMessages.value.sort((a, b) => (a.seq_no || 0) - (b.seq_no || 0));
          chatMessagesHasMore.value = Boolean(payload.has_more);
          chatMessagesNextBeforeSeq.value = Number(payload.next_before_seq || 0) || null;
        } else {
          chatMessages.value = items;
          chatMessages.value.sort((a, b) => (a.seq_no || 0) - (b.seq_no || 0));
          chatMessagesHasMore.value = Boolean(payload.has_more);
          chatMessagesNextBeforeSeq.value = Number(payload.next_before_seq || 0) || null;
        }
      }
    } catch (error) {
      if (generation !== loadGeneration) return;
      console.error('Failed to load messages', error);
      chatMessagesError.value = error instanceof Error ? error.message : '聊天记录加载失败';
      toast.showToast('error', chatMessagesError.value);
    } finally {
      if (generation === loadGeneration) {
        chatLoadingMessages.value = false;
      }
    }
  }

  async function loadMoreMessages() {
    if (!chatSelectedGroupId.value) return;
    if (!chatMessagesHasMore.value || chatLoadingMessages.value) return;
    await loadChatMessages(chatSelectedGroupId.value, { prepend: true, beforeSeq: chatMessagesNextBeforeSeq.value });
  }

  async function markChatGroupRead(groupId: string, readSeq?: number | null) {
    if (!groupId || !chatEnabled.value) return;

    const body: { read_seq?: number } = {};
    if (readSeq && readSeq > 0) {
      body.read_seq = readSeq;
    }

    try {
      const response = await post<ChatReadResponse>(`/api/v2/dispatch/collaboration/groups/${encodeURIComponent(groupId)}/read`, body);
      if (response.ok && response.data) {
        const payload = response.data;
        const group = chatGroups.value.find(g => g.group_id === groupId);
        if (group) {
          group.unread_count = Math.max(0, Number(payload.unread_count || 0));
        }
        if (Number.isFinite(Number(payload.unread_total))) {
          chatUnreadTotal.value = Math.max(0, Number(payload.unread_total));
        } else {
          syncChatUnreadTotalFromGroups();
        }
      } else if (!response.ok) {
        toast.showToast('error', describeApiError(response.data, `标记群聊已读失败 (${response.status})`));
      }
    } catch (error) {
      console.error('Failed to mark read', error);
      toast.showToast('error', `标记群聊已读失败: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  async function loadGroupMembers(groupId: string): Promise<void> {
    const id = String(groupId || '').trim();
    const generation = ++memberLoadGeneration;
    if (!id) {
      chatGroupMembers.value = [];
      return;
    }

    try {
      const response = await get<{ items?: GroupMemberRaw[] }>(
        `/api/v2/dispatch/collaboration/groups/${encodeURIComponent(id)}/members`,
      );
      if (generation !== memberLoadGeneration) return;
      if (!response.ok || !response.data) {
        chatGroupMembers.value = [];
        return;
      }
      const items = Array.isArray(response.data.items) ? response.data.items : [];
      chatGroupMembers.value = items
        .map((item) => {
          const userId = String(item.user_id ?? '').trim();
          const username = String(item.username ?? '').trim() || userId;
          return {
            user_id: userId,
            username,
            is_assignee: Boolean(item.is_assignee),
            is_dispatcher: Boolean(item.is_dispatcher),
          };
        })
        .filter((member) => member.user_id !== '');
    } catch {
      if (generation !== memberLoadGeneration) return;
      chatGroupMembers.value = [];
    }
  }

  async function selectChatGroup(groupId: string, options: { refreshMessages?: boolean, markRead?: boolean } = {}) {
    if (!groupId) {
      memberLoadGeneration += 1;
      chatGroupMembers.value = [];
      return;
    }

    const changed = chatSelectedGroupId.value !== groupId;
    chatSelectedGroupId.value = groupId;

    if (changed) {
      chatMessages.value = [];
      chatMessagesHasMore.value = false;
      chatMessagesNextBeforeSeq.value = null;
      chatGroupMembers.value = [];
    }

    void loadGroupMembers(groupId);

    if (options.refreshMessages !== false) {
      await loadChatMessages(groupId, { prepend: false });
    }

    if (options.markRead !== false && chatPanelVisible.value) {
      await markChatGroupRead(groupId);
    }
  }

  async function sendChatMessage(
    content: string,
    atAll: boolean = false,
    mentionUserIds: string[] = [],
  ): Promise<SendChatResult> {
    if (!chatEnabled.value || chatSending.value) return { ok: false, reason: 'error' };

    const selectedGroup = activeGroup.value;
    if (!selectedGroup) return { ok: false, reason: 'no-group' };

    const isArchived = Boolean(selectedGroup.read_only) || String(selectedGroup.status || '').toLowerCase() === 'archived';
    if (isArchived) return { ok: false, reason: 'archived' };

    chatSending.value = true;
    try {
      const response = await post<ChatMessage>(`/api/v2/dispatch/collaboration/groups/${encodeURIComponent(selectedGroup.group_id)}/messages`, {
        content,
        at_all: atAll,
        mention_user_ids: mentionUserIds,
        client_msg_id: newChatClientMsgId(),
      });

      if (!response.ok) {
        toast.showToast('error', `消息发送失败 (${response.status})`);
        return { ok: false, reason: 'error' };
      }
      if (response.data) {
        const message = normalizeMessage(response.data as unknown as Record<string, unknown>);
        chatMessages.value = appendUniqueChatMessage(chatMessages.value, message);

        selectedGroup.last_message_preview = String(message.content || '');
        selectedGroup.last_message_at = message.sent_at || new Date().toISOString();
        selectedGroup.last_message_seq = Number(message.seq_no || selectedGroup.last_message_seq || 0);
        selectedGroup.unread_count = 0;

        syncChatUnreadTotalFromGroups();
        sortChatGroups();

        return { ok: true, message };
      }
    } catch (error) {
      console.error('Failed to send message', error);
      toast.showToast('error', `消息发送失败: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      chatSending.value = false;
    }
    return { ok: false, reason: 'error' };
  }

  // --- SSE Logic ---
  let stream: EventSource | null = null;
  let reconnectTimer: number | null = null;
  let streamEverConnected = false;
  const RECONNECT_DELAY_MS = 5000;
  const sessionActive = ref(false);

  function applyUnreadTotal(unreadTotal: number | undefined) {
    if (Number.isFinite(Number(unreadTotal))) {
      chatUnreadTotal.value = Math.max(0, Number(unreadTotal));
    } else {
      syncChatUnreadTotalFromGroups();
    }
  }

  function handleSsePayload(event: DispatchChatSSEEvent) {
    switch (event.type) {
      case 'dispatch_chat_initial': {
        chatGroups.value = (event.items || []).map(g => normalizeGroup(g as unknown as Record<string, unknown>));
        sortChatGroups();
        applyUnreadTotal(event.unread_total);
        if (chatSelectedGroupId.value && !chatGroups.value.some(g => String(g.group_id) === chatSelectedGroupId.value)) {
          chatSelectedGroupId.value = '';
          chatMessages.value = [];
          memberLoadGeneration += 1;
          chatGroupMembers.value = [];
        }
        break;
      }
      case 'dispatch_chat_message': {
        const groupId = String(event.group_id || '').trim();
        const msg = event.message;
        if (!groupId || !msg) return;

        let group = chatGroups.value.find(g => String(g.group_id) === groupId);
        if (!group) {
          group = normalizeGroup({
            group_id: groupId,
            flight_id: String(event.flight_id || ''),
            name: `群聊 ${event.flight_id || '-'}`,
            group_name: `群聊 ${event.flight_id || '-'}`,
            status: 'active',
            read_only: false,
            unread_count: 0,
          });
          chatGroups.value.push(group);
        }

        const normalized = normalizeMessage(msg as unknown as Record<string, unknown>);
        group.last_message_preview = String(normalized.content || '');
        group.last_message_at = normalized.sent_at || new Date().toISOString();
        group.last_message_seq = Number(normalized.seq_no || group.last_message_seq || 0);
        group.unread_count = Math.max(0, Number(event.unread_count || 0));

        applyUnreadTotal(event.unread_total);
        sortChatGroups();

        if (chatSelectedGroupId.value === groupId) {
          chatMessages.value = appendUniqueChatMessage(chatMessages.value, normalized);
          const user = auth.getUser();
          const currentUserId = String(user?.sub || user?.id || user?.user_id || '').trim();
          if (
            chatPanelVisible.value
            && normalized.sender_user_id
            && currentUserId
            && normalized.sender_user_id !== currentUserId
          ) {
            markChatGroupRead(groupId, Number(normalized.seq_no || 0));
          }
        }
        break;
      }
      case 'dispatch_chat_group_upserted': {
        if (event.group && event.group.group_id) {
          upsertGroup(normalizeGroup(event.group as unknown as Record<string, unknown>));
          if (!chatSelectedGroupId.value) chatSelectedGroupId.value = String(event.group.group_id);
          syncChatUnreadTotalFromGroups();
        }
        break;
      }
      case 'dispatch_chat_group_archived': {
        const groupId = String(event.group_id || '').trim();
        if (groupId) {
          const group = chatGroups.value.find(g => String(g.group_id) === groupId);
          if (group) {
            group.status = 'archived';
            group.read_only = true;
            const archivedAt = (event as unknown as { archived_at?: string }).archived_at;
            if (archivedAt) group.archived_at = archivedAt;
            sortChatGroups();
          }
        }
        break;
      }
      case 'dispatch_chat_read_synced': {
        const groupId = String(event.group_id || '').trim();
        if (groupId) {
          const group = chatGroups.value.find(g => String(g.group_id) === groupId);
          if (group) group.unread_count = Math.max(0, Number(event.unread_count || 0));
          applyUnreadTotal(event.unread_total);
        }
        break;
      }
    }
  }

  function disconnectStream(clearReconnect = true) {
    if (stream) {
      try { stream.close(); } catch { /* stream already closed */ }
      stream = null;
    }
    if (clearReconnect && reconnectTimer) {
      window.clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  }

  function scheduleReconnect() {
    if (!sessionActive.value || !chatEnabled.value || reconnectTimer) return;
    reconnectTimer = window.setTimeout(() => {
      reconnectTimer = null;
      connectStream();
    }, RECONNECT_DELAY_MS);
  }

  async function recoverAfterReconnect() {
    await loadChatGroups({ silent: true });
    const groupId = chatSelectedGroupId.value;
    if (!groupId) return;
    const lastSeq = chatMessages.value.reduce((max, msg) => Math.max(max, Number(msg.seq_no || 0)), 0);
    await loadChatMessages(groupId, { afterSeq: lastSeq });
    if (chatPanelVisible.value) {
      await markChatGroupRead(groupId);
    }
  }

  function setChatPanelVisible(visible: boolean) {
    chatPanelVisible.value = visible;
    if (visible && chatSelectedGroupId.value) {
      void markChatGroupRead(chatSelectedGroupId.value);
    }
  }

  function connectStream() {
    if (!sessionActive.value || !chatEnabled.value) return;
    const isReconnect = streamEverConnected;

    void auth.refreshSSEToken();
    disconnectStream(false);

    try {
      stream = auth.getEventSource(CHAT_STREAM_URL, { clientScope: 'dispatch_chat' });
    } catch {
      scheduleReconnect();
      return;
    }

    if (!stream) {
      scheduleReconnect();
      return;
    }

    const parsePayload = (raw: unknown): unknown => {
      if (!raw) return null;
      if (typeof raw === 'object') return raw;
      if (typeof raw !== 'string') return null;
      try { return JSON.parse(raw); } catch { return null; }
    };

    const dispatchFrame = (raw: unknown, evtName: string) => {
      const event = normalizeDispatchChatEvent(parsePayload(raw), evtName);
      if (event) handleSsePayload(event);
    };

    const bindEvent = (evtName: string) => {
      stream!.addEventListener(evtName, (e: MessageEvent) => dispatchFrame(e.data, evtName));
    };

    ['initial', 'chat_message', 'chat_group_upserted', 'chat_group_archived', 'chat_read_synced'].forEach(bindEvent);

    stream.onmessage = (e: MessageEvent) => dispatchFrame(e.data, '');

    stream.onopen = () => {
      streamEverConnected = true;
      if (isReconnect) {
        void recoverAfterReconnect();
      }
    };

    stream.onerror = () => {
      disconnectStream(false);
      scheduleReconnect();
    };
  }

  function initChatSession() {
    if (!chatEnabled.value) return;
    sessionActive.value = true;
    connectStream();
  }

  function destroyChatSession() {
    sessionActive.value = false;
    streamEverConnected = false;
    chatPanelVisible.value = false;
    disconnectStream();
  }

  async function openGroupByFlightId(flightId: string): Promise<OpenGroupByFlightResult> {
    const normalized = String(flightId || '').trim();
    if (!normalized) return { ok: false, notMember: false, status: 0 };
    try {
      const response = await get<ChatGroup>(`/api/v2/dispatch/collaboration/groups/by-flight/${encodeURIComponent(normalized)}`);
      if (response.ok && response.data) {
        const group = normalizeGroup(response.data as unknown as Record<string, unknown>);
        upsertGroup(group);
        syncChatUnreadTotalFromGroups();
        await selectChatGroup(group.group_id, { refreshMessages: true, markRead: true });
        return { ok: true, group, status: 200 };
      }
      if (response.status === 404) {
        return { ok: false, notMember: true, status: 404 };
      }
      toast.showToast('error', describeApiError(response.data, `打开航班群聊失败 (${response.status})`));
      return { ok: false, notMember: false, status: response.status };
    } catch (error) {
      console.error('Failed to fetch chat group by flight', error);
      toast.showToast('error', `打开航班群聊失败: ${error instanceof Error ? error.message : String(error)}`);
      return { ok: false, notMember: false, status: 0 };
    }
  }

  onBeforeUnmount(() => {
    destroyChatSession();
  });

  return {
    chatEnabled,
    chatGroups,
    chatMessages,
    chatSelectedGroupId,
    chatLoadingGroups,
    chatLoadingMessages,
    chatMessagesError,
    chatSending,
    chatMessagesHasMore,
    chatMessagesNextBeforeSeq,
    chatUnreadTotal,
    chatGroupMembers,
    activeGroup,

    loadChatGroups,
    loadChatMessages,
    loadMoreMessages,
    loadGroupMembers,
    selectChatGroup,
    markChatGroupRead,
    sendChatMessage,
    initChatSession,
    destroyChatSession,
    setChatPanelVisible,
    openGroupByFlightId,
  };
}
