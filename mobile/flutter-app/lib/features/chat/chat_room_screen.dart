import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../app/l10n.dart';
import '../../bridge/api/chat.dart';
import '../../providers/chat_provider.dart';
import '../../providers/sse_demux.dart';
import '../../providers/workbench_provider.dart';
import '../../shared/widgets/snackbar.dart';
import 'chat_mention.dart';

/// 聊天室：分页、发送、@ 提及、read_only、系统消息。
class ChatRoomScreen extends ConsumerStatefulWidget {
  const ChatRoomScreen({
    super.key,
    required this.groupId,
    this.groupName,
  });

  final String groupId;
  final String? groupName;

  @override
  ConsumerState<ChatRoomScreen> createState() => _ChatRoomScreenState();
}

class _ChatRoomScreenState extends ConsumerState<ChatRoomScreen> {
  final _input = TextEditingController();
  final _focus = FocusNode();
  final _scroll = ScrollController();
  final _mentionIds = <String>{};
  var _atAll = false;
  var _sending = false;
  var _readOnly = false;
  ({int atIndex, String query})? _trigger;
  var _cursor = 0;

  @override
  void initState() {
    super.initState();
    _input.addListener(_onInput);
    // 标记活跃室供 SSE demux fanout；退出时清空。
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(activeChatRoomIdProvider.notifier).set(widget.groupId);
      // 群列表里取 read_only 标志。
      final groups = ref.read(chatGroupsProvider).asData?.value;
      final g = groups?.items
          .where((x) => x.groupId == widget.groupId)
          .firstOrNull;
      if (g != null && mounted) setState(() => _readOnly = g.readOnly);
    });
  }

  @override
  void dispose() {
    _input.removeListener(_onInput);
    _input.dispose();
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  void _onInput() {
    if (!mounted) return;
    final text = _input.text;
    final cursor = _input.selection.isValid
        ? _input.selection.baseOffset.clamp(0, text.length)
        : text.length;
    final trigger = mentionTrigger(text, cursor);
    final clearMentions = text.isEmpty && (_atAll || _mentionIds.isNotEmpty);
    if (trigger?.atIndex == _trigger?.atIndex &&
        trigger?.query == _trigger?.query &&
        cursor == _cursor &&
        !clearMentions) {
      return;
    }
    setState(() {
      _cursor = cursor;
      _trigger = trigger;
      if (text.isEmpty) {
        _atAll = false;
        _mentionIds.clear();
      }
    });
  }

  void _selectCandidate(MentionCandidate candidate) {
    final trigger = _trigger;
    if (trigger == null) return;
    final newText = insertMention(
      text: _input.text,
      atIndex: trigger.atIndex,
      cursor: _cursor,
      username: candidate.username,
    );
    final newCursor = trigger.atIndex + '@${candidate.username} '.length;
    setState(() {
      if (candidate.userId == mentionAllId) {
        _atAll = true;
      } else {
        _mentionIds.add(candidate.userId);
      }
      _trigger = null;
    });
    _input.value = TextEditingValue(
      text: newText,
      selection: TextSelection.collapsed(offset: newCursor),
    );
    _focus.requestFocus();
  }

  Future<void> _send() async {
    final text = _input.text.trim();
    if (text.isEmpty || _sending || _readOnly) return;
    setState(() => _sending = true);
    try {
      await ref.read(chatRoomProvider(widget.groupId).notifier).send(
            text,
            atAll: _atAll,
            mentionUserIds: [
              for (final id in _mentionIds)
                if (id != mentionAllId) id,
            ],
          );
      _input.clear();
      setState(() {
        _atAll = false;
        _mentionIds.clear();
        _trigger = null;
      });
      await Future<void>.delayed(const Duration(milliseconds: 50));
      if (_scroll.hasClients) {
        _scroll.jumpTo(_scroll.position.maxScrollExtent);
      }
    } catch (e) {
      if (mounted) showErrorSnackBar(context, e);
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    ref.watch(sseDemuxProvider);
    final messages = ref.watch(chatRoomProvider(widget.groupId));
    final currentUserId = ref.watch(workbenchProvider).asData?.value.userId;
    final members =
        ref.watch(chatMembersProvider(widget.groupId)).asData?.value ??
            const <ChatMember>[];
    final trigger = _trigger;
    final candidates = trigger == null
        ? const <MentionCandidate>[]
        : filterMentionCandidates(
            members: [
              for (final m in members)
                MentionCandidate(userId: m.userId, username: m.username),
            ],
            query: trigger.query,
            includeAll: true,
          );
    // 离开时清 active id。
    return PopScope(
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) {
          ref.read(activeChatRoomIdProvider.notifier).set(null);
          ref
              .read(chatRoomProvider(widget.groupId).notifier)
              .markReadUpToLatest()
              .catchError((_) {});
          ref.read(chatGroupsProvider.notifier).softRefresh().catchError((_) {});
        }
      },
      child: Scaffold(
        appBar: AppBar(
          title: Text(widget.groupName ?? S.chatRoomTitle),
          actions: [
            if (_readOnly)
              const Padding(
                padding: EdgeInsets.only(right: 12),
                child: Center(child: Text(S.chatReadOnly)),
              ),
          ],
        ),
        body: Column(
          children: [
            Expanded(
              child: messages.when(
                loading: () =>
                    const Center(child: CircularProgressIndicator()),
                error: (e, _) => Center(child: Text('${S.errorPrefix}$e')),
                data: (items) => _MessageList(
                  items: items,
                  controller: _scroll,
                  currentUserId: currentUserId,
                  onLoadMore: () => ref
                      .read(chatRoomProvider(widget.groupId).notifier)
                      .loadMore(),
                ),
              ),
            ),
            if (!_readOnly)
              _Composer(
                controller: _input,
                focusNode: _focus,
                sending: _sending,
                candidates: candidates,
                members: members,
                showPicker: _trigger != null && candidates.isNotEmpty,
                onSelectCandidate: _selectCandidate,
                onSend: _send,
              )
            else
              Material(
                color: Theme.of(context).colorScheme.surfaceContainerHighest,
                child: const SafeArea(
                  child: Padding(
                    padding: EdgeInsets.all(12),
                    child: Text(S.chatReadOnlyHint),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _MessageList extends StatelessWidget {
  const _MessageList({
    required this.items,
    required this.controller,
    required this.currentUserId,
    required this.onLoadMore,
  });

  final List<ChatMessage> items;
  final ScrollController controller;
  final String? currentUserId;
  final Future<bool> Function() onLoadMore;

  @override
  Widget build(BuildContext context) {
    if (items.isEmpty) {
      return const Center(child: Text(S.chatNoMessages));
    }
    return NotificationListener<ScrollNotification>(
      onNotification: (n) {
        if (n.metrics.pixels <= 40 && n is ScrollUpdateNotification) {
          onLoadMore();
        }
        return false;
      },
      child: ListView.builder(
        controller: controller,
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        itemCount: items.length,
        itemBuilder: (context, i) => _Bubble(
          message: items[i],
          currentUserId: currentUserId,
        ),
      ),
    );
  }
}

class _Bubble extends StatelessWidget {
  const _Bubble({required this.message, required this.currentUserId});
  final ChatMessage message;
  final String? currentUserId;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final isSystem = message.messageType == 'system' ||
        message.senderUserId == null ||
        message.senderUserId!.isEmpty;
    if (isSystem && message.messageType == 'system') {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 6),
        child: Center(
          child: Text(
            message.content,
            style: Theme.of(context).textTheme.labelSmall?.copyWith(
                  color: scheme.onSurfaceVariant,
                ),
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    final sender = message.senderUsername ?? message.senderUserId ?? '?';
    final isOwn =
        currentUserId != null && message.senderUserId == currentUserId;
    final mentioned = message.isAtAll ||
        (currentUserId != null &&
            message.mentionUserIds.contains(currentUserId));
    final highlight = mentioned && !isOwn;
    return Align(
      alignment: Alignment.centerLeft,
      child: Card(
        margin: const EdgeInsets.symmetric(vertical: 4),
        color: highlight
            ? scheme.primaryContainer
            : scheme.surfaceContainerHighest,
        child: Padding(
          padding: const EdgeInsets.all(10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(sender,
                      style: Theme.of(context).textTheme.labelMedium),
                  if (message.isAtAll) ...[
                    const SizedBox(width: 6),
                    Chip(
                      label: const Text('@${S.chatMentionAll}',
                          style: TextStyle(fontSize: 10)),
                      visualDensity: VisualDensity.compact,
                      padding: EdgeInsets.zero,
                      labelPadding: const EdgeInsets.symmetric(horizontal: 4),
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 4),
              Text.rich(
                TextSpan(
                  children: [
                    for (final seg in splitChatMentions(message.content))
                      TextSpan(
                        text: seg.text,
                        style: seg.mention
                            ? TextStyle(
                                fontWeight: FontWeight.w600,
                                color: scheme.primary,
                              )
                            : null,
                      ),
                  ],
                ),
              ),
              const SizedBox(height: 2),
              Text(
                message.sentAt,
                style: Theme.of(context).textTheme.labelSmall,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Composer extends StatelessWidget {
  const _Composer({
    required this.controller,
    required this.focusNode,
    required this.sending,
    required this.candidates,
    required this.members,
    required this.showPicker,
    required this.onSelectCandidate,
    required this.onSend,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final bool sending;
  final List<MentionCandidate> candidates;
  final List<ChatMember> members;
  final bool showPicker;
  final ValueChanged<MentionCandidate> onSelectCandidate;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Material(
        elevation: 2,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(8, 6, 8, 6),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (showPicker)
                ConstrainedBox(
                  constraints: const BoxConstraints(maxHeight: 180),
                  child: ListView.builder(
                    shrinkWrap: true,
                    itemCount: candidates.length,
                    itemBuilder: (context, i) {
                      final c = candidates[i];
                      final member = members
                          .where((m) => m.userId == c.userId)
                          .firstOrNull;
                      return ListTile(
                        dense: true,
                        visualDensity: VisualDensity.compact,
                        title: Text(c.username),
                        trailing: _roleLabel(context, member),
                        onTap: () => onSelectCandidate(c),
                      );
                    },
                  ),
                ),
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      controller: controller,
                      focusNode: focusNode,
                      minLines: 1,
                      maxLines: 4,
                      decoration: const InputDecoration(
                        hintText: S.chatInputHint,
                        border: OutlineInputBorder(),
                        isDense: true,
                      ),
                      onSubmitted: (_) => onSend(),
                    ),
                  ),
                  const SizedBox(width: 8),
                  IconButton.filled(
                    onPressed: sending ? null : onSend,
                    icon: sending
                        ? const SizedBox(
                            width: 18,
                            height: 18,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Icon(Icons.send),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget? _roleLabel(BuildContext context, ChatMember? member) {
    if (member == null) return null;
    final labels = [
      if (member.isDispatcher) S.chatRoleDispatcher,
      if (member.isAssignee) S.chatRoleAssignee,
    ];
    if (labels.isEmpty) return null;
    return Text(
      labels.join(' '),
      style: Theme.of(context).textTheme.labelSmall,
    );
  }
}
