<script setup lang="ts">
import { onMounted, ref, computed } from 'vue';
import { useApi } from '@/composables/useApi';
import { pageUrl, type PageKey } from '@/shared/page-routes';
import {
  resolveWorkspaceModuleFromDashboard,
  workspaceOpenUrl,
} from '@/shared/workspace-modules';
import AiReactEntryShell from '@/components/ai/AiReactEntryShell.vue';
import type {
  ApiEnvelope,
  DashboardAttentionItem,
  DashboardIconName,
  DashboardQuickLinkResponse,
  DashboardRecentChange,
  DashboardRiskSummary,
  DashboardWorkbenchResponse,
  QuickLink,
  WorkbenchChange,
  WorkbenchData,
  WorkbenchRisk,
  WorkbenchTask,
} from './types';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';

const api = useApi();

const workbenchData = ref<WorkbenchData | null>(null);
const isLoading = ref(true);
const loadFailed = ref(false);
const errorMessage = ref<string>('');
const lastUpdated = ref<string>('');
const isRetrying = ref(false);

const severityOrder: Record<WorkbenchRisk['severity'], number> = { critical: 0, warning: 1, info: 2 };

const moduleRouteMap: Partial<Record<string, PageKey>> = {
  anomaly: 'anomaly_monitor',
  anomaly_monitor: 'anomaly_monitor',
  anomalies: 'anomaly_monitor',
  dashboard: 'dashboard',
  dispatch: 'dispatch_board',
  dispatch_board: 'dispatch_board',
  flight: 'flight_monitor',
  flight_monitor: 'flight_monitor',
  handover: 'operations_review_report',
  kpi: 'kpi_dashboard',
  operations_review: 'operations_review_report',
  resource: 'resource_manager',
  resource_manager: 'resource_manager',
  shift_handover: 'operations_review_report',
  system: 'system_status',
  system_status: 'system_status',
};

const modulePresentation: Record<string, { title: string; description: string; icon: DashboardIconName; theme: string; wide?: boolean }> = {
  anomaly: {
    title: '异常监控',
    description: '异常检测与处置',
    icon: 'activity',
    theme: 'theme-red',
  },
  dashboard: {
    title: '运行总览',
    description: '值班工作台与关键态势',
    icon: 'activity',
    theme: 'theme-blue',
  },
  dispatch: {
    title: '派工调度',
    description: '甘特图派工与重排',
    icon: 'bar_chart',
    theme: 'theme-blue',
    wide: true,
  },
  flight: {
    title: '航班监控',
    description: '实时航班动态',
    icon: 'plane',
    theme: 'theme-orange',
  },
  handover: {
    title: '交接复盘',
    description: '运行交接与班后复盘',
    icon: 'activity',
    theme: 'theme-purple',
  },
  kpi: {
    title: 'KPI 诊断',
    description: '性能指标分析',
    icon: 'bar_chart',
    theme: 'theme-purple',
  },
  resource: {
    title: '资源管理',
    description: '人员设备调度',
    icon: 'users',
    theme: 'theme-teal',
  },
  system: {
    title: '系统状态',
    description: '基础设施健康',
    icon: 'settings',
    theme: 'theme-dark',
  },
};

const intentDescriptions: Record<string, string> = {
  inspect_flight_risk: '查看高风险航班与异常态势',
  inspect_runtime_health: '查看运行健康与数据状态',
  resolve_dispatch_work: '处理派工冲突与重排任务',
  review_operational_closure: '查看交接班与运行复盘',
};

const sortedRisks = computed<WorkbenchRisk[]>(() => {
  if (!workbenchData.value) return [];
  return [...workbenchData.value.operation_risks].sort(
    (a, b) => (severityOrder[a.severity] ?? Number.MAX_SAFE_INTEGER) - (severityOrder[b.severity] ?? Number.MAX_SAFE_INTEGER)
  );
});

function normalizeKey(value?: string | null): string {
  return String(value ?? '')
    .trim()
    .toLowerCase()
    .replace(/[\s-]+/g, '_');
}

function formatClock(value?: string | null): string {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
}

