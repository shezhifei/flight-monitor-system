import 'package:flutter/material.dart';

import '../../app/l10n.dart';

/// 统一 Snackbar 组件（plan §7.2：Dart 侧错误展示统一走这里）。
void showAppSnackBar(
  BuildContext context,
  String message, {
  bool isError = false,
}) {
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(
      SnackBar(
        content: Text(message),
        backgroundColor:
            isError ? Theme.of(context).colorScheme.error : null,
        behavior: SnackBarBehavior.floating,
      ),
    );
}

/// 错误反馈快捷入口（自动加"错误："前缀）。
void showErrorSnackBar(BuildContext context, Object error) {
  showAppSnackBar(context, '${S.errorPrefix}$error', isError: true);
}
