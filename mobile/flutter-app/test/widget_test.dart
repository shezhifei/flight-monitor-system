import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flight_monitor/app/l10n.dart';
import 'package:flight_monitor/app/router.dart';
import 'package:flight_monitor/features/auth/login_screen.dart';
import 'package:flight_monitor/providers/dispatch_provider.dart';

void main() {
  group('guardRedirect 登录守卫', () {
    test('匿名访问任意页 → /login', () {
      expect(guardRedirect(false, '/workbench'), '/login');
      expect(guardRedirect(false, '/dispatch'), '/login');
      expect(guardRedirect(false, '/settings'), '/login');
    });

    test('匿名访问 /login → 不重定向', () {
      expect(guardRedirect(false, '/login'), isNull);
    });

    test('已登录访问 /login → /workbench', () {
      expect(guardRedirect(true, '/login'), '/workbench');
    });

    test('已登录访问业务页 → 不重定向', () {
      expect(guardRedirect(true, '/workbench'), isNull);
      expect(guardRedirect(true, '/dispatch/abc/checklist'), isNull);
    });
  });

  group('dispatchActionsForStatus 状态机（对照旧 App）', () {
    test('assigned → 接单', () {
      expect(dispatchActionsForStatus('assigned'),
          [DispatchActionId.accept]);
    });

    test('accepted → 签到；checked_in → 开始作业', () {
      expect(dispatchActionsForStatus('accepted'),
          [DispatchActionId.checkIn]);
      expect(dispatchActionsForStatus('checked_in'),
          [DispatchActionId.start]);
    });

    test('in_progress → 上报预计/完工/签退', () {
      expect(
        dispatchActionsForStatus('in_progress'),
        [
          DispatchActionId.etaReport,
          DispatchActionId.complete,
          DispatchActionId.checkOut,
        ],
      );
    });

    test('终态无动作；问题上报/安全清单入口可见性', () {
      expect(dispatchActionsForStatus('completed'), isEmpty);
      expect(dispatchActionsForStatus('cancelled'), isEmpty);
      expect(reportIssueVisibleForStatus('completed'), isFalse);
      expect(reportIssueVisibleForStatus('in_progress'), isTrue);
      expect(safetyChecklistVisibleForStatus('in_progress'), isTrue);
      expect(safetyChecklistVisibleForStatus('assigned'), isFalse);
    });

    test('action_type 契约字符串', () {
      expect(dispatchActionType(DispatchActionId.checkIn), 'checkin');
      expect(dispatchActionType(DispatchActionId.etaReport), 'eta_report');
    });
  });

  testWidgets('登录页渲染：用户名/密码/登录按钮', (tester) async {
    // 只渲染不提交（提交才触 Rust 出口），无需初始化 RustLib。
    await tester.pumpWidget(const ProviderScope(
      child: MaterialApp(home: LoginScreen()),
    ));
    expect(find.text(S.loginUsername), findsOneWidget);
    expect(find.text(S.loginPassword), findsOneWidget);
    expect(find.text(S.loginButton), findsOneWidget);
    expect(find.byType(TextFormField), findsNWidgets(2));
  });

  testWidgets('登录页空表单校验提示', (tester) async {
    await tester.pumpWidget(const ProviderScope(
      child: MaterialApp(home: LoginScreen()),
    ));
    await tester.tap(find.text(S.loginButton));
    await tester.pump();
    expect(find.text(S.loginUsernameRequired), findsOneWidget);
    expect(find.text(S.loginPasswordRequired), findsOneWidget);
  });
}
