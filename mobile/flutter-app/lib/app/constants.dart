import 'package:flutter/foundation.dart';

/// 集中常量（plan §5：心跳 60s、limit 50/120 等集中定义）。
class AppConstants {
  AppConstants._();

  /// debug 默认后端地址（宿主实测端口 8000，非计划文档里的 5000）。
  static const String debugBaseUrl = 'http://10.0.2.2:8000';

  /// release 从 `--dart-define=API_BASE_URL=...` 注入，强制 https。
  static const String releaseBaseUrl = String.fromEnvironment('API_BASE_URL');

  /// 心跳循环间隔（auth/device 心跳 + 离线补传）。
  static const Duration heartbeatInterval = Duration(seconds: 60);

  /// workbench 工单上限（后端 1..=200，默认 50）。
  static const int workbenchMaxOrders = 50;

  /// operations 事件流 limit（plan §5 OperationsScreen）。
  static const int operationsEventsLimit = 120;

  /// 战情中心轮询间隔（plan §6 P3：45s）。
  static const Duration operationsPollInterval = Duration(seconds: 45);

  /// 派工列表无上限分页（my/assigned 一次拉全）。
  static const int dispatchListPageSize = 50;

  /// P2 列表分页（对照旧 App limit 50）。
  static const int chatListPageSize = 50;
  static const int chatMessagePageSize = 50;
  static const int notificationPageSize = 50;
  static const int handoverPageSize = 50;

  /// 上传附件的 multipart category（§3.3）。
  static const String uploadCategoryDispatchIssue = 'dispatch_issue';

  /// 离线队列 sqlite 文件名（位于 ApplicationSupportDirectory）。
  static const String offlineDbFileName = 'fms_offline.db';

  /// secure storage keys。
  static const String storageKeyTokenBundle = 'fms.token_bundle';
  static const String storageKeyBaseUrlOverride = 'fms.base_url_override';
  static const String storageKeyDeviceIdFallback = 'fms.device_id_fallback';

  /// 当前生效的 base_url：release 优先 dart-define 且必须 https；
  /// debug 允许 secure storage 覆盖（设置页可改），否则 10.0.2.2 默认值。
  static String baseUrl({String? debugOverride}) {
    if (kReleaseMode) {
      assert(
        releaseBaseUrl.startsWith('https://'),
        'release API_BASE_URL must be https',
      );
      return releaseBaseUrl;
    }
    return debugOverride ?? debugBaseUrl;
  }

  /// debug / 模拟器走明文 http（debug manifest 已允许明文）。
  static bool get allowCleartext => !kReleaseMode;
}
