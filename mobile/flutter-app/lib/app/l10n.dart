/// 中文文案集中处（全部中文文案进 app/l10n；用简单 const 映射，
/// 不引入 arb 复杂度）。
class S {
  S._();

  // 通用
  static const String appTitle = '航班保障监控';
  static const String confirm = '确定';
  static const String cancel = '取消';
  static const String retry = '重试';
  static const String save = '保存';
  static const String loading = '加载中…';
  static const String errorPrefix = '错误：';
  static const String actionQueuedOffline = '网络异常，已入离线队列，恢复后自动补传';
  static const String actionSent = '操作成功';

  // 登录
  static const String loginTitle = '登录';
  static const String loginUsername = '用户名';
  static const String loginPassword = '密码';
  static const String loginButton = '登 录';
  static const String loginUsernameRequired = '请输入用户名';
  static const String loginPasswordRequired = '请输入密码';
  static const String loginFailed = '登录失败';
  static const String loginSuccess = '登录成功';

  // 工作台 / 导航
  static const String navWorkbench = '工作台';
  static const String navDispatch = '派工';
  static const String navSettings = '设置';
  static const String navChat = '消息';
  static const String navNotifications = '通知';
  static const String navHandover = '交接';
  static const String navBusinessCase = '事项';
  static const String navOperations = '战情';
  static const String workbenchMyOrders = '我的工单';
  static const String workbenchUnreadNotifications = '未读通知';
  static const String workbenchUnreadChat = '未读消息';
  static const String workbenchPendingHandover = '待交接';
  static const String workbenchPendingSync = '待同步';
  static const String sseConnected = '实时已连接';
  static const String sseConnecting = '实时连接中…';
  static const String sseDisconnected = '实时已断开';

  // 聊天
  static const String chatTitle = '派工消息';
  static const String chatRoomTitle = '聊天';
  static const String chatEmpty = '暂无聊天群';
  static const String chatNoMessages = '暂无消息';
  static const String chatInputHint = '输入消息…';
  static const String chatReadOnly = '只读';
  static const String chatReadOnlyHint = '该群已只读，无法发送消息';
  static const String chatMentionAll = '全体';
  static const String chatRoleDispatcher = '调度';
  static const String chatRoleAssignee = '责任人';

  // 通知
  static const String notificationsTitle = '通知';
  static const String notificationsEmpty = '暂无通知';
  static const String notificationsReadAll = '全部已读';
  static const String notificationsReadAllConfirm = '确定将全部通知标记为已读？';
  static const String notificationsReadAllDone = '已全部标记为已读';
  static const String notificationsMarkRead = '标记已读';
  static const String notificationsMarkedRead = '已标记已读';
  static const String notificationsAck = '确认';
  static const String notificationsReject = '拒绝';
  static const String notificationsNoteRequired = '拒绝时必须填写备注';
  static const String notificationsNoteOptional = '备注（可选）';
  static const String notificationsReceiptRequired = '需回执';
  static const String notificationsReceiptGroup = '回执组';
  static const String notificationDetailTitle = '通知详情';
  static const String notificationsSeverity = '级别';
  static const String notificationsCategory = '分类';
  static const String notificationsOrigin = '来源';
  static const String notificationsCreatedAt = '时间';
  static const String notificationsAckStatus = '回执状态';
  static const String notificationsFlight = '航班';
  static const String receiptTotal = '总计';
  static const String receiptPending = '待回执';
  static const String receiptAcked = '已确认';
  static const String receiptRejected = '已拒绝';
  static const String receiptItems = '回执明细';

  // 交接班
  static const String handoverTitle = '交接班';
  static const String handoverDetailTitle = '交接详情';
  static const String handoverEmpty = '暂无交接班';
  static const String handoverStatus = '状态';
  static const String handoverRisk = '风险';
  static const String handoverItems = '交接事项';
  static const String handoverMandatory = '必签';
  static const String handoverAckItem = '签收本项';
  static const String handoverItemAcked = '事项已签收';
  static const String handoverAckWhole = '整单签收';
  static const String handoverWholeAcked = '交接班已签收';
  static const String handoverPendingItems = '项未签';
  static const String handoverAckedAt = '签收于';

