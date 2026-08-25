const mentionAllId = '@all';
const mentionAllLabel = '全体';

class MentionCandidate {
  const MentionCandidate({required this.userId, required this.username});

  final String userId;
  final String username;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is MentionCandidate &&
          userId == other.userId &&
          username == other.username;

  @override
  int get hashCode => Object.hash(userId, username);
}

class MentionSegment {
  const MentionSegment({required this.text, required this.mention});

  final String text;
  final bool mention;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is MentionSegment && text == other.text && mention == other.mention;

  @override
  int get hashCode => Object.hash(text, mention);
}

/// 光标前最近一个独立 `@`（行首/空格/换行后）；query 不含空格或换行。
({int atIndex, String query})? mentionTrigger(String text, int cursor) {
  final pos = cursor.clamp(0, text.length);
  final before = text.substring(0, pos);
  final atIndex = before.lastIndexOf('@');
  if (atIndex < 0) return null;
  if (atIndex > 0) {
    final prev = before[atIndex - 1];
    if (prev != ' ' && prev != '\n') return null;
  }
  final query = before.substring(atIndex + 1);
  if (query.contains(' ') || query.contains('\n')) return null;
  return (atIndex: atIndex, query: query);
}

bool _matchesQuery(String username, String userId, String query) {
  if (query.isEmpty) return true;
  final q = query.toLowerCase();
  return username.toLowerCase().contains(q) || userId.toLowerCase().contains(q);
}

bool _queryMatchesAll(String query) {
  if (query.isEmpty) return true;
  final q = query.toLowerCase();
  return mentionAllLabel.contains(query) ||
      q == 'all' ||
      q == mentionAllId.toLowerCase();
}

/// 按用户名 / userId 过滤；`includeAll` 时符合 empty/全体/all 则把 全体 放在最前。
/// `@all` 只作为 全体 哨兵，不会当成成员 id 进入结果。
List<MentionCandidate> filterMentionCandidates({
  required List<MentionCandidate> members,
  required String query,
  required bool includeAll,
}) {
  final people = [
    for (final m in members)
      if (m.userId != mentionAllId &&
          _matchesQuery(m.username, m.userId, query))
        m,
  ];
  if (includeAll && _queryMatchesAll(query)) {
    return [
      const MentionCandidate(userId: mentionAllId, username: mentionAllLabel),
      ...people,
    ];
  }
  return people;
}

/// 用 `@$username ` 替换 `[atIndex, cursor)`。
String insertMention({
  required String text,
  required int atIndex,
  required int cursor,
  required String username,
}) {
  final start = atIndex.clamp(0, text.length);
  final end = cursor.clamp(start, text.length);
  return '${text.substring(0, start)}@$username ${text.substring(end)}';
}

final _mentionToken = RegExp(
  r'@全体|@all(?=$|[\s@])|@[^\s@]+',
  caseSensitive: false,
);

/// 拆 `@全体` / `@all`（任意大小写）/ `@`+非空白非 `@`。
List<MentionSegment> splitChatMentions(String content) {
  if (content.isEmpty) return const [];
  final segments = <MentionSegment>[];
  var last = 0;
  for (final match in _mentionToken.allMatches(content)) {
    if (match.start > last) {
      segments.add(
        MentionSegment(text: content.substring(last, match.start), mention: false),
      );
    }
    segments.add(MentionSegment(text: match.group(0)!, mention: true));
    last = match.end;
  }
  if (last < content.length) {
    segments.add(MentionSegment(text: content.substring(last), mention: false));
  }
  return segments;
}
