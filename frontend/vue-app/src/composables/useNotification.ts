import { ref } from 'vue';
import { useApi } from './useApi';
import { useAuth } from './useAuth';

export interface DispatchOnlineUserOption {
  user_id: string;
  username: string;
  job_title?: string | null;
  department?: string | null;
  status: string;
  login_time?: string | null;
  last_heartbeat?: string | null;
}

export interface DispatchManualNotificationRequest {
  recipient_user_ids: string[];
  title: string;
  body: string;
  severity: "info" | "warning" | "critical";
  flight_id?: string | null;
  flight_no?: string | null;
  receipt_required?: boolean;
}

export interface NotificationResponse {
  notification_id: string;
  user_id: string;
  title: string;
  body: string;
  category: string;
  severity: string;
  is_read: boolean;
  read_status: string;
  delivery_status: string;
  delivered_at?: string | null;
  origin_type: string;
  origin_label: string;
  receipt_required: boolean;
  receipt_group_id?: string | null;
  ack_status: string;
  ack_at?: string | null;
  ack_note?: string | null;
  related_entity_type?: string | null;
  related_entity_id?: string | null;
  flight_id?: string | null;
  group_id?: string | null;
  sender_user_id?: string | null;
  sender_username?: string | null;
  created_at: string;
  read_at?: string | null;
}

export interface SentReceiptGroupSummaryResponse {
  receipt_group_id: string;
  title?: string | null;
  severity: string;
  origin_type: string;
  origin_label: string;
  flight_id?: string | null;
  dispatch_order_id?: string | null;
  group_id?: string | null;
  created_at?: string | null;
  latest_updated_at?: string | null;
  remind_after_at?: string | null;
  is_overdue: boolean;
  total_count: number;
  pending_count: number;
  acknowledged_count: number;
  rejected_count: number;
}

export interface NotificationListResponse {
  items: NotificationResponse[];
  total: number;
  limit: number;
  offset: number;
}

export interface SentReceiptGroupListResponse {
  items: SentReceiptGroupSummaryResponse[];
  total: number;
  limit: number;
  offset: number;
}

export interface SentReceiptDetailItem {
  receipt_id?: string | null;
  notification_id: string;
  recipient_user_id: string;
  recipient_username: string;
  recipient_display_name?: string | null;
  recipient_job_title?: string | null;
  recipient_department?: string | null;
  ack_status: string;
  ack_at?: string | null;
  ack_note?: string | null;
  is_overdue: boolean;
  delivery_status: string;
  delivered_at?: string | null;
}

export interface SentReceiptGroupDetailResponse {
  receipt_group_id: string;
  title?: string | null;
  severity: string;
  origin_type: string;
  origin_label: string;
  flight_id?: string | null;
  created_at?: string | null;
  is_overdue: boolean;
  summary: {
    total_count: number;
    pending_count: number;
    acknowledged_count: number;
    rejected_count: number;
    remind_after_at?: string | null;
    latest_updated_at?: string | null;
    is_overdue: boolean;
  };
  items: SentReceiptDetailItem[];
}

export interface UnreadCountResponse {
  unread_count: number;
}

export interface GenericApiResponse<T = unknown> {
  success: boolean;
  message?: string;
  data?: T;
  errors?: unknown[] | null;
}

