import 'package:flight_monitor/features/chat/chat_mention.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('mentionTrigger', () {
    test('@ at start opens; a@b does not; @李 query is 李', () {
      expect(mentionTrigger('@', 1), (atIndex: 0, query: ''));
      expect(mentionTrigger('a@b', 3), isNull);
      expect(mentionTrigger('@李', 2), (atIndex: 0, query: '李'));
    });

    test('space or newline before @ opens; space in query closes', () {
      expect(mentionTrigger('收到 @', 4), (atIndex: 3, query: ''));
      expect(mentionTrigger('x\n@ab', 5), (atIndex: 2, query: 'ab'));
      expect(mentionTrigger('@foo bar', 8), isNull);
      expect(mentionTrigger('', 0), isNull);
    });
  });

  group('filterMentionCandidates', () {
    const zhang = MentionCandidate(userId: 'u1', username: '张三');
    const li = MentionCandidate(userId: 'u2', username: '李四');
    const allMember = MentionCandidate(userId: mentionAllId, username: 'hacker');

    test('filter 全体 first when includeAll; @all is not a member id', () {
      final got = filterMentionCandidates(
        members: const [zhang, li, allMember],
        query: '',
        includeAll: true,
      );
      expect(got.first, const MentionCandidate(
        userId: mentionAllId,
        username: mentionAllLabel,
      ));
      expect(got.where((c) => c.userId == mentionAllId), hasLength(1));
      expect(got.any((c) => c.username == 'hacker'), isFalse);
      expect(got.map((c) => c.userId).toList(), [mentionAllId, 'u1', 'u2']);
    });

    test('全体 matches empty / 全体 / all; members filter by username or id', () {
      expect(
        filterMentionCandidates(
          members: const [zhang, li],
          query: '全体',
          includeAll: true,
        ).first.username,
        mentionAllLabel,
      );
      expect(
        filterMentionCandidates(
          members: const [zhang, li],
          query: 'ALL',
          includeAll: true,
        ).first.userId,
        mentionAllId,
      );
      expect(
        filterMentionCandidates(
          members: const [zhang, li],
          query: '李',
          includeAll: true,
        ).map((c) => c.userId).toList(),
        ['u2'],
      );
      expect(
        filterMentionCandidates(
          members: const [zhang, li],
          query: 'U1',
          includeAll: false,
        ),
        const [zhang],
      );
    });

    test('without includeAll, 全体 is not injected', () {
      final got = filterMentionCandidates(
        members: const [zhang, li],
        query: '',
        includeAll: false,
      );
      expect(got, const [zhang, li]);
      expect(got.any((c) => c.userId == mentionAllId), isFalse);
    });
  });

  group('insertMention', () {
    test('inserts @张三 ', () {
      expect(
        insertMention(text: '@', atIndex: 0, cursor: 1, username: '张三'),
        '@张三 ',
      );
      expect(
        insertMention(text: '收到 @', atIndex: 3, cursor: 4, username: '张三'),
        '收到 @张三 ',
      );
    });
  });

  group('splitChatMentions', () {
    test('split 请支援 @张三 @全体', () {
      expect(
        splitChatMentions('请支援 @张三 @全体'),
        const [
          MentionSegment(text: '请支援 ', mention: false),
          MentionSegment(text: '@张三', mention: true),
          MentionSegment(text: ' ', mention: false),
          MentionSegment(text: '@全体', mention: true),
        ],
      );
      expect(
        splitChatMentions('hello @All there'),
        const [
          MentionSegment(text: 'hello ', mention: false),
          MentionSegment(text: '@All', mention: true),
          MentionSegment(text: ' there', mention: false),
        ],
      );
      expect(
        splitChatMentions('ping @all'),
        const [
          MentionSegment(text: 'ping ', mention: false),
          MentionSegment(text: '@all', mention: true),
        ],
      );
    });

    test('<script> stays a text segment', () {
      expect(
        splitChatMentions('<script>alert(1)</script>'),
        const [
          MentionSegment(text: '<script>alert(1)</script>', mention: false),
        ],
      );
      expect(
        splitChatMentions('<script>alert(1)</script> @李四'),
        const [
          MentionSegment(text: '<script>alert(1)</script> ', mention: false),
          MentionSegment(text: '@李四', mention: true),
        ],
      );
    });

    test('empty and plain text', () {
      expect(splitChatMentions(''), isEmpty);
      expect(
        splitChatMentions('hello world'),
        const [MentionSegment(text: 'hello world', mention: false)],
      );
    });
  });
}
