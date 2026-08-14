import { useEffect, useMemo, useState } from 'react';
import { Badge, Button, Card, Space, Statistic, Table, Tag, Typography, message, Alert } from 'antd';
import { ReloadOutlined, ThunderboltOutlined, CheckCircleOutlined, CloseCircleOutlined, ScheduleOutlined, ExclamationCircleOutlined } from '@ant-design/icons';
import { ChatMessage } from '@/components/chat/AiChatShell';
import { AiPageNavigation } from '@/components/shell/AiPageNavigation';
import { getAiCapabilities, getExecutionVisibilityMetrics, getReportSchemaMetrics, getRoutingMetrics, listPendingActions, approvePendingAction, rejectPendingAction, listAiJobs, getAiJobStats, getProposalStats, getRolloutStatus, executeProposal, listProposals, type RolloutStatusResponse, type ProposalRow } from '@/lib/api/aiApi';
import { EventSourceClient } from '@/lib/sse/eventSourceClient';
import { normalizeTime } from '@/lib/utils';
import { applyPendingActionEvent, type PendingRow } from '@/features/ai-monitor/pendingActionRows';

function ReadinessBanner({ status }: { status: RolloutStatusResponse | null }): JSX.Element | null {
  if (!status?.readiness) return null;

  const isReady = status.readiness.overall_status === 'Ready';
  const checks = Array.isArray(status.readiness.checks) ? status.readiness.checks : [];
  const failures = checks.filter((check) => check.status === 'Fail');
  const allowedActions = Array.isArray(status.allowed_actions) ? status.allowed_actions : [];
  const allowText =
    status.execution_mode === 'allow_all'
      ? 'All actions'
      : allowedActions.length > 0
        ? allowedActions.join(', ')
        : 'None';

  return (
    <Alert
      type={isReady ? 'success' : 'error'}
      showIcon
      message={isReady ? 'Execution Ready' : 'Execution Not Ready'}
      description={
        <Space direction="vertical" size={4}>
          <Typography.Text>Mode: {status.execution_mode}; Allowed: {allowText}</Typography.Text>
          {failures.map((check) => (
            <Typography.Text key={check.name} type="danger">
              {check.name}: {check.message}
            </Typography.Text>
          ))}
        </Space>
      }
    />
  );
}

function canExecuteProposal(row: ProposalRow, status: RolloutStatusResponse | null): boolean {
  if (!status?.execution_enabled || status.readiness?.overall_status !== 'Ready') return false;
  if (status.execution_mode === 'allow_all') return true;
  const allowedActions = Array.isArray(status.allowed_actions) ? status.allowed_actions : [];
  return allowedActions.some(
    (item) => item.toLowerCase() === `${row.object_type}.${row.action_name}`.toLowerCase(),
  );
}

