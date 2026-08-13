import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/l10n.dart';
import '../../bridge/api/chat.dart';
import '../../providers/chat_provider.dart';
import '../../providers/sse_demux.dart';

/// 聊天群列表。
class ChatGroupsScreen extends ConsumerWidget {
  const ChatGroupsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    ref.watch(sseDemuxProvider);
    final groups = ref.watch(chatGroupsProvider);

    return Scaffold(
      appBar: AppBar(title: const Text(S.chatTitle)),
      body: RefreshIndicator(
        onRefresh: () => ref.read(chatGroupsProvider.notifier).refresh(),
        child: groups.when(
          loading: () => const Center(child: CircularProgressIndicator()),
          error: (e, _) => ListView(
            children: [
              const SizedBox(height: 120),
              Center(child: Text('${S.errorPrefix}$e')),
              Center(
                child: FilledButton(
                  onPressed: () =>
                      ref.read(chatGroupsProvider.notifier).refresh(),
                  child: const Text(S.retry),
                ),
              ),
            ],
          ),
          data: (list) {
            if (list.items.isEmpty) {
              return ListView(
                children: const [
                  SizedBox(height: 120),
                  Center(child: Text(S.chatEmpty)),
                ],
              );
            }
            return ListView.separated(
              itemCount: list.items.length,
              separatorBuilder: (_, _) => const Divider(height: 1),
              itemBuilder: (context, i) => _GroupTile(group: list.items[i]),
            );
          },
        ),
      ),
    );
  }
}

class _GroupTile extends StatelessWidget {
  const _GroupTile({required this.group});
  final ChatGroup group;

  @override
  Widget build(BuildContext context) {
    final unread = group.unreadCount.toInt();
    return ListTile(
      leading: CircleAvatar(
        child: Text(
          group.groupName.isNotEmpty ? group.groupName.characters.first : '?',
        ),
      ),
      title: Text(group.groupName),
      subtitle: Text(
        group.lastMessagePreview ?? group.flightId,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          if (group.readOnly)
            Text(S.chatReadOnly,
                style: Theme.of(context).textTheme.labelSmall),
          if (unread > 0)
            Badge(
              label: Text('$unread'),
              child: const Icon(Icons.chat_bubble_outline),
            )
          else
            const Icon(Icons.chevron_right),
        ],
      ),
      onTap: () => context.push(
        '/chat/${group.groupId}',
        extra: group.groupName,
      ),
    );
  }
}
