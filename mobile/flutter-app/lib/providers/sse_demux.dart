import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart';
import 'chat_provider.dart';
import 'notification_provider.dart';
import 'session_provider.dart';
import 'workbench_provider.dart';

/// SSE 事件 demux：
/// 单流 `/api/v2/sse/stream` 按 event 名分发到聊天/通知；
/// 未知事件 1.2s 防抖全量刷新兜底。
final sseDemuxProvider = Provider<void>((ref) {
  Timer? debounce;
  ref.onDispose(() => debounce?.cancel());

  void scheduleFallback() {
    debounce?.cancel();
    debounce = Timer(const Duration(milliseconds: 1200), () {
      ref.read(chatGroupsProvider.notifier).softRefresh().catchError((_) {});
      ref.read(notificationsProvider.notifier).softRefresh().catchError((_) {});
      ref.read(unreadCountProvider.notifier).refresh().catchError((_) {});
      ref.read(workbenchProvider.notifier).refresh().catchError((_) {});
    });
  }

  ref.listen<AsyncValue<SseUpdate>>(sseUpdatesProvider, (_, next) {
    final update = next.asData?.value;
    if (update is! SseUpdate_Event) return;
    final event = update.field0.event;
    final data = update.field0.data;

    switch (event) {
      case 'chat_message':
      case 'dispatch_chat_message':
        ref.read(chatGroupsProvider.notifier).softRefresh().catchError((_) {});
        _fanoutChatMessage(ref, data);
      case 'chat_read_synced':
      case 'dispatch_chat_read_synced':
      case 'chat_group_upserted':
      case 'dispatch_chat_group_upserted':
      case 'chat_group_archived':
      case 'dispatch_chat_group_archived':
        ref.read(chatGroupsProvider.notifier).softRefresh().catchError((_) {});
      case 'user_notification':
      case 'notification':
        ref.read(notificationsProvider.notifier).upsertFromJson(data);
        ref.read(unreadCountProvider.notifier).refresh().catchError((_) {});
      case 'initial':
        scheduleFallback();
      case 'heartbeat':
        break;
      default:
        scheduleFallback();
    }
  });
});

/// 当前打开的聊天室 id（ChatRoomScreen 写入；demux 只向活跃室 fanout）。
final activeChatRoomIdProvider =
    NotifierProvider<MutableNotifier<String?>, String?>(
  () => MutableNotifier(null),
);

void _fanoutChatMessage(Ref ref, String data) {
  final activeId = ref.read(activeChatRoomIdProvider);
  if (activeId == null) return;
  ref.read(chatRoomProvider(activeId).notifier).upsertFromJson(data);
}