const globalUnreadCount = ref(0);
const globalSentReceiptReminderQueue = ref<string[]>([]);
export function useNotification() {
  const api = useApi();
  const auth = useAuth();
  const unreadCount = globalUnreadCount;
  const sentReceiptReminderQueue = globalSentReceiptReminderQueue;
  
  async function fetchOnlineUsers(keyword: string = ''): Promise<{ ok: boolean; items: DispatchOnlineUserOption[]; total: number }> {
    const { ok, data } = await api.get<GenericApiResponse<{items: DispatchOnlineUserOption[], total: number}>>(`${auth.apiBase.value}/notifications/dispatch/online-users?keyword=${encodeURIComponent(keyword)}&limit=120`);
    if (ok && data?.success && data.data) {
      return { ok: true, items: data.data.items, total: data.data.total };
    }
    return { ok: false, items: [], total: 0 };
  }

  async function sendDispatch(payload: DispatchManualNotificationRequest): Promise<boolean> {
    const { ok, data } = await api.post<GenericApiResponse>(`${auth.apiBase.value}/notifications/dispatch/send`, payload);
    return ok && !!data?.success;
  }

  async function fetchInbox(unreadOnly: boolean = false, limit: number = 50, offset: number = 0): Promise<{ ok: boolean; items: NotificationResponse[]; total: number }> {
    const { ok, data } = await api.get<NotificationListResponse>(`${auth.apiBase.value}/notifications?unread_only=${unreadOnly}&limit=${limit}&offset=${offset}`);
    if (ok && data) {
      return { ok: true, items: data.items, total: data.total };
    }
    return { ok: false, items: [], total: 0 };
  }

  async function markRead(notificationId: string): Promise<boolean> {
    const { ok } = await api.post<GenericApiResponse>(`${auth.apiBase.value}/notifications/${notificationId}/read`);
    if (ok) {
      updateUnreadCount();
    }
    return ok;
  }
  
  async function markAllRead(): Promise<boolean> {
    const { ok } = await api.post<GenericApiResponse>(`${auth.apiBase.value}/notifications/read-all`);
    if (ok) {
      updateUnreadCount();
    }
    return ok;
  }

  async function acknowledge(notificationId: string, action: "acknowledged" | "rejected", note?: string): Promise<boolean> {
    const { ok, data } = await api.post<GenericApiResponse>(`${auth.apiBase.value}/notifications/${notificationId}/ack`, { action, note });
    if (ok) {
      updateUnreadCount();
    }
    return ok && !!data?.success;
  }

  async function fetchHistory(limit: number = 20, offset: number = 0): Promise<{ ok: boolean; items: SentReceiptGroupSummaryResponse[]; total: number }> {
    const { ok, data } = await api.get<SentReceiptGroupListResponse>(`${auth.apiBase.value}/notifications/sent-receipt-groups?limit=${limit}&offset=${offset}`);
    if (ok && data) {
      if (Array.isArray(data.items)) {
        data.items.forEach(group => {
          if (group.is_overdue && group.pending_count > 0) {
            if (!sentReceiptReminderQueue.value.includes(group.receipt_group_id)) {
              const reminderKey = `dispatchNotifyReminderShown:${group.receipt_group_id}`;
              if (window.sessionStorage?.getItem(reminderKey) !== '1') {
                window.sessionStorage?.setItem(reminderKey, '1');
                sentReceiptReminderQueue.value.push(group.receipt_group_id);
              }
            }
          }
        });
      }
      return { ok: true, items: data.items, total: data.total };
    }
    return { ok: false, items: [], total: 0 };
  }

  async function fetchHistoryDetail(groupId: string): Promise<SentReceiptGroupDetailResponse | null> {
    const { ok, data } = await api.get<SentReceiptGroupDetailResponse>(`${auth.apiBase.value}/notifications/receipt-groups/${encodeURIComponent(groupId)}`);
    if (!ok || !data || !Array.isArray(data.items)) {
      return null;
    }
    return data;
  }

  async function updateUnreadCount(): Promise<void> {
    const { ok, data } = await api.get<UnreadCountResponse>(`${auth.apiBase.value}/notifications/unread-count`);
    if (ok && data) {
      unreadCount.value = data.unread_count;
    }
  }

  return {
    unreadCount,
    sentReceiptReminderQueue,
    fetchOnlineUsers,
    sendDispatch,
    fetchInbox,
    fetchHistory,
    fetchHistoryDetail,
    markRead,
    markAllRead,
    acknowledge,
    updateUnreadCount
  };
}