function formatDue(value?: string | null): string | undefined {
  if (!value) return undefined;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return undefined;

  const now = new Date();
  const isToday =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();

  const timeText = date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
  if (isToday) {
    return `今日 ${timeText}`;
  }

  const dateText = date.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' });
  return `${dateText} ${timeText}`;
}

function resolveHref(moduleName?: string | null, fallback: PageKey = 'dashboard'): string {
  // D1: 业务模块进入多标签工作区；dashboard 自身仍回工作台首页
  const routeKey = moduleRouteMap[normalizeKey(moduleName)] ?? fallback;
  if (routeKey === 'dashboard' || routeKey === 'login') {
    return pageUrl(routeKey);
  }
  const workspaceMod = resolveWorkspaceModuleFromDashboard(moduleName)
    ?? resolveWorkspaceModuleFromDashboard(routeKey);
  if (workspaceMod) {
    return workspaceOpenUrl(workspaceMod);
  }
  return pageUrl(routeKey);
}

function normalizePriority(priority?: string | null): WorkbenchTask['priority'] {
  const normalized = normalizeKey(priority);
  if (['critical', 'high', 'urgent', '严重', '高'].includes(normalized)) {
    return 'high';
  }
  if (['medium', 'warning', 'normal', '中'].includes(normalized)) {
    return 'medium';
  }
  return 'low';
}

function normalizeTaskStatus(status?: string | null): WorkbenchTask['status'] {
  const normalized = normalizeKey(status);
  if (['done', 'closed', 'completed', 'resolved'].includes(normalized)) {
    return 'done';
  }
  if (['in_progress', 'processing', 'working'].includes(normalized)) {
    return 'in_progress';
  }
  return 'pending';
}

function normalizeRiskSeverity(severity?: string | null): WorkbenchRisk['severity'] {
  const normalized = normalizeKey(severity);
  if (['critical', 'high', '严重', '高'].includes(normalized)) {
    return 'critical';
  }
  if (['warning', 'medium', 'warn', '中'].includes(normalized)) {
    return 'warning';
  }
  return 'info';
}

function humanizeRole(roleHint?: string | null): string | undefined {
  const normalized = normalizeKey(roleHint);
  if (!normalized) return undefined;

  const labels: Record<string, string> = {
    dispatcher: '运行调度',
    flight_operator: '航班运行',
    operations_admin: '运行管理员',
    operator: '值班员',
  };

  return labels[normalized] ?? roleHint ?? undefined;
}

function sourceLabel(source?: string | null): string {
  const normalized = normalizeKey(source);
  const labels: Record<string, string> = {
    anomaly: '异常监控',
    anomalies: '异常监控',
    dispatch: '派工调度',
    system_health: '系统状态',
    todo: '待办中心',
  };

  return labels[normalized] ?? source ?? '系统';
}

function statusLabel(status?: string | null): string {
  const normalized = normalizeKey(status);
  const labels: Record<string, string> = {
    acknowledged: '已确认',
    closed: '已关闭',
    done: '已完成',
    in_progress: '处理中',
    open: '待处理',
    pending: '待处理',
  };

  return labels[normalized] ?? (status || '待处理');
}

function severityLabel(severity?: string | null): string | undefined {
  const normalized = normalizeKey(severity);
  if (!normalized) return undefined;

  const labels: Record<string, string> = {
    critical: '严重',
    high: '高',
    info: '提示',
    low: '低',
    medium: '中',
    warning: '预警',
  };

  return labels[normalized] ?? severity ?? undefined;
}

function summarizeAttentionItem(item: DashboardAttentionItem): string {
  const parts = [
    `来源：${sourceLabel(item.source)}`,
    `状态：${statusLabel(item.status)}`,
  ];

  if (item.recommended_action) {
    parts.push(`动作：${item.recommended_action}`);
  }

  return parts.join(' · ');
}

function mapAttentionItems(items: DashboardAttentionItem[]): WorkbenchTask[] {
  return items.map((item) => ({
    id: item.id,
    title: item.title,
    description: summarizeAttentionItem(item),
    priority: normalizePriority(item.priority),
    due: formatDue(item.due_at),
    href: resolveHref(item.source, 'dashboard'),
    status: normalizeTaskStatus(item.status),
  }));
}

