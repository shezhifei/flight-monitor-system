import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api/business_case.dart';

final businessCasesProvider =
    AsyncNotifierProvider<BusinessCasesNotifier, List<BusinessCase>>(
  BusinessCasesNotifier.new,
);

class BusinessCasesNotifier extends AsyncNotifier<List<BusinessCase>> {
  @override
  Future<List<BusinessCase>> build() => businessCases();

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(() => businessCases());
  }
}

final businessCaseDetailProvider = AsyncNotifierProvider.family<
    BusinessCaseDetailNotifier, BusinessCase, String>(
  BusinessCaseDetailNotifier.new,
);

class BusinessCaseDetailNotifier extends AsyncNotifier<BusinessCase> {
  BusinessCaseDetailNotifier(this.caseId);
  final String caseId;

  @override
  Future<BusinessCase> build() => businessCaseDetail(id: caseId);

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(() => businessCaseDetail(id: caseId));
  }

  Future<void> append(String content) async {
    await appendBusinessCase(caseId: caseId, content: content);
    await refresh();
  }

  Future<void> acknowledgeAppend(String appendId) async {
    await ackAppend(caseId: caseId, appendId: appendId);
    await refresh();
  }
}

final businessCaseTypesProvider =
    FutureProvider<List<BusinessCaseType>>((ref) => businessCaseTypes(activeOnly: true));
