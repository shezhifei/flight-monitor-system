import { useSSE } from '@/composables/useSSE';
import { useToast } from '@/composables/useToast';

/**
 * 全局回执 toast 通道。
 *
 * 发送端"某某已确认/已拒绝《xxx》"的实时回执不应该依赖用户正好停在哪个页面——
 * 挂在 WorkspacePage 壳层时，用户在 dashboard / 建模器 / 任何独立页面都收不到。
 * 因此由 bootstrapProtectedPage 在每个受保护页面统一拉起一条统一 SSE 流
 * （服务端自动附加 user_notifications_{uid} / user_dispatch_chat_{uid}），
 * 只消费 sender_receipt_update 一类事件。
 *
 * critical 强制确认弹窗不归这里管：那是 flight_monitor 标签页自己的
 * CriticalNotifyModal。
 */

const RECEIPT_STREAM_URL = '/api/v2/sse/stream?topics=global_status';

function coerceRecord(raw: unknown): Record<string, unknown> | null {
  if (typeof raw === 'string') {
    try {
      return JSON.parse(raw) as Record<string, unknown>;
    } catch {
      return null;
    }
  }
  return raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : null;
}

export function startGlobalReceiptToasts(): void {
  const { showToast } = useToast();

  function handleSenderReceiptUpdate(rawPayload: unknown): void {
    const payload = coerceRecord(rawPayload);
    if (!payload) return;

    const recipientLabel =
      String(payload.recipient_username || payload.recipient_user_id || '对方').trim() || '对方';
    const title = String(payload.title || '通知').trim() || '通知';
    const ackStatus = String(payload.ack_status || '').trim().toLowerCase();

    if (ackStatus === 'acknowledged') {
      showToast('success', `${recipientLabel} 已确认《${title}》`, { duration: 3200 });
    } else if (ackStatus === 'rejected') {
      const suffix = payload.ack_note ? `：${String(payload.ack_note).trim()}` : '';
      showToast('warning', `${recipientLabel} 已拒绝《${title}》${suffix}`, { duration: 4200 });
    }
  }

  const sse = useSSE({
    url: RECEIPT_STREAM_URL,
    authenticated: true,
    autoReconnect: true,
    clientScope: 'global_receipt_toasts',
  });

  // 命名事件（event: sender_receipt_update）与按 topic 分派的默认 message 帧都接住
  sse.on('sender_receipt_update', (event) => {
    handleSenderReceiptUpdate((event as MessageEvent).data);
  });

  // 页面级通道：不随组件卸载，生命周期 = 页面生命周期
  void sse.connect();
}
