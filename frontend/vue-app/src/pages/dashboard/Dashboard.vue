<script setup lang="ts">
import { onMounted, ref, computed } from 'vue';
import { useApi } from '@/composables/useApi';
import { pageUrl, type PageKey } from '@/shared/page-routes';
import {
  resolveWorkspaceModuleFromDashboard,
  workspaceOpenUrl,
} from '@/shared/workspace-modules';
import DashboardAiWidget from '@/components/ai/DashboardAiWidget.vue';
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
import UiBanner from '@/components/ui/UiBanner.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSkeleton from '@/components/ui/UiSkeleton.vue';

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

const modulePresentation: Record<string, { title: string; description: string; icon: DashboardIconName; wide?: boolean }> = {
  anomaly: {
    title: '异常监控',
    description: '异常检测与处置',
    icon: 'activity',
  },
  dashboard: {
    title: '运行总览',
    description: '值班工作台与关键态势',
    icon: 'activity',
  },
  dispatch: {
    title: '派工调度',
    description: '甘特图派工与重排',
    icon: 'bar_chart',
    wide: true,
  },
  flight: {
    title: '航班监控',
    description: '实时航班动态',
    icon: 'plane',
  },
  handover: {
    title: '交接复盘',
    description: '运行交接与班后复盘',
    icon: 'activity',
  },
  kpi: {
    title: 'KPI 诊断',
    description: '性能指标分析',
    icon: 'bar_chart',
  },
  resource: {
    title: '资源管理',
    description: '人员设备调度',
    icon: 'users',
  },
  system: {
    title: '系统状态',
    description: '基础设施健康',
    icon: 'settings',
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

function severityTone(severity: string): 'danger' | 'warn' | 'act' {
  return severity === 'critical' ? 'danger' : severity === 'warning' ? 'warn' : 'act';
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
          <UiPill v-if="workbenchData?.user_role" tone="act">
            {{ workbenchData.user_role }}
          </UiPill>
          <UiPill v-if="workbenchData?.shift_label" tone="mute">
            {{ workbenchData.shift_label }}
          </UiPill>
        </div>
      </div>
      <div class="wb-header-right">
        <UiPill v-if="loadFailed" tone="danger" role="alert">
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
        </UiPill>
        <button
          v-if="loadFailed"
          class="wb-retry-btn"
          :disabled="isRetrying"
          aria-label="重试加载"
          @click="handleRetry"
        >
          {{ isRetrying ? '重试中...' : '重试' }}
        </button>
        <span v-if="lastUpdated" class="wb-updated">更新 {{ lastUpdated }}</span>
      </div>
    </header>

    <!-- 升：加载失败时从顶部报一句话的事态 + 重试谓词 -->
    <div v-if="loadFailed && !isLoading" class="wb-banner-wrap" role="alert">
      <UiBanner tone="danger">
        <span class="wb-error-text">
          无法加载工作台数据：{{ errorMessage || '请检查网络连接或联系管理员' }}
        </span>
        <UiButton variant="danger" :disabled="isRetrying" @click="handleRetry">
          {{ isRetrying ? '重试中...' : '重新加载' }}
        </UiButton>
      </UiBanner>
    </div>

    <!-- Workbench Grid -->
    <div v-if="workbenchData" class="wb-grid">
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
          <UiPill v-if="workbenchData?.my_tasks?.length">
            {{ workbenchData.my_tasks.length }}
          </UiPill>
        </div>
        <div class="panel-body">
          <div
            v-if="isLoading"
            class="wb-skeleton-list"
            role="status"
            aria-label="加载中"
            aria-busy="true"
          >
            <UiSkeleton v-for="i in 3" :key="i" height="32px" />
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
          <UiPill v-if="sortedRisks.length" :tone="sortedRisks[0]?.severity === 'critical' ? 'danger' : 'mute'">
            {{ sortedRisks.length }}
          </UiPill>
        </div>
        <div class="panel-body">
          <div
            v-if="isLoading"
            class="wb-skeleton-list"
            role="status"
            aria-label="加载中"
            aria-busy="true"
          >
            <UiSkeleton v-for="i in 3" :key="i" height="32px" />
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
                <span v-if="risk.count !== undefined" class="risk-count">
                  <UiPill :tone="severityTone(risk.severity)">{{ risk.count }}</UiPill>
                </span>
              </a>
              <div v-else class="risk-link risk-nohref">
                <span class="risk-severity-bar" :class="severityClass(risk.severity)" />
                <div class="risk-body">
                  <span class="risk-title">{{ risk.title }}</span>
                  <span class="risk-desc">{{ risk.description }}</span>
                </div>
                <span v-if="risk.count !== undefined" class="risk-count">
                  <UiPill :tone="severityTone(risk.severity)">{{ risk.count }}</UiPill>
                </span>
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
          <div
            v-if="isLoading"
            class="wb-skeleton-list"
            role="status"
            aria-label="加载中"
            aria-busy="true"
          >
            <UiSkeleton v-for="i in 3" :key="i" height="32px" />
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
          <div
            v-if="isLoading"
            class="wb-skeleton-grid"
            role="status"
            aria-label="加载中"
            aria-busy="true"
          >
            <UiSkeleton v-for="i in 4" :key="i" height="56px" />
          </div>
          <div v-else-if="workbenchData?.quick_links?.length" class="ql-grid">
            <a
              v-for="link in workbenchData.quick_links"
              :key="link.id"
              :href="link.href"
              class="ql-item"
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
        <div
          v-if="isLoading"
          class="wb-skeleton-grid"
          role="status"
          aria-label="加载中"
          aria-busy="true"
        >
          <UiSkeleton v-for="i in 6" :key="i" height="56px" />
        </div>
        <template v-else>
          <a
            v-for="mod in (workbenchData?.modules ?? [])"
            :key="mod.id"
            :href="mod.href"
            class="mod-card"
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

  <DashboardAiWidget />
  <ThemeToggle />
</template>

<style scoped>
.wb-shell {
  width: 100%;
  padding: 36px 48px 48px;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--s4);
}

/* Header */
.wb-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--s4) 0;
  margin-bottom: var(--s5);
  border-bottom: 1px solid var(--line);
  gap: var(--s3);
  flex-wrap: wrap;
}

