import 'package:flutter_test/flutter_test.dart';
import 'package:flight_monitor/realtime/sse_payload.dart';

void main() {
  group('parseSseChatMessage', () {
    test('unwraps nested message object', () {
      const raw =
          '{"type":"dispatch_chat_message","group_id":"g1","message":{"message_id":"m1","seq_no":7,"group_id":"g1","content":"hi"}}';
      final got = parseSseChatMessage(raw);
      expect(got, isNotNull);
      expect(got!.messageId, 'm1');
      expect(got.seqNo, 7);
      expect(got.groupId, 'g1');
      expect(got.content, 'hi');
    });

    test('rejects payload without message_id', () {
      expect(parseSseChatMessage('{"content":"x"}'), isNull);
    });
  });

  group('parseSseNotification', () {
    test('unwraps nested notification object', () {
      const raw =
          '{"notification":{"notification_id":"n1","title":"t","body":"b","receipt_group_id":"rg"}}';
      final got = parseSseNotification(raw);
      expect(got!.notificationId, 'n1');
      expect(got.receiptGroupId, 'rg');
    });
  });

  group('upsertUnique seq 去重', () {
    test('same seq is dropped; order stays sorted', () {
      final seen = <int>{1};
      final first = upsertUnique([1], 2, 2, seen, compare: (a, b) => a.compareTo(b));
      expect(first, [1, 2]);
      final again = upsertUnique(first, 2, 2, seen, compare: (a, b) => a.compareTo(b));
      expect(again, [1, 2]);
    });
  });
}