function buildOperationRisks(riskSummary: DashboardRiskSummary): WorkbenchRisk[] {
  const risks: WorkbenchRisk[] = [];

  if (riskSummary.unresolved_anomalies > 0) {
    risks.push({
      id: 'risk-unresolved-anomalies',
      title: '待处理异常',
      description: `当前仍有 ${riskSummary.unresolved_anomalies} 条异常待处置`,
      severity: riskSummary.unresolved_anomalies >= 5 ? 'critical' : 'warning',
      count: riskSummary.unresolved_anomalies,
      href: workspaceOpenUrl('anomaly_monitor'),
    });
  }

  if (riskSummary.high_risk_flights > 0) {
    const firstFlight = riskSummary.high_risk_flight_refs[0];
    risks.push({
      id: 'risk-high-risk-flights',
      title: '高风险航班',
      description: firstFlight
        ? `${firstFlight.title} 等 ${riskSummary.high_risk_flights} 架航班需关注`
        : `当前有 ${riskSummary.high_risk_flights} 架高风险航班需关注`,
      severity: normalizeRiskSeverity(firstFlight?.severity ?? 'critical'),
      count: riskSummary.high_risk_flights,
      href: workspaceOpenUrl('flight_monitor'),
    });
  }

  if (riskSummary.dispatch_conflicts > 0) {
    risks.push({
      id: 'risk-dispatch-conflicts',
      title: '派工冲突',
      description: `存在 ${riskSummary.dispatch_conflicts} 项派工冲突等待处理`,
      severity: riskSummary.dispatch_conflicts >= 3 ? 'critical' : 'warning',
      count: riskSummary.dispatch_conflicts,
      href: workspaceOpenUrl('dispatch_board'),
    });
  }

  riskSummary.stale_data_indicators.forEach((indicator, index) => {
    risks.push({
      id: `risk-stale-data-${index}`,
      title: `${sourceLabel(indicator.source)} 数据时效`,
      description: indicator.detail,
      severity: indicator.state === 'unknown' ? 'warning' : 'info',
      href: workspaceOpenUrl('system_status'),
    });
  });

  return risks;
}

function mapRecentChanges(changes: DashboardRecentChange[]): WorkbenchChange[] {
  return changes.map((change) => {
    const parts = [`来源：${sourceLabel(change.source)}`];
    const severity = severityLabel(change.severity);
    if (severity) {
      parts.push(`级别：${severity}`);
    }
    if (change.entity_id) {
      parts.push(`对象：${change.entity_id}`);
    }

    return {
      id: change.id,
      title: change.title,
      description: parts.join(' · '),
      timestamp: formatClock(change.changed_at),
      actor: sourceLabel(change.source),
      type: normalizeKey(change.source),
    };
  });
}

function getModulePresentation(moduleName?: string | null, intent?: string | null) {
  const normalizedModule = normalizeKey(moduleName);
  if (modulePresentation[normalizedModule]) {
    return modulePresentation[normalizedModule];
  }

  const normalizedIntent = normalizeKey(intent);
  if (normalizedIntent.includes('dispatch')) {
    return modulePresentation.dispatch;
  }
  if (normalizedIntent.includes('flight')) {
    return modulePresentation.flight;
  }
  if (normalizedIntent.includes('health')) {
    return modulePresentation.system;
  }
  if (normalizedIntent.includes('handover') || normalizedIntent.includes('closure')) {
    return modulePresentation.handover;
  }

  return modulePresentation.dashboard;
}

function mapQuickLink(link: DashboardQuickLinkResponse): QuickLink {
  const presentation = getModulePresentation(link.module, link.intent);

  return {
    id: link.id,
    title: presentation.title,
    description: intentDescriptions[normalizeKey(link.intent)] ?? presentation.description,
    icon: presentation.icon,
    href: link.href || resolveHref(link.module, 'dashboard'),
    theme: presentation.theme,
    wide: presentation.wide,
  };
}