  // 派工
  static const String dispatchEmpty = '暂无派工工单';
  static const String dispatchActionAccept = '接单';
  static const String dispatchActionCheckIn = '签到';
  static const String dispatchActionCheckOut = '签退';
  static const String dispatchActionStart = '开始作业';
  static const String dispatchActionComplete = '完工';
  static const String dispatchActionEtaReport = '上报预计完成';
  static const String dispatchActionReportIssue = '上报问题';
  static const String dispatchSafetyChecklist = '安全检查清单';
  static const String dispatchNoteHint = '备注（可选）';
  static const String etaDialogTitle = '预计完成时间';
  static const String etaDialogHint = "yyyy-MM-dd'T'HH:mm:ss'Z'（UTC）";
  static const String completeDialogTitle = '实际完工时间';
  static const String completeBlocked = '安全检查清单未通过，禁止完工';
  static const String issueDialogTitle = '上报问题';
  static const String issueTitleHint = '问题标题';
  static const String issueDescriptionHint = '问题描述（可选）';
  static const String issueAddAttachment = '添加附件';
  static const String issueAttachmentUploading = '附件上传中…';

  // 安全清单
  static const String checklistTitle = '安全检查清单';
  static const String checklistReady = '清单已通过';
  static const String checklistNotReady = '清单未通过';
  static const String checklistResultPass = '通过';
  static const String checklistResultFail = '不通过';
  static const String checklistResultNa = '不适用';
  static const String checklistEnforced = '强制执行';

  // 工单状态标签
  static const String statusPending = '待分配';
  static const String statusAssigned = '待接单';
  static const String statusAccepted = '已接单';
  static const String statusCheckedIn = '已签到';
  static const String statusInProgress = '作业中';
  static const String statusCompleted = '已完工';
  static const String statusCancelled = '已取消';

  // 设置
  static const String settingsTitle = '设置';
  static const String settingsBaseUrl = '服务器地址';
  static const String settingsBaseUrlDebugOnly = '仅 debug 可修改，保存后需重新登录';
  static const String settingsBaseUrlSaved = '已保存，请重新登录';
  static const String settingsDeviceId = '设备 ID';
  static const String settingsPendingSync = '离线待同步';
  static const String settingsSyncNow = '立即同步';
  static const String settingsLogout = '退出登录';
  static const String settingsLogoutConfirm = '确定退出登录吗？';

  // 业务事项
  static const String businessCaseTitle = '业务事项';
  static const String businessCaseDetailTitle = '事项详情';
  static const String businessCaseEmpty = '暂无业务事项';
  static const String businessCaseCreate = '新建事项';
  static const String businessCaseType = '事项类型';
  static const String businessCaseFlightId = '航班 ID';
  static const String businessCaseDescription = '描述';
  static const String businessCaseFormRequired = '请填写类型、航班与描述';
  static const String businessCaseStartWorkflow = '启动工作流';
  static const String businessCaseStartWorkflowHint = '使用模板启动流程并创建事项';
  static const String businessCaseSubmitWorkflow = '启动流程';
  static const String businessCaseSubmitCreate = '仅创建事项';
  static const String businessCaseWorkflowStarted = '工作流已启动';
  static const String businessCaseCreatedBy = '创建人';
  static const String businessCaseCreatedAt = '创建时间';
  static const String businessCaseFinishedAt = '完成时间';
  static const String businessCaseAppends = '追加记录';
  static const String businessCaseNoAppends = '暂无追加';
  static const String businessCaseAddAppend = '追加说明';
  static const String businessCaseAppendHint = '输入追加内容…';
  static const String businessCaseAckAppend = '确认追加';
  static const String businessCaseWorkflow = '查看工作流';
  static const String businessCaseNoWorkflow = '暂无关联工作流';
  static const String businessCaseWorkflowStatus = '工作流状态';

  // 战情
  static const String operationsTitle = '战情中心';
  static const String operationsEmpty = '暂无事件';
  static const String operationsFilterAll = '全部';
}
