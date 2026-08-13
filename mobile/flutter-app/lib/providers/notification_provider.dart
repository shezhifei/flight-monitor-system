import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/constants.dart';
import '../bridge/api/notification.dart' as notif_api;

/// 通知列表。
final notificationsProvider = AsyncNotifierProvider<NotificationsNotifier,
    List<notif_api.Notification>>(NotificationsNotifier.new);

class NotificationsNotifier
    extends AsyncNotifier<List<notif_api.Notification>> {
  final Set<String> _seenIds = {};

  @override
  Future<List<notif_api.Notification>> build() => _load();

  Future<List<notif_api.Notification>> _load() async {
    final page = await notif_api.notifications(
      limit: AppConstants.notificationPageSize,
      offset: 0,
      onlyUnread: false,
    );
    _seenIds
      ..clear()
      ..addAll(page.items.map((n) => n.notificationId));
    return page.items;
  }

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(_load);
  }

  Future<void> softRefresh() async {
    state = await AsyncValue.guard(_load);
  }

  void upsert(notif_api.Notification item) {
    if (!_seenIds.add(item.notificationId)) {
      final list =
          List<notif_api.Notification>.from(state.asData?.value ?? const []);
      final i = list.indexWhere((n) => n.notificationId == item.notificationId);
      if (i >= 0) {
        list[i] = item;
        state = AsyncData(list);
      }
      return;
    }
    final list =
        List<notif_api.Notification>.from(state.asData?.value ?? const []);
    list.insert(0, item);
    state = AsyncData(list);
  }

  void upsertFromJson(String dataJson) {
    try {
      final map = jsonDecode(dataJson) as Map<String, dynamic>;
      final raw = map['notification'] is Map
          ? Map<String, dynamic>.from(map['notification'] as Map)
          : map;
      final id = (raw['notification_id'] ?? '').toString();
      if (id.isEmpty) return;
      upsert(notif_api.Notification(
        notificationId: id,
        userId: (raw['user_id'] ?? '').toString(),
        title: (raw['title'] ?? '').toString(),
        body: (raw['body'] ?? '').toString(),
        category: (raw['category'] ?? '').toString(),
        severity: (raw['severity'] ?? '').toString(),
        isRead: raw['is_read'] == true,
        readStatus: (raw['read_status'] ?? 'unread').toString(),
        deliveryStatus: (raw['delivery_status'] ?? 'sent').toString(),
        deliveredAt: raw['delivered_at'] as String?,
        originType: (raw['origin_type'] ?? 'manual').toString(),
        originLabel: (raw['origin_label'] ?? '人工').toString(),
        receiptRequired: raw['receipt_required'] == true,
        receiptGroupId: raw['receipt_group_id'] as String?,
        ackStatus: (raw['ack_status'] ?? 'pending').toString(),
        ackAt: raw['ack_at'] as String?,
        ackNote: raw['ack_note'] as String?,
        relatedEntityType: raw['related_entity_type'] as String?,
        relatedEntityId: raw['related_entity_id'] as String?,
        createdAt: (raw['created_at'] ?? '').toString(),
        readAt: raw['read_at'] as String?,
      ));
    } catch (_) {}
  }

  Future<void> markRead(String id) async {
    await notif_api.notificationRead(id: id);
    await softRefresh();
  }

  Future<void> markAllRead() async {
    await notif_api.notificationReadAll();
    await softRefresh();
  }

  Future<void> ack(String id, String action, {String? note}) async {
    await notif_api.notificationAck(id: id, action: action, note: note);
    await softRefresh();
  }
}

/// 未读数（工作台徽章 / 导航联动）。
final unreadCountProvider =
    AsyncNotifierProvider<UnreadCountNotifier, int>(UnreadCountNotifier.new);

class UnreadCountNotifier extends AsyncNotifier<int> {
  @override
  Future<int> build() async => (await notif_api.unreadCount()).toInt();

  Future<void> refresh() async {
    state = await AsyncValue.guard(
      () async => (await notif_api.unreadCount()).toInt(),
    );
  }
}

/// 回执组详情。
final receiptGroupProvider =
    FutureProvider.family<notif_api.ReceiptGroup, String>(
  (ref, id) => notif_api.receiptGroup(receiptGroupId: id),
);
