import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, Button, Card, Drawer, Form, Input, Select, Space, Switch, Table, Tag, Typography, message } from 'antd';
import { PlusOutlined, ReloadOutlined, EyeOutlined, StopOutlined, SafetyCertificateOutlined } from '@ant-design/icons';
import { AiPageNavigation } from '@/components/shell/AiPageNavigation';
import { cancelEvalJob, createEvalJob, getEvalJob, listEvalJobs } from '@/lib/api/llmEvalApi';
import type { EvalJobDetail, EvalJobSummary } from '@/lib/types/apiModels';

// Frozen G1 fixtures (docs/fixtures); custom paths are allowed.
const DATASET_OPTIONS = [
  { label: 'query_ops 基线 (agent_query_ops_eval.jsonl)', value: 'docs/fixtures/agent_query_ops_eval.jsonl' },
  { label: 'dispatch_ops 基线 (agent_dispatch_ops_eval.jsonl)', value: 'docs/fixtures/agent_dispatch_ops_eval.jsonl' },
];

const ACTIVE_STATUSES = new Set(['pending', 'running']);

export function LlmEvalLabPage(): JSX.Element {
  const [form] = Form.useForm();
  const [jobs, setJobs] = useState<EvalJobSummary[]>([]);
  const [currentJob, setCurrentJob] = useState<EvalJobDetail | null>(null);
  const [creating, setCreating] = useState(false);
  const [loadingJobs, setLoadingJobs] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refreshJobs = useCallback(async (): Promise<EvalJobSummary[]> => {
    setLoadingJobs(true);
    try {
      const rows = await listEvalJobs(30);
      setJobs(rows);
      return rows;
    } catch (error) {
      message.error(error instanceof Error ? error.message : '加载评测任务失败');
      return [];
    } finally {
      setLoadingJobs(false);
    }
  }, []);

  const refreshCurrentJob = useCallback(async (jobId: string): Promise<void> => {
    try {
      const detail = await getEvalJob(jobId);
      setCurrentJob(detail);
    } catch (error) {
      message.error(error instanceof Error ? error.message : '加载详情失败');
    }
  }, []);

  // Initial load + light polling while any job (or the open detail) is active.
  useEffect(() => {
    let cancelled = false;

    const tick = async (): Promise<void> => {
      const rows = await refreshJobs();
      if (cancelled) {
        return;
      }
      const hasActive =
        rows.some((job) => ACTIVE_STATUSES.has(job.status)) ||
        (currentJob !== null && ACTIVE_STATUSES.has(currentJob.status));
      if (hasActive) {
        if (currentJob !== null && ACTIVE_STATUSES.has(currentJob.status)) {
          await refreshCurrentJob(currentJob.job_id);
        }
        pollTimer.current = setTimeout(() => void tick(), 4000);
      }
    };

    void tick();
    return () => {
      cancelled = true;
      if (pollTimer.current !== null) {
        clearTimeout(pollTimer.current);
      }
    };
  }, [refreshJobs, refreshCurrentJob, currentJob]);

  const statusColor = (status: string): string => {
    switch (status) {
      case 'completed': return 'var(--ai-success)';
      case 'running': return 'var(--ai-brand)';
      case 'failed': return 'var(--ai-danger)';
      default: return 'var(--ai-muted)';
    }
  };

  const gates = currentJob?.gates ?? [];

  return (
    <div className="ai-page-shell">
      <Space direction="vertical" style={{ width: '100%' }} size={16}>
        {/* ---- Header ---- */}
        <div className="ai-toolbar ai-reveal">
          <Typography.Title level={3} style={{ margin: 0 }}>
            LLM 评测
          </Typography.Title>
          <Space>
            <Button icon={<PlusOutlined />} type="primary" onClick={() => setDrawerOpen(true)}>
              创建任务
            </Button>
            <Button icon={<ReloadOutlined />} onClick={() => void refreshJobs()} loading={loadingJobs}>
              刷新
            </Button>
          </Space>
        </div>

        {/* ---- Navigation ---- */}
        <AiPageNavigation currentKey="llm_eval_lab" />

        {/* ---- Job list (persistent ai_eval_jobs) ---- */}
        <Card className="ai-page-card ai-reveal ai-reveal-2">
          <Typography.Text strong style={{ fontSize: 13, textTransform: 'uppercase', letterSpacing: '0.04em', color: 'var(--ai-muted)', display: 'block', marginBottom: 12 }}>
            评测任务列表
          </Typography.Text>
          <div style={{ overflowX: 'auto' }}>
            <Table
              size="small"
              loading={loadingJobs}
              dataSource={jobs.map((job, idx) => ({ ...job, key: String(job.job_id || idx) }))}
              pagination={false}
              columns={[
                {
                  title: 'Job ID',
                  dataIndex: 'job_id',
                  width: 240,
                  render: (val: string) => (
                    <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 12 }}>{val}</span>
                  ),
                },
                {
                  title: '名称',
                  dataIndex: 'name',
                  width: 180,
                  render: (val: string) => <span style={{ fontSize: 12 }}>{val || '-'}</span>,
                },
                {
                  title: '数据集',
                  dataIndex: 'dataset_path',
                  render: (val: string) => (
                    <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 11, color: 'var(--ai-muted)' }}>{val || '-'}</span>
                  ),
                },
                {
                  title: '状态',
                  dataIndex: 'status',
                  width: 110,
                  render: (val: string) => (
                    <Tag style={{ borderColor: statusColor(val), color: statusColor(val), background: 'transparent', fontFamily: 'var(--ai-font-mono)', fontSize: 11 }}>
                      {val || 'unknown'}
                    </Tag>
                  ),
                },
                {
                  title: '进度',
                  key: 'progress',
                  width: 110,
                  render: (_: unknown, record: EvalJobSummary) => (
                    <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 11, color: 'var(--ai-muted)' }}>
                      {record.completed_runs ?? 0}/{record.total_runs ?? 0}
                    </span>
                  ),
                },
                {
                  title: '创建时间',
                  dataIndex: 'created_at',
                  width: 200,
                  render: (val: string) => <span style={{ fontSize: 12, color: 'var(--ai-muted)' }}>{val || '-'}</span>,
                },
                {
                  title: '操作',
                  key: 'actions',
                  width: 160,
                  render: (_: unknown, record: EvalJobSummary) => {
                    const jobId = String(record.job_id || '');
                    return (
                      <Space size={6}>
                        <Button
                          size="small"
                          icon={<EyeOutlined />}
                          onClick={() => void refreshCurrentJob(jobId)}
                        >
                          查看
                        </Button>
                        <Button
                          size="small"
                          danger
                          icon={<StopOutlined />}
                          disabled={!ACTIVE_STATUSES.has(record.status)}
                          onClick={async () => {
                            try {
                              await cancelEvalJob(jobId);
                              message.success('评测任务已取消');
                              await refreshJobs();
                            } catch (error) {
                              message.error(error instanceof Error ? error.message : '取消失败');
                            }
                          }}
                        >
                          取消
                        </Button>
                      </Space>
                    );
                  },
                },
              ]}
            />
          </div>
        </Card>

        {/* ---- Detail: status + evidence-coverage gate table ---- */}
        {currentJob ? (
          <Card className="ai-page-card ai-reveal ai-reveal-3">
            <div style={{ marginBottom: 16, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <Typography.Text strong style={{ fontSize: 13, textTransform: 'uppercase', letterSpacing: '0.04em', color: 'var(--ai-muted)' }}>
                  <SafetyCertificateOutlined style={{ marginRight: 6 }} />门禁结果
                </Typography.Text>
                <Tag
                  style={{
                    marginLeft: 10,
                    borderColor: statusColor(String(currentJob.status || '')),
                    color: statusColor(String(currentJob.status || '')),
                    background: 'transparent',
                    fontFamily: 'var(--ai-font-mono)',
                    fontSize: 11,
                  }}
                >
                  {String(currentJob.status || '')}
                </Tag>
              </div>
              <Typography.Text style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 11, color: 'var(--ai-muted)' }}>
                {String(currentJob.job_id || '')}
              </Typography.Text>
            </div>

            {currentJob.error_message ? (
              <Alert type="error" showIcon message={currentJob.error_message} style={{ marginBottom: 12 }} />
            ) : null}

            {gates.length > 0 ? (
              <Table
                size="small"
                pagination={false}
                dataSource={gates.map((gate, idx) => ({ ...gate, key: `${gate.metric_name}-${idx}` }))}
                columns={[
                  {
                    title: '门禁',
                    dataIndex: 'metric_name',
                    render: (val: string) => (
                      <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 12 }}>{val}</span>
                    ),
                  },
                  {
                    title: '实测值',
                    dataIndex: 'value',
                    width: 120,
                    render: (val: number) => (
                      <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 12 }}>{Number(val).toFixed(3)}</span>
                    ),
                  },
                  {
                    title: '阈值',
                    dataIndex: 'threshold',
                    width: 120,
                    render: (val: number) => (
                      <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 12, color: 'var(--ai-muted)' }}>
                        {Number(val).toFixed(2)}
                      </span>
                    ),
                  },
                  {
                    title: '结果',
                    dataIndex: 'status',
                    width: 100,
                    render: (val: string) => {
                      const color = val === 'pass' ? 'var(--ai-success)' : val === 'fail' ? 'var(--ai-danger)' : 'var(--ai-muted)';
                      return (
                        <Tag style={{ borderColor: color, color, background: 'transparent', fontFamily: 'var(--ai-font-mono)', fontSize: 11 }}>
                          {val}
                        </Tag>
                      );
                    },
                  },
                ]}
              />
            ) : (
              <Typography.Text type="secondary">
                尚无门禁记录——任务完成（或失败）后这里显示证据覆盖与工具策略门禁。
              </Typography.Text>
            )}
          </Card>
        ) : (
          <Card className="ai-page-card ai-reveal ai-reveal-3">
            <div style={{ padding: 24, textAlign: 'center' }}>
              <Typography.Text type="secondary">从上方列表中选择一个任务查看门禁结果</Typography.Text>
            </div>
          </Card>
        )}
      </Space>

      {/* ---- Create job drawer (dataset-based) ---- */}
      <Drawer
        rootClassName="ai-eval-lab-drawer"
        title="创建评测任务"
        placement="right"
        width={Math.min(460, typeof window !== 'undefined' ? window.innerWidth * 0.9 : 460)}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        styles={{
          body: { background: 'var(--ai-bg-subtle)' },
          header: { background: 'var(--ai-bg-subtle)', borderColor: 'var(--ai-border)' },
        }}
      >
        <Form
          layout="vertical"
          form={form}
          initialValues={{
            name: 'agent eval',
            dataset_path: DATASET_OPTIONS[0].value,
            run: true,
          }}
          onFinish={async (values) => {
            setCreating(true);
            try {
              const created = await createEvalJob({
                name: String(values.name || 'agent eval'),
                dataset_path: String(values.dataset_path || ''),
                description: String(values.description || ''),
                run: Boolean(values.run),
              });
              message.success(`评测任务已创建: ${created.job_id}`);
              setDrawerOpen(false);
              await refreshJobs();
              if (created.job_id) {
                await refreshCurrentJob(created.job_id);
              }
            } catch (error) {
              message.error(error instanceof Error ? error.message : '创建评测失败');
            } finally {
              setCreating(false);
            }
          }}
        >
          <Form.Item name="name" label="任务名" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="dataset_path" label="评测数据集 (JSONL)" rules={[{ required: true }]}>
            <Select options={DATASET_OPTIONS} showSearch allowClear placeholder="选择或输入数据集路径" />
          </Form.Item>
          <Form.Item name="description" label="描述（可选）">
            <Input.TextArea rows={2} />
          </Form.Item>
          <Form.Item name="run" label="创建后立即执行" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item>
            <Button type="primary" htmlType="submit" loading={creating} block>
              创建任务
            </Button>
          </Form.Item>
        </Form>
      </Drawer>
    </div>
  );
}