.wb-header-left {
  display: flex;
  align-items: center;
  gap: var(--s3);
}

.wb-identity {
  display: flex;
  align-items: center;
  gap: var(--s2);
  flex-wrap: wrap;
}

.wb-user {
  font-size: var(--fs-page);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  letter-spacing: -0.3px;
}

.wb-header-right {
  display: flex;
  align-items: center;
  gap: var(--s3);
}

.wb-retry-btn {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--danger);
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 0 var(--s1);
  text-decoration: underline;
}

.wb-retry-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  text-decoration: none;
}

.wb-banner-wrap {
  margin-bottom: var(--s3);
}

.wb-error-text {
  flex: 1;
  min-width: 0;
  word-break: break-word;
}

.wb-updated {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

/* Workbench Grid */
.wb-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--s4);
  min-height: 0;
}

/* Panels */
.wb-panel {
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
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
  padding: var(--s2) var(--s3);
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
}

.panel-title {
  display: flex;
  align-items: center;
  gap: var(--s2);
  font-size: var(--fs-section);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
  margin: 0;
}

.panel-body {
  flex: 1;
  overflow-y: auto;
  padding: var(--s2) 0;
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
  border-top: 1px solid var(--line);
}

.task-link {
  display: flex;
  align-items: flex-start;
  gap: var(--s2);
  padding: var(--s2) var(--s3);
  text-decoration: none;
  color: inherit;
  transition: background-color var(--t-fast) var(--ease);
  cursor: pointer;
}

.task-link:hover {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.task-link:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.task-priority-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  margin-top: var(--s1);
}

.priority-high { background: var(--danger); }
.priority-medium { background: var(--warn); }
.priority-low { background: var(--ink-muted); }

.task-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.task-title {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.task-desc {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.task-due {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--warn);
  white-space: nowrap;
  flex-shrink: 0;
}

/* Risk List */
.risk-link,
.risk-nohref {
  display: flex;
  align-items: flex-start;
  gap: var(--s2);
  padding: var(--s2) var(--s3);
  text-decoration: none;
  color: inherit;
  transition: background-color var(--t-fast) var(--ease);
  cursor: pointer;
}

.risk-link:hover {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.risk-link:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.risk-severity-bar {
  width: 3px;
  border-radius: 2px;
  flex-shrink: 0;
  align-self: stretch;
  min-height: var(--h-sm);
}

.sev-critical { background: var(--danger); }
.sev-warning { background: var(--warn); }
.sev-info { background: var(--act); }

.risk-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.risk-title {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.risk-desc {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.risk-count {
  flex-shrink: 0;
}

/* Change List */
.change-item {
  display: flex;
  align-items: flex-start;
  gap: var(--s2);
  padding: var(--s2) var(--s3);
  border-top: 1px solid var(--line);
}

.change-time {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  font-family: var(--mono);
  color: var(--ink-muted);
  flex-shrink: 0;
}

.change-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.change-title {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.change-desc {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.change-actor {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  flex-shrink: 0;
}

/* Quick Links Grid */
.ql-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--s2);
  padding: var(--s2) var(--s3);
}

.ql-item {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--s1);
  padding: var(--s2) var(--s3);
  border-radius: var(--r-control);
  text-decoration: none;
  color: inherit;
  background: color-mix(in srgb, var(--ink) 4%, transparent);
  transition: background-color var(--t-fast) var(--ease);
  cursor: pointer;
  border: 1px solid transparent;
}

.ql-item:hover {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  border-color: var(--line);
}

.ql-item:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.ql-icon {
  width: 18px;
  height: 18px;
  color: var(--ink-subtle);
}

.ql-title {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.ql-desc {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  line-height: 1.3;
}

/* Modules Section */
.wb-modules {
  display: flex;
  flex-direction: column;
  gap: var(--s4);
}

.modules-head {
  padding: 0;
}

.modules-title {
  font-size: var(--fs-section);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
  margin: 0;
}

.modules-grid {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: var(--s4);
}

.mod-card {
  display: flex;
  align-items: center;
  gap: var(--s3);
  padding: var(--s4);
  text-decoration: none;
  color: inherit;
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-sm);
  transition: background-color var(--t-fast) var(--ease);
  cursor: pointer;
}

.mod-card:hover {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.mod-card:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.mod-icon-wrap {
  width: 32px;
  height: 32px;
  border-radius: var(--r-control);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  color: var(--ink-subtle);
}

.mod-body {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.mod-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mod-desc {
  font-size: var(--fs-label);
  color: var(--ink-muted);
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
  gap: var(--s2);
  padding: var(--s4) var(--s3);
  color: var(--ink-muted);
  font-size: var(--fs-label);
  text-align: center;
}

.panel-empty--ok {
  color: var(--ok);
}

/* 骨架的砖与洗光来自 UiSkeleton（信号面 §3.9 只有一块砖），这里只负责摆位 */
.wb-skeleton-list {
  padding: var(--s2) var(--s3);
  display: flex;
  flex-direction: column;
  gap: var(--s2);
}

.wb-skeleton-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--s2);
  padding: var(--s2) var(--s3);
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