function buildModules(quickLinks: QuickLink[]): QuickLink[] {
  if (!quickLinks.length) {
    return [];
  }

  const modules = new Map<string, QuickLink>();
  quickLinks.forEach((link) => {
    const key = normalizeKey(link.id || link.title);
    if (modules.has(key)) {
      return;
    }
    modules.set(key, {
      ...link,
      id: `module-${key}`,
      wide: false,
    });
  });

  return Array.from(modules.values());
}

function mapWorkbenchResponse(payload: DashboardWorkbenchResponse): WorkbenchData {
  const quickLinks = payload.quick_links.map(mapQuickLink);

  return {
    user_name: payload.user_context.username?.trim() || humanizeRole(payload.role_hint) || '当前用户',
    user_role: humanizeRole(payload.role_hint),
    shift_label: payload.user_context.department ?? undefined,
    my_tasks: mapAttentionItems(payload.attention_items),
    operation_risks: buildOperationRisks(payload.risk_summary),
    recent_changes: mapRecentChanges(payload.recent_changes),
    quick_links: quickLinks,
    modules: buildModules(quickLinks),
  };
}

function useErrorState(message: string): void {
  workbenchData.value = null;
  loadFailed.value = true;
  errorMessage.value = message;
}

async function loadDashboard(): Promise<void> {
  isLoading.value = !isRetrying.value;
  loadFailed.value = false;
  errorMessage.value = '';
  try {
    const result = await api.get<ApiEnvelope<DashboardWorkbenchResponse>>('/api/v2/dashboard/workbench');
    if (result.ok && result.data?.success && result.data.data) {
      workbenchData.value = mapWorkbenchResponse(result.data.data);
      lastUpdated.value = formatClock(result.data.data.generated_at);
    } else {
      const apiMsg = result.data?.error || 'Dashboard returned an invalid response';
      useErrorState(apiMsg);
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : 'Failed to load dashboard data';
    useErrorState(msg);
  } finally {
    isLoading.value = false;
    isRetrying.value = false;
    if (!lastUpdated.value && !loadFailed.value) {
      lastUpdated.value = new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
    }
  }
}

async function handleRetry(): Promise<void> {
  isRetrying.value = true;
  await loadDashboard();
}

onMounted(() => {
  void loadDashboard();
});

function priorityClass(priority: string): string {
  return priority === 'high' ? 'priority-high' : priority === 'medium' ? 'priority-medium' : 'priority-low';
}

function severityClass(severity: string): string {
  return severity === 'critical' ? 'sev-critical' : severity === 'warning' ? 'sev-warning' : 'sev-info';
}
</script>

<template>
  <div id="navbar-host" />

  <div class="wb-shell">
    <!-- Workbench Header -->
    <header class="wb-header">
      <div class="wb-header-left">
        <div class="wb-identity">
          <span class="wb-user">{{ workbenchData?.user_name ?? (loadFailed ? '加载失败' : '加载中...') }}</span>
          <span v-if="workbenchData?.user_role" class="wb-role-tag">{{ workbenchData.user_role }}</span>
          <span v-if="workbenchData?.shift_label" class="wb-shift-tag">{{ workbenchData.shift_label }}</span>
        </div>
      </div>
      <div class="wb-header-right">
        <span v-if="loadFailed" class="wb-error-badge" role="alert">
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            aria-hidden="true"
          >
            <path d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          数据加载失败
          <button
            class="wb-retry-btn"
            @click="handleRetry"
            :disabled="isRetrying"
            aria-label="重试加载"
          >
            {{ isRetrying ? '重试中...' : '重试' }}
          </button>
        </span>
        <span v-if="lastUpdated" class="wb-updated">更新 {{ lastUpdated }}</span>
      </div>
    </header>

    <!-- Error banner when dashboard fails to load -->
    <div v-if="loadFailed && !isLoading" class="wb-error-banner" role="alert">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
        <path d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <div class="wb-error-content">
        <p class="wb-error-title">无法加载工作台数据</p>
        <p class="wb-error-detail">{{ errorMessage || '请检查网络连接或联系管理员' }}</p>
      </div>
      <button class="wb-retry-btn-primary" @click="handleRetry" :disabled="isRetrying">
        {{ isRetrying ? '重试中...' : '重新加载' }}
      </button>
    </div>

    <!-- Workbench Grid -->
    <div class="wb-grid" v-if="workbenchData">
      <!-- My Tasks -->
      <section class="wb-panel" aria-labelledby="tasks-title">
        <div class="panel-head">
          <h2 id="tasks-title" class="panel-title">
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              aria-hidden="true"
            >
              <path d="M9 11l3 3L22 4M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11" />
            </svg>
            我的待办
          </h2>
          <span v-if="workbenchData?.my_tasks?.length" class="panel-count">{{ workbenchData.my_tasks.length }}</span>
        </div>
        <div class="panel-body">
          <div v-if="isLoading" class="wb-skeleton-list">
            <div v-for="i in 3" :key="i" class="skel-row" />
          </div>
          <ul v-else-if="workbenchData?.my_tasks?.length" class="task-list" role="list">
            <li v-for="task in workbenchData.my_tasks" :key="task.id" class="task-item">
              <a :href="task.href" class="task-link">
                <span class="task-priority-dot" :class="priorityClass(task.priority)" :title="task.priority === 'high' ? '高优先级' : task.priority === 'medium' ? '中优先级' : '低优先级'" />
                <div class="task-body">
                  <span class="task-title">{{ task.title }}</span>
                  <span class="task-desc">{{ task.description }}</span>
                </div>
                <span v-if="task.due" class="task-due">{{ task.due }}</span>
              </a>
            </li>
          </ul>
          <div v-else class="panel-empty">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              aria-hidden="true"
            ><path d="M9 11l3 3L22 4M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11" /></svg>
            <span>暂无待办</span>
          </div>
        </div>
      </section>

      <!-- Operation Risks -->
      <section class="wb-panel" aria-labelledby="risks-title">
        <div class="panel-head">
          <h2 id="risks-title" class="panel-title">
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              aria-hidden="true"
            >
              <path d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            运行风险
          </h2>
          <span v-if="sortedRisks.length" class="panel-count" :class="{ 'count-critical': sortedRisks[0]?.severity === 'critical' }">{{ sortedRisks.length }}</span>
        </div>
        <div class="panel-body">
          <div v-if="isLoading" class="wb-skeleton-list">
            <div v-for="i in 3" :key="i" class="skel-row" />
          </div>
          <ul v-else-if="sortedRisks.length" class="risk-list" role="list">
            <li
              v-for="risk in sortedRisks"
              :key="risk.id"
              class="risk-item"
              :class="severityClass(risk.severity)"
            >
              <a v-if="risk.href" :href="risk.href" class="risk-link">
                <span class="risk-severity-bar" :class="severityClass(risk.severity)" />
                <div class="risk-body">
                  <span class="risk-title">{{ risk.title }}</span>
                  <span class="risk-desc">{{ risk.description }}</span>
                </div>
                <span v-if="risk.count !== undefined" class="risk-count" :class="severityClass(risk.severity)">{{ risk.count }}</span>
              </a>
              <div v-else class="risk-link risk-nohref">
                <span class="risk-severity-bar" :class="severityClass(risk.severity)" />
                <div class="risk-body">
                  <span class="risk-title">{{ risk.title }}</span>
                  <span class="risk-desc">{{ risk.description }}</span>
                </div>
                <span v-if="risk.count !== undefined" class="risk-count" :class="severityClass(risk.severity)">{{ risk.count }}</span>
              </div>
            </li>
          </ul>
          <div v-else class="panel-empty panel-empty--ok">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              aria-hidden="true"
            ><path d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
            <span>当前无运行风险</span>
          </div>
        </div>
      </section>

      <!-- Recent Changes -->
      <section class="wb-panel" aria-labelledby="changes-title">
        <div class="panel-head">
          <h2 id="changes-title" class="panel-title">
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              aria-hidden="true"
            >
              <path d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            最近变化
          </h2>
        </div>
        <div class="panel-body">
          <div v-if="isLoading" class="wb-skeleton-list">
            <div v-for="i in 3" :key="i" class="skel-row" />
          </div>
          <ul v-else-if="workbenchData?.recent_changes?.length" class="change-list" role="list">
            <li v-for="change in workbenchData.recent_changes" :key="change.id" class="change-item">
              <div class="change-time">
                {{ change.timestamp }}
              </div>
              <div class="change-body">
                <span class="change-title">{{ change.title }}</span>
                <span class="change-desc">{{ change.description }}</span>
              </div>
              <span v-if="change.actor" class="change-actor">{{ change.actor }}</span>
            </li>
          </ul>
          <div v-else class="panel-empty">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              aria-hidden="true"
            ><path d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
            <span>暂无最新变化</span>
          </div>
        </div>
      </section>

      <!-- Quick Links -->
      <section class="wb-panel" aria-labelledby="ql-title">
        <div class="panel-head">
          <h2 id="ql-title" class="panel-title">
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              aria-hidden="true"
            >
              <path d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
            快捷入口
          </h2>
        </div>
        <div class="panel-body">
          <div v-if="isLoading" class="wb-skeleton-grid">
            <div v-for="i in 4" :key="i" class="skel-card" />
          </div>
          <div v-else-if="workbenchData?.quick_links?.length" class="ql-grid">
            <a
              v-for="link in workbenchData.quick_links"
              :key="link.id"
              :href="link.href"
              class="ql-item"
              :class="link.theme || 'theme-blue'"
            >
              <SvgIcon
                :src="`/frontend/icons/${link.icon}.svg`"
                class="ql-icon"
              />
              <span class="ql-title">{{ link.title }}</span>
              <span class="ql-desc">{{ link.description }}</span>
            </a>
          </div>
          <div v-else class="panel-empty">
            <span>暂无可用入口</span>
          </div>
        </div>
      </section>
    </div>

    <!-- Module Cards (demoted below) -->
    <section class="wb-modules" aria-labelledby="modules-title">
      <div class="modules-head">
        <h2 id="modules-title" class="modules-title">
          系统模块
        </h2>
      </div>
      <div class="modules-grid">
        <div v-if="isLoading" class="wb-skeleton-grid">
          <div v-for="i in 6" :key="i" class="skel-card" />
        </div>
        <template v-else>
          <a
            v-for="mod in (workbenchData?.modules ?? [])"
            :key="mod.id"
            :href="mod.href"
            class="mod-card"
            :class="mod.theme || 'theme-blue'"
          >
            <div class="mod-icon-wrap">
              <SvgIcon :src="`/frontend/icons/${mod.icon}.svg`" />
            </div>
            <div class="mod-body">
              <span class="mod-title">{{ mod.title }}</span>
              <span class="mod-desc">{{ mod.description }}</span>
            </div>
          </a>
        </template>
      </div>
    </section>
  </div>

  <AiReactEntryShell :entry-name="'dashboard_ai_widget'" surface="widget" />
  <ThemeToggle />
</template>

<style scoped>
.wb-shell {
  width: 100%;
  padding: 36px 48px 48px;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* Header */
.wb-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 24px 0;
  margin-bottom: 32px;
  border-bottom: 1px solid var(--border-light);
  gap: 12px;
  flex-wrap: wrap;
}

.wb-header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.wb-identity {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.wb-user {
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: -0.3px;
}

.wb-role-tag {
  font-size: 11px;
  font-weight: 600;
  color: var(--system-blue);
  background: var(--system-blue-subtle);
  padding: 2px 8px;
  border-radius: 999px;
  text-transform: uppercase;
  letter-spacing: 0.02em;
}

.wb-shift-tag {
  font-size: 11px;
  font-weight: 500;
  color: var(--system-gray);
  background: rgba(142,142,147,0.12);
  padding: 2px 8px;
  border-radius: 999px;
}

.wb-header-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.wb-error-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 600;
  color: var(--system-red);
  background: var(--error-bg-subtle);
  padding: 3px 8px;
  border-radius: 999px;
}

.wb-retry-btn {
  font-size: 11px;
  font-weight: 500;
  color: var(--system-red);
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 0 4px;
  text-decoration: underline;
}

.wb-retry-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  text-decoration: none;
}

