import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/constants.dart';
import '../bridge/api/operations.dart';

final operationsFeedProvider =
    AsyncNotifierProvider<OperationsFeedNotifier, OperationsFeed>(
  OperationsFeedNotifier.new,
);

class OperationsFeedNotifier extends AsyncNotifier<OperationsFeed> {
  @override
  Future<OperationsFeed> build() =>
      operationsEvents(limit: AppConstants.operationsEventsLimit);

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(
      () => operationsEvents(limit: AppConstants.operationsEventsLimit),
    );
  }
}
