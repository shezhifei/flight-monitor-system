import { useEffect, useMemo, useState } from 'react';
import { Alert, Button, Card, Drawer, Form, Input, InputNumber, Progress, Select, Space, Table, Tag, Typography, message } from 'antd';
import { PlusOutlined, ReloadOutlined, EyeOutlined, StopOutlined, BarChartOutlined } from '@ant-design/icons';
import { AiPageNavigation } from '@/components/shell/AiPageNavigation';
import { cancelEvalJob, compareEvalProfiles, createEvalJob, getEvalJob, listEvalJobs } from '@/lib/api/llmEvalApi';

const DEFAULT_EVAL_SUITE = 'quick';
const EVAL_SUITE_OPTIONS = [
  { label: '快速回归 (quick)', value: 'quick' },
  { label: '标准套件 (standard)', value: 'standard' },
  { label: '全量套件 (full)', value: 'full' },
  { label: '推理专项 (reasoning)', value: 'reasoning' },
  { label: 'Text2SQL 专项', value: 'text2sql' },
  { label: '人机协同 (human_in_the_loop)', value: 'human_in_the_loop' },
];

export function LlmEvalLabPage(): JSX.Element {
  const [form] = Form.useForm();
  const [jobs, setJobs] = useState<Array<Record<string, unknown>>>([]);
  const [currentJob, setCurrentJob] = useState<Record<string, unknown> | null>(null);
  const [creating, setCreating] = useState(false);
  const [compareResult, setCompareResult] = useState<Record<string, unknown> | null>(null);
  const [loadingJobs, setLoadingJobs] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const refreshJobs = async (): Promise<void> => {
    setLoadingJobs(true);
    try {
      const rows = await listEvalJobs(30);
      setJobs(rows as unknown as Array<Record<string, unknown>>);
    } catch (error) {
      message.error(error instanceof Error ? error.message : '加载评测任务失败');
    } finally {
      setLoadingJobs(false);
    }
  };

  useEffect(() => {
    void refreshJobs();
    form.setFieldsValue({
      profile_name: 'default_eval_profile',
      base_url: 'https://api.openai.com/v1',
      api_key: '',
      model: 'gpt-4o-mini',
      timeout: 30,
      max_retries: 2,
      retry_delay: 0.5,
      suite: DEFAULT_EVAL_SUITE,
      repeat_count: 2,
      profile_concurrency: 1,
      case_concurrency: 1,
    });
  }, []);

  const ranking = useMemo(() => {
    const rows = currentJob?.ranking;
    return Array.isArray(rows) ? (rows as Array<Record<string, unknown>>) : [];
  }, [currentJob]);

  const statusColor = (status: string): string => {
    switch (status) {
      case 'completed': return 'var(--ai-success)';
      case 'running': return 'var(--ai-brand)';
      case 'failed': return 'var(--ai-danger)';
      default: return 'var(--ai-muted)';
    }
  };

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
            <Button icon={<ReloadOutlined />} onClick={() => refreshJobs()} loading={loadingJobs}>
              刷新
            </Button>
          </Space>
        </div>

        {/* ---- Navigation ---- */}
        <AiPageNavigation currentKey="llm_eval_lab" />

        {/* ---- Job List as Table ---- */}
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
                  title: '状态',
                  dataIndex: 'status',
                  width: 120,
                  render: (val: string) => (
                    <Tag style={{ borderColor: statusColor(val), color: statusColor(val), background: 'transparent', fontFamily: 'var(--ai-font-mono)', fontSize: 11 }}>
                      {val || 'unknown'}
                    </Tag>
                  ),
                },
                {
                  title: '创建时间',
                  dataIndex: 'created_at',
                  render: (val: string) => <span style={{ fontSize: 12, color: 'var(--ai-muted)' }}>{val || '-'}</span>,
                },
                {
                  title: '操作',
                  key: 'actions',
                  width: 160,
                  render: (_: unknown, record: Record<string, unknown>) => {
                    const jobId = String(record.job_id || '');
                    return (
                      <Space size={6}>
                        <Button
                          size="small"
                          icon={<EyeOutlined />}
                          onClick={async () => {
                            try {
                              const detail = await getEvalJob(jobId);
                              setCurrentJob(detail as unknown as Record<string, unknown>);
                            } catch (error) {
                              message.error(error instanceof Error ? error.message : '加载详情失败');
                            }
                          }}
                        >
                          查看
                        </Button>
                        <Button
                          size="small"
                          danger
                          icon={<StopOutlined />}
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

        {/* ---- Results & Comparison ---- */}
        {currentJob ? (
          <Card className="ai-page-card ai-reveal ai-reveal-3">
            <div style={{ marginBottom: 16, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <Typography.Text strong style={{ fontSize: 13, textTransform: 'uppercase', letterSpacing: '0.04em', color: 'var(--ai-muted)' }}>
                  <BarChartOutlined style={{ marginRight: 6 }} />评测结果
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

            {/* Score Progress Bars */}
            {ranking.length > 0 && (
              <Space direction="vertical" style={{ width: '100%', marginBottom: 20 }} size={12}>
                {ranking.map((row, idx) => {
                  const finalScore = Number(row.final_score || 0);
                  const name = String(row.name || row.profile_id || `Profile ${idx + 1}`);
                  return (
                    <div key={String(row.profile_id || idx)}>
                      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                        <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 12, color: 'var(--ai-text-heading)' }}>
                          #{idx + 1} {name}
                        </span>
                        <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 12, color: 'var(--ai-muted)' }}>
                          {String(row.model || '')}
                        </span>
                      </div>
                      <Progress
                        percent={Math.round(finalScore * 100)}
                        strokeColor={idx === 0 ? 'var(--ai-brand)' : 'var(--ai-muted)'}
                        trailColor="var(--ai-surface-raised)"
                        format={(pct) => <span style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 11, color: 'var(--ai-text)' }}>{pct}%</span>}
                      />
                      <div style={{ display: 'flex', gap: 16, marginTop: 2, fontSize: 11, color: 'var(--ai-muted)', fontFamily: 'var(--ai-font-mono)' }}>
                        <span>Generalization: {String(row.generalization_score || '-')}</span>
                        <span>Stability: {String(row.stability_score || '-')}</span>
                      </div>
                    </div>
                  );
                })}
              </Space>
            )}

            {/* Compare Button */}
            <Space>
              <Button
                icon={<BarChartOutlined />}
                onClick={async () => {
                  if (ranking.length < 2) {
                    message.warning('至少需要两组配置才能对比');
                    return;
                  }
                  const left = String(ranking[0].profile_id || '');
                  const right = String(ranking[1].profile_id || '');
                  const jobId = String(currentJob.job_id || '');
                  try {
                    const payload = await compareEvalProfiles(jobId, left, right);
                    setCompareResult(payload);
                  } catch (error) {
                    message.error(error instanceof Error ? error.message : '对比失败');
                  }
                }}
              >
                对比前两名
              </Button>
            </Space>

            {compareResult ? (
              <pre style={{ marginTop: 12 }}>{JSON.stringify(compareResult, null, 2)}</pre>
            ) : null}
          </Card>
        ) : (
          <Card className="ai-page-card ai-reveal ai-reveal-3">
            <div style={{ padding: 24, textAlign: 'center' }}>
              <Typography.Text type="secondary">从上方列表中选择一个任务查看结果</Typography.Text>
            </div>
          </Card>
        )}
      </Space>

      {/* ---- Create Job Drawer ---- */}
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
          onFinish={async (values) => {
            setCreating(true);
            try {
              const profileName = String(values.profile_name || 'profile_1');
              const payload = {
                profiles: [
                  {
                    profile_id: profileName,
                    name: profileName,
                    base_url: String(values.base_url || ''),
                    api_key: String(values.api_key || ''),
                    model: String(values.model || ''),
                    timeout: Number(values.timeout || 30),
                    max_retries: Number(values.max_retries || 2),
                    retry_delay: Number(values.retry_delay || 0.5),
                    api_mode: String(values.api_mode || 'chat'),
                  },
                ],
                options: {
                  suite: String(values.suite || DEFAULT_EVAL_SUITE),
                  repeat_count: Number(values.repeat_count || 2),
                  profile_concurrency: Number(values.profile_concurrency || 1),
                  case_concurrency: Number(values.case_concurrency || 1),
                  enable_tool_routing: true,
                },
              };
              const created = await createEvalJob(payload);
              message.success(`评测任务已创建: ${created.job_id}`);
              setDrawerOpen(false);
              await refreshJobs();
            } catch (error) {
              message.error(error instanceof Error ? error.message : '创建评测失败');
            } finally {
              setCreating(false);
            }
          }}
        >
          <Form.Item name="profile_name" label="配置名" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="base_url" label="Base URL" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="api_key" label="API Key" rules={[{ required: true }]}>
            <Input.Password />
          </Form.Item>
          <Form.Item name="model" label="模型" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(100px, 1fr))', gap: 10 }}>
            <Form.Item name="timeout" label="超时(s)">
              <InputNumber min={1} max={300} style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item name="max_retries" label="重试次数">
              <InputNumber min={0} max={10} style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item name="retry_delay" label="延迟(s)">
              <InputNumber min={0} max={10} step={0.1} style={{ width: '100%' }} />
            </Form.Item>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(120px, 1fr))', gap: 10 }}>
            <Form.Item name="suite" label="评测套件">
              <Select options={EVAL_SUITE_OPTIONS} />
            </Form.Item>
            <Form.Item name="repeat_count" label="重复次数">
              <InputNumber min={1} max={20} style={{ width: '100%' }} />
            </Form.Item>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(120px, 1fr))', gap: 10 }}>
            <Form.Item name="profile_concurrency" label="配置并发">
              <InputNumber min={1} max={20} style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item name="case_concurrency" label="Case 并发">
              <InputNumber min={1} max={20} style={{ width: '100%' }} />
            </Form.Item>
          </div>
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
