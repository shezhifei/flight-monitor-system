import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../app/l10n.dart';
import '../../bridge/api/chat.dart';
import '../../providers/chat_provider.dart';
import '../../providers/sse_demux.dart';
import '../../shared/widgets/snackbar.dart';

/// 聊天室（plan §5 ChatRoomScreen）：分页、发送、at_all、read_only、系统消息。
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
  final _scroll = ScrollController();
  var _atAll = false;
  var _sending = false;
  var _readOnly = false;

  @override
  void initState() {
    super.initState();
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
    _input.dispose();
    _scroll.dispose();
    super.dispose();
  }

  Future<void> _send() async {
    final text = _input.text.trim();
    if (text.isEmpty || _sending || _readOnly) return;
    setState(() => _sending = true);
    try {
      await ref
          .read(chatRoomProvider(widget.groupId).notifier)
          .send(text, atAll: _atAll);
      _input.clear();
      setState(() => _atAll = false);
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
                  onLoadMore: () => ref
                      .read(chatRoomProvider(widget.groupId).notifier)
                      .loadMore(),
                ),
              ),
            ),
            if (!_readOnly) _Composer(
              controller: _input,
              atAll: _atAll,
              sending: _sending,
              onAtAllChanged: (v) => setState(() => _atAll = v),
              onSend: _send,
            ) else
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
    required this.onLoadMore,
  });

  final List<ChatMessage> items;
  final ScrollController controller;
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
        itemBuilder: (context, i) => _Bubble(message: items[i]),
      ),
    );
  }
}

class _Bubble extends StatelessWidget {
  const _Bubble({required this.message});
  final ChatMessage message;

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
    return Align(
      alignment: Alignment.centerLeft,
      child: Card(
        margin: const EdgeInsets.symmetric(vertical: 4),
        color: scheme.surfaceContainerHighest,
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
                      label: const Text('@all', style: TextStyle(fontSize: 10)),
                      visualDensity: VisualDensity.compact,
                      padding: EdgeInsets.zero,
                      labelPadding: const EdgeInsets.symmetric(horizontal: 4),
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 4),
              Text(message.content),
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
    required this.atAll,
    required this.sending,
    required this.onAtAllChanged,
    required this.onSend,
  });

  final TextEditingController controller;
  final bool atAll;
  final bool sending;
  final ValueChanged<bool> onAtAllChanged;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Material(
        elevation: 2,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(8, 6, 8, 6),
          child: Row(
            children: [
              FilterChip(
                label: const Text('@all'),
                selected: atAll,
                onSelected: onAtAllChanged,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: TextField(
                  controller: controller,
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
        ),
      ),
    );
  }
}