.wb-error-banner {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 16px 24px;
  padding: 16px 20px;
  background: var(--error-bg-subtle);
  border: 1px solid var(--error-border-subtle);
  border-radius: 10px;
  color: var(--text-primary);
}

.wb-error-banner svg {
  color: var(--system-red);
  flex-shrink: 0;
}

.wb-error-content {
  flex: 1;
}

.wb-error-title {
  font-size: 14px;
  font-weight: 600;
  margin: 0 0 2px 0;
}

.wb-error-detail {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0;
  word-break: break-word;
}

.wb-retry-btn-primary {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-inverse);
  background: var(--system-red);
  border: none;
  border-radius: 8px;
  padding: 8px 16px;
  cursor: pointer;
  flex-shrink: 0;
}

.wb-retry-btn-primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.wb-updated {
  font-size: 11px;
  color: var(--text-tertiary);
}

/* Workbench Grid */
.wb-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 20px;
  min-height: 0;
}

/* Panels */
.wb-panel {
  background: var(--bg-card);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-m);
  box-shadow: var(--shadow-sm);
  display: flex;
  flex-direction: column;
  min-height: 200px;
  max-height: 280px;
  overflow: hidden;
}

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px 8px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}

.panel-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0;
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.panel-count {
  font-size: 11px;
  font-weight: 700;
  color: var(--system-gray);
  background: rgba(142,142,147,0.1);
  padding: 1px 6px;
  border-radius: 999px;
  min-width: 20px;
  text-align: center;
}

