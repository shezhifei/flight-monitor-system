import { PAGE_ROUTES, pageUrl, type PageKey } from './page-routes';

/** 可在工作区标签中打开的功能模块（对齐 Dashboard 能力面） */
export type WorkspaceModuleId = Exclude<PageKey, 'login' | 'dashboard' | 'workspace'>;

export interface WorkspaceModuleDef {
  id: WorkspaceModuleId;
  title: string;
  shortTitle: string;
  description: string;
  icon: string;
  /** 固定标签：不可关闭，默认打开 */
  pinned?: boolean;
}

/**
 * 顶栏功能列表（横向展示）。
 * 顺序按运行值班常用度排列。
 */
export const WORKSPACE_MODULES: WorkspaceModuleDef[] = [
  {
    id: 'flight_monitor',
    title: '航班监控',
    shortTitle: '航班',
    description: '实时航班动态',
    icon: 'plane',
    pinned: true,
  },
  {
    id: 'dispatch_board',
    title: '派工调度',
    shortTitle: '派工',
    description: '甘特图派工与重排',
    icon: 'bar_chart',
  },
  {
    id: 'anomaly_monitor',
    title: '异常监控',
    shortTitle: '异常',
    description: '异常检测与处置',
    icon: 'activity',
  },
  {
    id: 'command_center',
    title: '指挥中心',
    shortTitle: '指挥',
    description: '运行指挥与态势',
    icon: 'target',
  },
  {
    id: 'kpi_dashboard',
    title: 'KPI 诊断',
    shortTitle: 'KPI',
    description: '性能指标分析',
    icon: 'bar_chart',
  },
  {
    id: 'operations_review_report',
    title: '交接复盘',
    shortTitle: '交接',
    description: '运行交接与班后复盘',
    icon: 'messages',
  },
  {
    id: 'resource_manager',
    title: '资源管理',
    shortTitle: '资源',
    description: '人员设备调度',
    icon: 'users',
  },
  {
    id: 'resource_utilization',
    title: '资源利用率',
    shortTitle: '利用率',
    description: '班组与设备利用率',
    icon: 'bar_chart',
  },
  {
    id: 'dispatch_rule_center',
    title: '规则与标签',
    shortTitle: '规则',
    description: '派工规则与航班标签',
    icon: 'settings',
  },
  {
    id: 'flight_imports',
    title: '航班导入',
    shortTitle: '导入',
    description: '计划导入与校验',
    icon: 'upload',
  },
  {
    id: 'system_status',
    title: '系统状态',
    shortTitle: '状态',
    description: '基础设施健康',
    icon: 'connection',
  },
  {
    id: 'system_flags',
    title: '系统开关',
    shortTitle: '开关',
    description: '运行时特性开关',
    icon: 'settings',
  },
  {
    id: 'flowable_modeler',
    title: '流程建模',
    shortTitle: '流程',
    description: 'BPMN 流程编辑',
    icon: 'folder',
  },
  {
    id: 'ai_config_center',
    title: 'AI 配置',
    shortTitle: 'AI配置',
    description: '模型与工具配置',
    icon: 'ai',
  },
  {
    id: 'ai_monitor',
    title: 'AI 监控',
    shortTitle: 'AI监控',
    description: '运行与护栏事件',
    icon: 'activity',
  },
  {
    id: 'nl_query',
    title: 'NL 查询',
    shortTitle: '查询',
    description: '自然语言查数',
    icon: 'search',
  },
  {
    id: 'llm_eval_lab',
    title: '评测实验室',
    shortTitle: '评测',
    description: '模型与提示评测',
    icon: 'ai',
  },
  {
    id: 'user_manager',
    title: '用户管理',
    shortTitle: '用户',
    description: '账号与权限',
    icon: 'users',
  },
];

const MODULE_BY_ID = new Map(WORKSPACE_MODULES.map((m) => [m.id, m]));

export const WORKSPACE_MAX_TABS = 8;
export const WORKSPACE_STORAGE_KEY = 'fms-workspace-tabs-v1';
export const PINNED_MODULE_ID: WorkspaceModuleId = 'flight_monitor';

export function isWorkspaceModuleId(value: string | null | undefined): value is WorkspaceModuleId {
  if (!value) return false;
  return MODULE_BY_ID.has(value as WorkspaceModuleId);
}

export function getWorkspaceModule(id: string | null | undefined): WorkspaceModuleDef | null {
  if (!isWorkspaceModuleId(id)) return null;
  return MODULE_BY_ID.get(id) ?? null;
}

/** 嵌入工作区 iframe 的页面 URL（带 embed=1，隐藏页内顶栏） */
export function workspaceEmbedSrc(moduleId: WorkspaceModuleId): string {
  const base = PAGE_ROUTES[moduleId];
  const sep = base.includes('?') ? '&' : '?';
  return `${base}${sep}embed=1`;
}

/** Dashboard / 外链进入工作区并打开指定标签 */
export function workspaceOpenUrl(moduleId?: string | null): string {
  const base = pageUrl('workspace');
  if (!moduleId || !isWorkspaceModuleId(moduleId)) {
    return base;
  }
  return `${base}?tab=${encodeURIComponent(moduleId)}`;
}

/**
 * 将 Dashboard module 别名映射到工作区模块 id。
 * 与 Dashboard.vue moduleRouteMap 保持一致。
 */
export function resolveWorkspaceModuleFromDashboard(moduleName?: string | null): WorkspaceModuleId | null {
  const key = String(moduleName ?? '').trim().toLowerCase();
  if (!key) return null;

  const alias: Record<string, WorkspaceModuleId> = {
    anomaly: 'anomaly_monitor',
    anomaly_monitor: 'anomaly_monitor',
    anomalies: 'anomaly_monitor',
    dispatch: 'dispatch_board',
    dispatch_board: 'dispatch_board',
    flight: 'flight_monitor',
    flight_monitor: 'flight_monitor',
    handover: 'operations_review_report',
    operations_review: 'operations_review_report',
    kpi: 'kpi_dashboard',
    kpi_dashboard: 'kpi_dashboard',
    resource: 'resource_manager',
    resource_manager: 'resource_manager',
    system: 'system_status',
    system_status: 'system_status',
    shift_handover: 'operations_review_report',
    command: 'command_center',
    command_center: 'command_center',
    // 标签管理已并入派工规则
    label: 'dispatch_rule_center',
    labels: 'dispatch_rule_center',
    label_manager: 'dispatch_rule_center',
  };

  if (alias[key]) return alias[key];
  if (isWorkspaceModuleId(key)) return key;
  return null;
}