export function AiMonitorPage(): JSX.Element {
  const [loading, setLoading] = useState(false);
  const [capability, setCapability] = useState<Record<string, unknown> | null>(null);
  const [pendingRows, setPendingRows] = useState<PendingRow[]>([]);
  const [events, setEvents] = useState<ChatMessage[]>([]);
  const [metrics, setMetrics] = useState({
    routing: {} as Record<string, unknown>,
    schema: {} as Record<string, unknown>,
    visibility: {} as Record<string, unknown>,
  });
  const [jobs, setJobs] = useState<Array<Record<string, unknown>>>([]);
  const [jobsLoading, setJobsLoading] = useState(false);
  const [jobStats, setJobStats] = useState<Record<string, unknown>>({});
  const [proposalStats, setProposalStats] = useState<Record<string, unknown>>({});
  const [rolloutStatus, setRolloutStatus] = useState<RolloutStatusResponse | null>(null);
  const [approvedProposals, setApprovedProposals] = useState<ProposalRow[]>([]);
  const [optionalLoadErrors, setOptionalLoadErrors] = useState<string[]>([]);

  const loadPageData = async (): Promise<void> => {
    setLoading(true);
    const optionalErrors: string[] = [];
    const optional = async <T,>(label: string, loader: () => Promise<T>, fallback: T): Promise<T> => {
      try {
        return await loader();
      } catch (error) {
        optionalErrors.push(`${label}: ${error instanceof Error ? error.message : '加载失败'}`);
        return fallback;
      }
    };
    try {
      const [cap, pending, routing, schema, visibility, jStats, pStats, jobList, rollout, proposals] = await Promise.all([
        getAiCapabilities(),
        listPendingActions({ status: 'pending', limit: 200, offset: 0 }),
        optional('路由指标', getRoutingMetrics, {}),
        optional('Schema 指标', getReportSchemaMetrics, {}),
        optional('执行可见性', getExecutionVisibilityMetrics, {}),
        optional('任务统计', getAiJobStats, {}),
        optional('建议统计', getProposalStats, {}),
        optional('任务列表', () => listAiJobs({ limit: 20 }), []),
        optional('Rollout 状态', getRolloutStatus, null),
        optional('已批准建议', () => listProposals({ status: 'approved', limit: 50, offset: 0 }), []),
      ]);
      setOptionalLoadErrors(optionalErrors);
      setCapability(cap as unknown as Record<string, unknown>);
      setPendingRows(
        (pending.items || []).map((item) => {
          const actionId = String(item.action_id || '');
          return {
            key: actionId,
            actionId,
            toolName: String(item.tool_name || ''),
            status: String(item.status || ''),
            createdAt: normalizeTime(String(item.created_at || '')),
            expiresAt: normalizeTime(String(item.expires_at || '')),
            raw: item,
          };
        }),
      );
      setMetrics({
        routing: routing || {},
        schema: schema || {},
        visibility: visibility || {},
      });
      setJobStats(jStats || {});
      setProposalStats(pStats || {});
      setJobs(Array.isArray(jobList) ? jobList : []);
      setRolloutStatus(rollout);
      setApprovedProposals(
        (Array.isArray(proposals) ? proposals : []).map((item) => ({
          proposal_id: String(item.proposal_id || ''),
          object_type: String(item.object_type || ''),
          object_id: String(item.object_id || ''),
          action_name: String(item.action_name || ''),
          status: String(item.status || ''),
          created_at: item.created_at ? String(item.created_at) : undefined,
          updated_at: item.updated_at ? String(item.updated_at) : undefined,
        })),
      );
    } catch (error) {
      message.error(error instanceof Error ? error.message : 'AI 监控数据加载失败');
      setOptionalLoadErrors([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadPageData();
  }, []);

  const handleExecuteProposal = async (proposalId: string): Promise<void> => {
    try {
      await executeProposal(proposalId);
      message.success(`已执行 ${proposalId}`);
      setApprovedProposals((prev) => prev.filter((row) => row.proposal_id !== proposalId));
      await loadPageData();
    } catch (error) {
      message.error(error instanceof Error ? error.message : '执行失败');
    }
  };

  useEffect(() => {
    const client = new EventSourceClient({
      url: '/api/v2/ai/events/stream',
      eventNames: ['ai_execution', 'smart_monitor'],
      clientScope: 'ai_monitor_react',
      onEvent: (eventName, payload) => {
        const runtime = payload.payload && typeof payload.payload === 'object'
          ? (payload.payload as Record<string, unknown>)
          : payload;
        const runtimeRecord = runtime && typeof runtime === 'object'
          ? runtime as Record<string, unknown>
          : {};
        const semantic = String(runtimeRecord.event || payload.type || eventName || '').toLowerCase();
        const text = String(runtimeRecord.message || payload.message || semantic || '新事件');
        const id = `${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        setEvents((prev) => [
          ...prev.slice(-49),
          {
            id,
            role: 'assistant',
            content: `[${semantic || 'runtime'}] ${text}`,
          },
        ]);
        const pendingAction = runtimeRecord.pending_action && typeof runtimeRecord.pending_action === 'object'
          ? runtimeRecord.pending_action as Record<string, unknown>
          : null;
        if ((semantic === 'approval_required' || semantic === 'approval_result') && pendingAction) {
          setPendingRows((prev) => applyPendingActionEvent(prev, semantic, pendingAction));
        }
      },
      onError: () => {
        message.warning('AI 实时事件流断开，等待自动重连');
        setEvents((prev) => [
          ...prev.slice(-49),
          { id: `err_${Date.now()}`, role: 'system', content: '实时事件流断开，等待自动重连...' },
        ]);
      },
    });
    client.connect();
    return () => client.disconnect();
  }, []);

  const metricJson = useMemo(
    () => ({
      routing: JSON.stringify(metrics.routing, null, 2),
      schema: JSON.stringify(metrics.schema, null, 2),
      visibility: JSON.stringify(metrics.visibility, null, 2),
    }),
    [metrics.routing, metrics.schema, metrics.visibility],
  );

  // Stat helpers
  const aiReady = Boolean(capability?.ai_ready);
  const aiChat = Boolean(capability?.ai_chat_permission);
  const aiExec = Boolean(capability?.ai_execute_permission);

  return (
    <div className="ai-page-shell">
      <Space direction="vertical" size={16} style={{ width: '100%' }}>
        {/* ---- Header ---- */}
        <div className="ai-toolbar ai-reveal">
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              实时监控
            </Typography.Title>
            <Badge
              count={pendingRows.length}
              style={{ backgroundColor: pendingRows.length > 0 ? 'var(--ai-warning)' : 'var(--ai-success)' }}
            />
          </div>
          <Button icon={<ReloadOutlined />} onClick={() => loadPageData()} loading={loading}>
            刷新
          </Button>
        </div>

        {/* ---- Navigation ---- */}
        <AiPageNavigation currentKey="ai-monitor" />

        {/* ---- Readiness Banner ---- */}
        <ReadinessBanner status={rolloutStatus} />
        {optionalLoadErrors.length > 0 ? (
          <Alert
            type="warning"
            showIcon
            message="部分 AI 监控指标加载失败"
            description={optionalLoadErrors.join('；')}
          />
        ) : null}

        {/* ---- Status Dashboard ---- */}
        {capability ? (
          <div className="ai-grid-auto ai-reveal ai-reveal-2">
            <div className="ai-stat-card">
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <span className={`ai-status-dot ${aiReady ? 'ai-status-dot--ok' : 'ai-status-dot--error'}`} />
                <span className="ai-stat-card__value">{aiReady ? 'READY' : 'OFFLINE'}</span>
              </div>
              <div className="ai-stat-card__label">AI Engine Status</div>
            </div>
            <div className="ai-stat-card">
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <span className={`ai-status-dot ${aiChat ? 'ai-status-dot--ok' : 'ai-status-dot--warn'}`} />
                <span className="ai-stat-card__value">{aiChat ? 'ON' : 'OFF'}</span>
              </div>
              <div className="ai-stat-card__label">Chat Permission</div>
            </div>
            <div className="ai-stat-card">
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <span className={`ai-status-dot ${aiExec ? 'ai-status-dot--ok' : 'ai-status-dot--warn'}`} />
                <span className="ai-stat-card__value">{aiExec ? 'ON' : 'OFF'}</span>
              </div>
              <div className="ai-stat-card__label">Execute Permission</div>
            </div>
          </div>
        ) : null}

        {/* ---- Main Content: Event Stream + Approval Queue ---- */}
        <div className="ai-split-panel ai-reveal ai-reveal-3">
          {/* Left: Live Event Stream */}
          <Card className="ai-page-card" style={{ minHeight: 480 }}>
            <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <Typography.Text strong style={{ fontSize: 13, textTransform: 'uppercase', letterSpacing: '0.04em', color: 'var(--ai-muted)' }}>
                <ThunderboltOutlined style={{ marginRight: 6 }} />Events Stream
              </Typography.Text>
              <Button size="small" onClick={() => setEvents([])}>清空</Button>
            </div>
            <div className="ai-log-stream" style={{ height: 400, overflow: 'auto' }}>
              {events.length === 0 && <span style={{ color: 'var(--ai-muted)' }}>等待事件...</span>}
              {events.map((evt) => (
                <div key={evt.id} style={{ marginBottom: 2, color: evt.role === 'system' ? 'var(--ai-warning)' : 'var(--ai-success)' }}>
                  {evt.content}
                </div>
              ))}
            </div>
          </Card>

          {/* Right: Approval Queue */}
          <Card className="ai-page-card" style={{ minHeight: 480 }}>
            <Typography.Text strong style={{ fontSize: 13, textTransform: 'uppercase', letterSpacing: '0.04em', color: 'var(--ai-muted)', display: 'block', marginBottom: 12 }}>
              审批队列 ({pendingRows.length})
            </Typography.Text>

            {pendingRows.length === 0 ? (
              <div style={{ padding: '40px 0', textAlign: 'center' }}>
                <Typography.Text type="secondary">暂无待审批任务</Typography.Text>
              </div>
            ) : (
              <Space direction="vertical" style={{ width: '100%' }} size={8}>
                {pendingRows.map((row) => (
                  <div
                    key={row.key}
                    className={`ai-alert-card ${row.status === 'expired' ? 'ai-alert-card--danger' : ''} ai-pulse`}
                  >
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8 }}>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 13, fontWeight: 600, color: 'var(--ai-text-heading)' }}>
                          {row.toolName}
                        </div>
                        <div style={{ fontSize: 11, color: 'var(--ai-muted)', marginTop: 4, fontFamily: 'var(--ai-font-mono)' }}>
                          {row.actionId}
                        </div>
                        <div style={{ fontSize: 11, color: 'var(--ai-muted)', marginTop: 2 }}>
                          创建: {row.createdAt} · 过期: {row.expiresAt}
                        </div>
                      </div>
                      <Space size={6}>
                        <Button
                          size="small"
                          type="primary"
                          icon={<CheckCircleOutlined />}
                          onClick={async () => {
                            try {
                              await approvePendingAction(row.actionId);
                              message.success(`已批准 ${row.actionId}`);
                              setPendingRows((prev) => prev.filter((item) => item.actionId !== row.actionId));
                            } catch (error) {
                              message.error(error instanceof Error ? error.message : '批准失败');
                            }
                          }}
                        >
                          批准
                        </Button>
                        <Button
                          size="small"
                          danger
                          icon={<CloseCircleOutlined />}
                          onClick={async () => {
                            try {
                              await rejectPendingAction(row.actionId, 'react_monitor_manual_reject');
                              message.warning(`已拒绝 ${row.actionId}`);
                              setPendingRows((prev) => prev.filter((item) => item.actionId !== row.actionId));
                            } catch (error) {
                              message.error(error instanceof Error ? error.message : '拒绝失败');
                            }
                          }}
                        >
                          拒绝
                        </Button>
                      </Space>
                    </div>
                  </div>
                ))}
              </Space>
            )}
          </Card>
        </div>

        {/* ---- Metrics Section ---- */}
        <Card className="ai-page-card ai-reveal ai-reveal-4">
          <Typography.Text strong style={{ fontSize: 13, textTransform: 'uppercase', letterSpacing: '0.04em', color: 'var(--ai-muted)', display: 'block', marginBottom: 12 }}>
            运行指标
          </Typography.Text>
          <div className="ai-grid-auto-lg">
            <div>
              <Typography.Text strong style={{ fontSize: 11, display: 'block', marginBottom: 6, color: 'var(--ai-muted)', textTransform: 'uppercase' }}>Query Routing</Typography.Text>
              <pre>{metricJson.routing}</pre>
            </div>
            <div>
              <Typography.Text strong style={{ fontSize: 11, display: 'block', marginBottom: 6, color: 'var(--ai-muted)', textTransform: 'uppercase' }}>Report Schema</Typography.Text>
              <pre>{metricJson.schema}</pre>
            </div>
            <div>
              <Typography.Text strong style={{ fontSize: 11, display: 'block', marginBottom: 6, color: 'var(--ai-muted)', textTransform: 'uppercase' }}>Execution Visibility</Typography.Text>
              <pre>{metricJson.visibility}</pre>
            </div>
          </div>
        </Card>

        <div className="ai-grid-auto-lg">
          <Card className="ai-page-card" title={<span><ScheduleOutlined style={{ marginRight: 8 }} />Job 监控</span>}>
            <Table
              size="small"
              dataSource={jobs}
              rowKey={(row) => String((row as Record<string, unknown>).job_id || `row_${(row as Record<string, unknown>)._index ?? ''}`)}
              pagination={{ pageSize: 10 }}
              columns={[
                {
                  title: 'Job ID',
                  dataIndex: 'job_id',
                  key: 'job_id',
                  ellipsis: true,
                  render: (val: string) => val ? `${val.slice(0, 8)}…` : '-',
                },
                {
                  title: 'Job Type',
                  dataIndex: 'job_type',
                  key: 'job_type',
                },
                {
                  title: 'Status',
                  dataIndex: 'status',
                  key: 'status',
                  render: (val: string) => {
                    const colorMap: Record<string, string> = {
                      pending: 'blue',
                      running: 'orange',
                      succeeded: 'green',
                      failed_terminal: 'red',
                      cancelled: 'default',
                    };
                    return <Tag color={colorMap[val] || 'default'}>{val || '-'}</Tag>;
                  },
                },
                {
                  title: 'Created At',
                  dataIndex: 'created_at',
                  key: 'created_at',
                  render: (val: string) => normalizeTime(val),
                },
              ]}
            />
          </Card>

          <Card className="ai-page-card" title="统计概览">
            <div style={{ display: 'flex', gap: 16, marginBottom: 24 }}>
              <Card size="small" style={{ flex: 1 }}>
                <Statistic title="Pending" value={Number(jobStats.pending ?? 0)} valueStyle={{ color: '#1677ff' }} prefix={<ExclamationCircleOutlined />} />
              </Card>
              <Card size="small" style={{ flex: 1 }}>
                <Statistic title="Running" value={Number(jobStats.running ?? 0)} valueStyle={{ color: '#fa8c16' }} prefix={<ScheduleOutlined />} />
              </Card>
              <Card size="small" style={{ flex: 1 }}>
                <Statistic title="Succeeded" value={Number(jobStats.succeeded ?? 0)} valueStyle={{ color: '#52c41a' }} prefix={<CheckCircleOutlined />} />
              </Card>
              <Card size="small" style={{ flex: 1 }}>
                <Statistic title="Failed" value={Number(jobStats.failed_terminal ?? 0)} valueStyle={{ color: '#ff4d4f' }} prefix={<CloseCircleOutlined />} />
              </Card>
            </div>
            <div style={{ display: 'flex', gap: 16 }}>
              <Card size="small" style={{ flex: 1 }}>
                <Statistic title="Proposals Pending" value={Number(proposalStats.pending ?? 0)} valueStyle={{ color: '#1677ff' }} prefix={<ExclamationCircleOutlined />} />
              </Card>
              <Card size="small" style={{ flex: 1 }}>
                <Statistic title="Approved" value={Number(proposalStats.approved ?? 0)} valueStyle={{ color: '#52c41a' }} prefix={<CheckCircleOutlined />} />
              </Card>
              <Card size="small" style={{ flex: 1 }}>
                <Statistic title="Rejected" value={Number(proposalStats.rejected ?? 0)} valueStyle={{ color: '#ff4d4f' }} prefix={<CloseCircleOutlined />} />
              </Card>
              <Card size="small" style={{ flex: 1 }}>
                <Statistic title="Executed" value={Number(proposalStats.executed ?? 0)} valueStyle={{ color: '#722ed1' }} prefix={<ScheduleOutlined />} />
              </Card>
            </div>
          </Card>
        </div>

        <div className="ai-grid-auto-lg">
          <Card className="ai-page-card" title="Approved Proposals">
            <Table
              size="small"
              dataSource={approvedProposals}
              rowKey="proposal_id"
              pagination={{ pageSize: 8 }}
              columns={[
                { title: 'Object', key: 'object', render: (_, row) => `${row.object_type}.${row.action_name}` },
                { title: 'Object ID', dataIndex: 'object_id', key: 'object_id', ellipsis: true },
                {
                  title: 'Action',
                  key: 'action',
                  render: (_, row) => (
                    <Button
                      size="small"
                      type="primary"
                      disabled={!canExecuteProposal(row, rolloutStatus)}
                      onClick={() => handleExecuteProposal(row.proposal_id)}
                    >
                      Execute
                    </Button>
                  ),
                },
              ]}
            />
          </Card>
        </div>
      </Space>
    </div>
  );
}