.panel-count.count-critical {
  color: var(--system-red);
  background: var(--error-bg-subtle);
}

.panel-body {
  flex: 1;
  overflow-y: auto;
  padding: 6px 0;
}

/* Task List */
.task-list,
.risk-list,
.change-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.task-item + .task-item,
.risk-item + .risk-item,
.change-item + .change-item {
  border-top: 1px solid var(--border-light);
}

.task-link {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 7px 12px;
  text-decoration: none;
  color: inherit;
  transition: background-color 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  cursor: pointer;
}

.task-link:hover {
  background: rgba(0,0,0,0.025);
}

.task-link:focus-visible {
  outline: 2px solid var(--system-blue);
  outline-offset: -2px;
}

.task-priority-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  margin-top: 4px;
}

.priority-high { background: var(--system-red); }
.priority-medium { background: var(--system-orange); }
.priority-low { background: var(--system-gray); }

.task-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.task-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.task-desc {
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.task-due {
  font-size: 10px;
  font-weight: 500;
  color: var(--system-orange);
  white-space: nowrap;
  flex-shrink: 0;
}

/* Risk List */
.risk-link,
.risk-nohref {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 7px 12px;
  text-decoration: none;
  color: inherit;
  transition: background-color 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  cursor: pointer;
}

.risk-link:hover {
  background: rgba(0,0,0,0.025);
}

.risk-link:focus-visible {
  outline: 2px solid var(--system-blue);
  outline-offset: -2px;
}

.risk-severity-bar {
  width: 3px;
  border-radius: 2px;
  flex-shrink: 0;
  align-self: stretch;
  min-height: 32px;
}

.sev-critical { background: var(--system-red); }
.sev-warning { background: var(--system-orange); }
.sev-info { background: var(--system-blue); }

.risk-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.risk-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.risk-desc {
  font-size: 11px;
  color: var(--text-secondary);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.risk-count {
  font-size: 11px;
  font-weight: 700;
  padding: 1px 5px;
  border-radius: 4px;
  flex-shrink: 0;
}

.risk-count.sev-critical {
  color: var(--system-red);
  background: var(--error-bg-subtle);
}

.risk-count.sev-warning {
  color: var(--system-orange);
  background: var(--dh-signal-warn-soft);
}

.risk-count.sev-info {
  color: var(--system-blue);
  background: var(--system-blue-subtle);
}

/* Change List */
.change-item {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 6px 12px;
  border-top: 1px solid var(--border-light);
}

.change-time {
  font-size: 10px;
  font-weight: 600;
  font-family: 'SF Mono', 'Menlo', monospace;
  color: var(--text-tertiary);
  flex-shrink: 0;
  margin-top: 1px;
}

.change-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.change-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.change-desc {
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.change-actor {
  font-size: 10px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

/* Quick Links Grid */
.ql-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 6px;
  padding: 6px 8px;
}

.ql-item {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  padding: 8px 10px;
  border-radius: 8px;
  text-decoration: none;
  color: inherit;
  background: rgba(0,0,0,0.02);
  transition: background-color 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  cursor: pointer;
  border: 1px solid transparent;
}

.ql-item:hover {
  background: rgba(0,0,0,0.05);
  border-color: var(--border-light);
}

.ql-item:focus-visible {
  outline: 2px solid var(--system-blue);
  outline-offset: -2px;
}

.ql-icon {
  width: 18px;
  height: 18px;
  color: var(--text-secondary);
}

.ql-title {
  font-size: 11px;
  font-weight: 700;
  color: var(--text-primary);
}

.ql-desc {
  font-size: 10px;
  color: var(--text-tertiary);
  line-height: 1.3;
}

/* Modules Section */
.wb-modules {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.modules-head {
  padding: 0;
}

.modules-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0;
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.modules-grid {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 20px;
}

.mod-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 22px 24px;
  text-decoration: none;
  color: inherit;
  background: var(--bg-card);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-l);
  box-shadow: var(--shadow-sm);
  transition: background-color 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  cursor: pointer;
}

.mod-card:hover {
  background: rgba(0,0,0,0.025);
}

.mod-card:focus-visible {
  outline: 2px solid var(--system-blue);
  outline-offset: -2px;
}

.mod-icon-wrap {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  background: rgba(0,0,0,0.04);
}

.mod-body {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.mod-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mod-desc {
  font-size: 10px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Empty & Skeleton */
.panel-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 24px 12px;
  color: var(--text-tertiary);
  font-size: 12px;
  text-align: center;
}

.panel-empty--ok {
  color: var(--system-green);
}

.wb-skeleton-list {
  padding: 6px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.wb-skeleton-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 6px;
  padding: 6px 8px;
}

.skel-row {
  height: 32px;
  border-radius: 4px;
  background: linear-gradient(90deg, rgba(0,0,0,0.05) 25%, rgba(0,0,0,0.08) 50%, rgba(0,0,0,0.05) 75%);
  background-size: 200% 100%;
  animation: shimmer 1.6s ease-in-out infinite;
}

.skel-card {
  height: 56px;
  border-radius: 6px;
  background: linear-gradient(90deg, rgba(0,0,0,0.05) 25%, rgba(0,0,0,0.08) 50%, rgba(0,0,0,0.05) 75%);
  background-size: 200% 100%;
  animation: shimmer 1.6s ease-in-out infinite;
}

@media (prefers-reduced-motion: reduce) {
  .skel-row, .skel-card {
    animation: none;
    background: rgba(0,0,0,0.07);
  }
}

@keyframes shimmer {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

/* Responsive */
@media (max-width: 1200px) {
  .wb-grid { grid-template-columns: repeat(3, 1fr); }
  .modules-grid { grid-template-columns: repeat(4, 1fr); }
}

@media (max-width: 1024px) {
  .wb-grid { grid-template-columns: repeat(3, 1fr); }
  .modules-grid { grid-template-columns: repeat(3, 1fr); }
  .wb-shell { padding: 28px 32px 40px; }
}

@media (max-width: 768px) {
  .wb-grid { grid-template-columns: repeat(2, 1fr); }
  .modules-grid { grid-template-columns: repeat(2, 1fr); }
  .wb-shell { padding: 24px 20px 32px; }
  .wb-shift-tag { display: none; }
  .wb-updated { display: none; }
}

@media (max-width: 600px) {
  .wb-grid { grid-template-columns: 1fr; }
  .modules-grid { grid-template-columns: 1fr; }
  .ql-grid { grid-template-columns: 1fr; }
  .wb-shell { padding: 16px 12px 24px; }
}
</style>
