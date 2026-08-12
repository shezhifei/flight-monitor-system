import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../app/l10n.dart';
import '../bridge/api/session.dart';
import '../features/auth/login_screen.dart';
import '../features/business_case/business_case_detail_screen.dart';
import '../features/business_case/business_case_editor_screen.dart';
import '../features/business_case/business_case_list_screen.dart';
import '../features/chat/chat_groups_screen.dart';
import '../features/chat/chat_room_screen.dart';
import '../features/dispatch/dispatch_screen.dart';
import '../features/dispatch/safety_checklist_screen.dart';
import '../features/handover/handover_detail_screen.dart';
import '../features/handover/handover_list_screen.dart';
import '../features/notifications/notification_detail_screen.dart';
import '../features/notifications/notifications_screen.dart';
import '../features/notifications/receipt_group_screen.dart';
import '../features/operations/operations_screen.dart';
import '../features/settings/settings_screen.dart';
import '../features/workbench/workbench_screen.dart';
import '../providers/session_provider.dart';

/// 登录守卫重定向（纯函数，widget 测试直接覆盖）：
/// 匿名 → /login；已登录访问 /login → /workbench。
String? guardRedirect(bool loggedIn, String location) {
  final onLogin = location == '/login';
  if (!loggedIn && !onLogin) return '/login';
  if (loggedIn && onLogin) return '/workbench';
  return null;
}

final routerProvider = Provider<GoRouter>((ref) {
  final refresh = ValueNotifier<int>(0);
  ref.onDispose(refresh.dispose);
  // Rust session_state 流 → 登录态标志 + 触发守卫重算。
  ref.listen(sessionStateStreamProvider, (_, next) {
    final active = next.value is SessionState_Active;
    if (ref.read(loggedInProvider) != active) {
      ref.read(loggedInProvider.notifier).set(active);
      refresh.value++;
    }
  });

  return GoRouter(
    initialLocation: '/workbench',
    refreshListenable: refresh,
    redirect: (context, state) =>
        guardRedirect(ref.read(loggedInProvider), state.uri.path),
    routes: [
      GoRoute(path: '/login', builder: (context, state) => const LoginScreen()),
      StatefulShellRoute.indexedStack(
        builder: (context, state, shell) => HomeShell(shell: shell),
        branches: [
          StatefulShellBranch(routes: [
            GoRoute(
                path: '/workbench',
                builder: (context, state) => const WorkbenchScreen()),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(
                path: '/dispatch',
                builder: (context, state) => const DispatchScreen(),
                routes: [
                  GoRoute(
                    path: ':orderId/checklist',
                    builder: (_, state) => SafetyChecklistScreen(
                      orderId: state.pathParameters['orderId']!,
                    ),
                  ),
                ]),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(
              path: '/chat',
              builder: (context, state) => const ChatGroupsScreen(),
              routes: [
                GoRoute(
                  path: ':groupId',
                  builder: (context, state) => ChatRoomScreen(
                    groupId: state.pathParameters['groupId']!,
                    groupName: state.extra as String?,
                  ),
                ),
              ],
            ),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(
              path: '/notifications',
              builder: (context, state) => const NotificationsScreen(),
              routes: [
                GoRoute(
                  path: 'receipt-groups/:receiptGroupId',
                  builder: (_, state) => ReceiptGroupScreen(
                    receiptGroupId:
                        state.pathParameters['receiptGroupId']!,
                  ),
                ),
                GoRoute(
                  path: ':notificationId',
                  builder: (_, state) => NotificationDetailScreen(
                    notificationId:
                        state.pathParameters['notificationId']!,
                  ),
                ),
              ],
            ),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(
                path: '/settings',
                builder: (context, state) => const SettingsScreen()),
          ]),
        ],
      ),
      // 次级入口挂在 shell 外（从工作台进入，避免底部导航过载）。
      GoRoute(
        path: '/handover',
        builder: (context, state) => const HandoverListScreen(),
        routes: [
          GoRoute(
            path: ':handoverId',
            builder: (_, state) => HandoverDetailScreen(
              handoverId: state.pathParameters['handoverId']!,
            ),
          ),
        ],
      ),
      GoRoute(
        path: '/business-cases',
        builder: (context, state) => const BusinessCaseListScreen(),
        routes: [
          GoRoute(
            path: 'new',
            builder: (context, state) => const BusinessCaseEditorScreen(),
          ),
          GoRoute(
            path: ':caseId',
            builder: (_, state) => BusinessCaseDetailScreen(
              caseId: state.pathParameters['caseId']!,
            ),
          ),
        ],
      ),
      GoRoute(
        path: '/operations',
        builder: (context, state) => const OperationsScreen(),
      ),
    ],
  );
});

/// 底部导航壳：工作台 / 派工 / 消息 / 通知 / 设置。
class HomeShell extends StatelessWidget {
  const HomeShell({super.key, required this.shell});

  final StatefulNavigationShell shell;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: shell,
      bottomNavigationBar: NavigationBar(
        selectedIndex: shell.currentIndex,
        onDestinationSelected: (i) => shell.goBranch(
          i,
          initialLocation: i == shell.currentIndex,
        ),
        destinations: const [
          NavigationDestination(
              icon: Icon(Icons.dashboard_outlined), label: S.navWorkbench),
          NavigationDestination(
              icon: Icon(Icons.assignment_outlined), label: S.navDispatch),
          NavigationDestination(
              icon: Icon(Icons.chat_bubble_outline), label: S.navChat),
          NavigationDestination(
              icon: Icon(Icons.notifications_outlined),
              label: S.navNotifications),
          NavigationDestination(
              icon: Icon(Icons.settings_outlined), label: S.navSettings),
        ],
      ),
    );
  }
}
