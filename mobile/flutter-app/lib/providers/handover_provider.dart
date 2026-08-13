import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/constants.dart';
import '../bridge/api/handover.dart';

/// 交接班列表。
final handoversProvider =
    AsyncNotifierProvider<HandoversNotifier, List<Handover>>(
  HandoversNotifier.new,
);

class HandoversNotifier extends AsyncNotifier<List<Handover>> {
  @override
  Future<List<Handover>> build() => _load();

  Future<List<Handover>> _load() => shiftHandovers(
        status: null,
        limit: AppConstants.handoverPageSize,
        offset: 0,
      );

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(_load);
  }
}

/// 交接班详情（family create 接收 handoverId）。
final handoverDetailProvider = AsyncNotifierProvider.family<
    HandoverDetailNotifier, Handover, String>(HandoverDetailNotifier.new);

class HandoverDetailNotifier extends AsyncNotifier<Handover> {
  HandoverDetailNotifier(this.handoverId);
  final String handoverId;

  @override
  Future<Handover> build() => shiftHandoverDetail(id: handoverId);

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(
      () => shiftHandoverDetail(id: handoverId),
    );
  }

  Future<void> ackItem(String itemId) async {
    await ackHandoverItem(
      handoverId: handoverId,
      itemId: itemId,
      acknowledged: true,
    );
    await refresh();
  }

  Future<void> ackWhole() async {
    await ackHandover(handoverId: handoverId);
    await refresh();
  }
}
