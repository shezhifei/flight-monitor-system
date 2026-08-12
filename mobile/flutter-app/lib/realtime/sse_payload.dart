import 'dart:convert';

/// Parsed chat message from a universal SSE `chat_message` payload.
class SseChatMessage {
  const SseChatMessage({
    required this.messageId,
    required this.seqNo,
    required this.groupId,
    required this.content,
  });

  final String messageId;
  final int seqNo;
  final String groupId;
  final String content;
}

/// Parsed notification from a universal SSE `user_notification` payload.
class SseNotification {
  const SseNotification({
    required this.notificationId,
    required this.title,
    required this.body,
    this.receiptGroupId,
  });

  final String notificationId;
  final String title;
  final String body;
  final String? receiptGroupId;
}

SseChatMessage? parseSseChatMessage(String dataJson) {
  try {
    final map = jsonDecode(dataJson) as Map<String, dynamic>;
    final raw = map['message'] is Map
        ? Map<String, dynamic>.from(map['message'] as Map)
        : map;
    final id = (raw['message_id'] ?? '').toString();
    if (id.isEmpty) return null;
    final gid = (raw['group_id'] ?? map['group_id'] ?? '').toString();
    return SseChatMessage(
      messageId: id,
      seqNo: _asInt(raw['seq_no']),
      groupId: gid,
      content: (raw['content'] ?? '').toString(),
    );
  } catch (_) {
    return null;
  }
}

SseNotification? parseSseNotification(String dataJson) {
  try {
    final map = jsonDecode(dataJson) as Map<String, dynamic>;
    final raw = map['notification'] is Map
        ? Map<String, dynamic>.from(map['notification'] as Map)
        : map;
    final id = (raw['notification_id'] ?? '').toString();
    if (id.isEmpty) return null;
    return SseNotification(
      notificationId: id,
      title: (raw['title'] ?? '').toString(),
      body: (raw['body'] ?? '').toString(),
      receiptGroupId: raw['receipt_group_id'] as String?,
    );
  } catch (_) {
    return null;
  }
}

/// Insert [item] if [key] has not been seen. Returns the new list (sorted
/// when [compare] is provided). Used by chat seq / notification id upsert.
List<T> upsertUnique<T, K>(
  List<T> current,
  T item,
  K key,
  Set<K> seen, {
  int Function(T a, T b)? compare,
}) {
  if (!seen.add(key)) return current;
  final next = List<T>.from(current)..add(item);
  if (compare != null) next.sort(compare);
  return next;
}

int _asInt(Object? v) {
  if (v is int) return v;
  if (v is num) return v.toInt();
  return int.tryParse(v?.toString() ?? '') ?? 0;
}
