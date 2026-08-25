import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/constants.dart';
import '../bridge/api/chat.dart';

/// 聊天群列表。
final chatGroupsProvider =
    AsyncNotifierProvider<ChatGroupsNotifier, ChatGroupList>(
  ChatGroupsNotifier.new,
);

class ChatGroupsNotifier extends AsyncNotifier<ChatGroupList> {
  @override
  Future<ChatGroupList> build() => _load();

  Future<ChatGroupList> _load() => chatGroups(
        status: 'active',
        limit: AppConstants.chatListPageSize,
        offset: 0,
      );

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(_load);
  }

  Future<void> softRefresh() async {
    state = await AsyncValue.guard(_load);
  }
}

/// 单个聊天室消息（按 groupId family）。
/// Riverpod 3：`AsyncNotifierProvider.family` 的 create 接收 arg。
final chatRoomProvider = AsyncNotifierProvider.family<ChatRoomNotifier,
    List<ChatMessage>, String>(ChatRoomNotifier.new);

class ChatRoomNotifier extends AsyncNotifier<List<ChatMessage>> {
  ChatRoomNotifier(this.groupId);
  final String groupId;

  final Set<int> _seenSeq = {};

  @override
  Future<List<ChatMessage>> build() => _loadInitial();

  Future<List<ChatMessage>> _loadInitial() async {
    final page = await chatMessages(
      groupId: groupId,
      limit: AppConstants.chatMessagePageSize,
    );
    _seenSeq
      ..clear()
      ..addAll(page.items.map((m) => m.seqNo.toInt()));
    final items = List<ChatMessage>.from(page.items)
      ..sort((a, b) => a.seqNo.toInt().compareTo(b.seqNo.toInt()));
    return items;
  }

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(_loadInitial);
  }

  Future<bool> loadMore() async {
    final current = state.asData?.value ?? const <ChatMessage>[];
    if (current.isEmpty) return false;
    final oldest = current.first.seqNo;
    final page = await chatMessages(
      groupId: groupId,
      limit: AppConstants.chatMessagePageSize,
      beforeSeq: oldest,
    );
    if (page.items.isEmpty) return false;
    final merged = <ChatMessage>[];
    for (final m in page.items) {
      final seq = m.seqNo.toInt();
      if (_seenSeq.add(seq)) merged.add(m);
    }
    merged.sort((a, b) => a.seqNo.toInt().compareTo(b.seqNo.toInt()));
    state = AsyncData([...merged, ...current]);
    return page.hasMore;
  }

  Future<ChatMessage> send(
    String content, {
    bool atAll = false,
    List<String> mentionUserIds = const [],
  }) async {
    final msg = await sendChatMessage(
      groupId: groupId,
      content: content,
      atAll: atAll,
      mentionUserIds: mentionUserIds,
    );
    upsert(msg);
    return msg;
  }

  void upsert(ChatMessage msg) {
    final seq = msg.seqNo.toInt();
    if (!_seenSeq.add(seq)) return;
    final current = List<ChatMessage>.from(state.asData?.value ?? const []);
    current.add(msg);
    current.sort((a, b) => a.seqNo.toInt().compareTo(b.seqNo.toInt()));
    state = AsyncData(current);
  }

  void upsertFromJson(String dataJson) {
    try {
      final map = jsonDecode(dataJson) as Map<String, dynamic>;
      final raw = map['message'] is Map
          ? Map<String, dynamic>.from(map['message'] as Map)
          : map;
      final gid = (raw['group_id'] ?? map['group_id'] ?? '').toString();
      if (gid.isNotEmpty && gid != groupId) return;
      final msg = ChatMessage(
        messageId: (raw['message_id'] ?? '').toString(),
        seqNo: _asInt(raw['seq_no']),
        groupId: gid.isEmpty ? groupId : gid,
        senderUserId: raw['sender_user_id'] as String?,
        senderUsername: raw['sender_username'] as String?,
        messageType: (raw['message_type'] ?? 'text').toString(),
        content: (raw['content'] ?? '').toString(),
        isAtAll: raw['is_at_all'] == true,
        mentionUserIds: _asStringList(raw['mention_user_ids']),
        sentAt: (raw['sent_at'] ?? '').toString(),
      );
      if (msg.messageId.isEmpty) return;
      upsert(msg);
    } catch (_) {}
  }

  Future<void> markReadUpToLatest() async {
    final current = state.asData?.value;
    if (current == null || current.isEmpty) return;
    final latest = current.last.seqNo;
    await markChatRead(groupId: groupId, readSeq: latest);
  }
}

int _asInt(Object? v) {
  if (v is int) return v;
  if (v is num) return v.toInt();
  return int.tryParse(v?.toString() ?? '') ?? 0;
}

List<String> _asStringList(Object? v) {
  if (v is! List) return const [];
  return [
    for (final e in v)
      if (e is String && e.isNotEmpty) e,
  ];
}

/// 群成员（含 inactive，可 @）。失败时空列表。
final chatMembersProvider =
    FutureProvider.family<List<ChatMember>, String>((ref, groupId) async {
  try {
    return (await chatGroupMembers(groupId: groupId)).items;
  } catch (_) {
    return const [];
  }
});
