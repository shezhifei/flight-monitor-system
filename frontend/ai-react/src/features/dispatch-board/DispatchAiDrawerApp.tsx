import { useEffect, useMemo, useRef, useState } from 'react';
import { Badge, Button, Card, Drawer, FloatButton, Input, Select, Space, Statistic, Table, Tabs, Tag, Typography, message } from 'antd';
import { RobotOutlined } from '@ant-design/icons';
import { AiChatShell, ChatMessage } from '@/components/chat/AiChatShell';
import { applyReplan, loadDispatchConflicts, previewReplan, type DispatchReplanRequest } from '@/lib/api/dispatchApi';
import { listProposals, approveProposal, rejectProposal } from '@/lib/api/aiApi';
import { hasPermission } from '@/lib/auth/authBridge';
import { streamQuery } from '@/lib/api/nlQueryApi';
import { createRequestId, normalizeTime } from '@/lib/utils';

interface DispatchConflictRow {
  key: string;
  message: string;
  severity: string;
  conflictType: string;
  resource: string;
  orderIds: string[];
}

function normalizeConflicts(rows: Array<Record<string, unknown>>): DispatchConflictRow[] {
  return rows.map((row, index) => {
    const orderIdsRaw = Array.isArray(row.related_dispatch_order_ids) ? row.related_dispatch_order_ids : [];
    const orderIds = orderIdsRaw.map((item) => String(item || '').trim()).filter(Boolean);
    const key = String(row.id || row.conflict_id || `${index}_${Date.now()}`);
    return {
      key,
      message: String(row.message || '检测到冲突'),
      severity: String(row.severity || 'medium').toLowerCase(),
      conflictType: String(row.conflict_type || 'unknown'),
      resource: String(row.resource_name || row.resource_id || '-'),
      orderIds,
    };
  });
}

export function DispatchAiDrawerApp(): JSX.Element {
  const [open, setOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<'assistant' | 'conflict'>('assistant');
  const [objective, setObjective] = useState('clear_pending');
  const [question, setQuestion] = useState('');
  const [sending, setSending] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [toolItems, setToolItems] = useState<Array<{ id: string; toolName: string; status: string; message?: string; time?: string }>>([]);
  const [conflicts, setConflicts] = useState<DispatchConflictRow[]>([]);
  const [severityFilter, setSeverityFilter] = useState('all');
  const [queryFilter, setQueryFilter] = useState('');
  const [conflictsLoading, setConflictsLoading] = useState(false);
  const [replanLoading, setReplanLoading] = useState(false);
  const [replanPreviewRows, setReplanPreviewRows] = useState<Array<Record<string, unknown>>>([]);
  const [replanPreviewRequest, setReplanPreviewRequest] = useState<DispatchReplanRequest | null>(null);
  const [contextPayload, setContextPayload] = useState<Record<string, unknown> | null>(null);
  const [replanProposals, setReplanProposals] = useState<Array<Record<string, unknown>>>([]);
  const [replanProposalsLoading, setReplanProposalsLoading] = useState(false);

  const canView = hasPermission('dispatch:view') || hasPermission('dispatch:manage') || hasPermission('ai:view');
  const canChat = hasPermission('ai:chat') || hasPermission('dispatch:manage');
  const canExecute = hasPermission('ai:execute') || hasPermission('dispatch:manage');

  const appendMessage = (row: ChatMessage): void => {
    setMessages((prev) => [...prev.slice(-119), row]);
  };

  const upsertMessage = (id: string, content: string): void => {
    setMessages((prev) => prev.map((row) => (row.id === id ? { ...row, content } : row)));
  };

  const refreshConflicts = async (): Promise<void> => {
    setConflictsLoading(true);
    try {
      const rows = await loadDispatchConflicts();
      setConflicts(normalizeConflicts(rows));
    } catch (error) {
      message.error(error instanceof Error ? error.message : '冲突列表加载失败');
    } finally {
      setConflictsLoading(false);
    }
  };

  const loadReplanProposals = async (): Promise<void> => {
    setReplanProposalsLoading(true);
    try {
      const result = await listProposals({ object_type: 'DispatchOrder', action_name: 'recommend_replan', status: 'pending' });
      setReplanProposals(result as Array<Record<string, unknown>>);
    } catch (error) {
      message.error(error instanceof Error ? error.message : '重排建议加载失败');
    } finally {
      setReplanProposalsLoading(false);
    }
  };

  const filteredConflicts = useMemo(() => {
    return conflicts.filter((row) => {
      if (severityFilter !== 'all' && row.severity !== severityFilter) {
        return false;
      }
      if (!queryFilter.trim()) {
        return true;
      }
      const query = queryFilter.trim().toLowerCase();
      const blob = `${row.message} ${row.resource} ${row.conflictType} ${row.orderIds.join(' ')}`.toLowerCase();
      return blob.includes(query);
    });
  }, [conflicts, queryFilter, severityFilter]);

  const sendAssistantQuestion = async (value: string): Promise<void> => {
    const content = String(value || '').trim();
    if (!content || sending) {
      return;
    }
    if (!canChat) {
      message.warning('当前账号缺少 ai:chat 权限');
      return;
    }
    setSending(true);
    setQuestion('');
    appendMessage({ id: `u_${Date.now()}`, role: 'user', content });
    const assistantId = `a_${Date.now()}`;
    appendMessage({ id: assistantId, role: 'assistant', content: '正在生成调度建议...' });
    let assistantText = '';
    try {
      const result = await streamQuery(
        {
          question: `目标(${objective}): ${content}`,
          request_id: createRequestId('dispatch_ai'),
          context: {
            source_page: 'dispatch_board',
            scope_mode: 'dispatch',
            ...(contextPayload || {}),
          },
        },
        (eventName, payload) => {
          const semantic = String(payload.event || eventName || '').toLowerCase();
          if (semantic === 'text_delta') {
            assistantText = `${assistantText}${String(payload.delta || payload.text || '')}`;
            upsertMessage(assistantId, assistantText || '正在生成调度建议...');
            return;
          }
          if (semantic.includes('tool')) {
            setToolItems((prev) => [
              ...prev.slice(-79),
              {
                id: createRequestId('tool'),
                toolName: String(payload.tool_name || 'dispatch_tool'),
                status: String(payload.status || semantic),
                message: String(payload.message || semantic),
                time: normalizeTime(new Date().toISOString()),
              },
            ]);
          }
        },
      );
      upsertMessage(assistantId, String(result.summary || '建议已生成'));
      setToolItems((prev) => [
        ...prev.slice(-79),
        {
          id: createRequestId('tool'),
          toolName: 'dispatch_assistant',
          status: 'success',
          message: '建议生成完成',
          time: normalizeTime(new Date().toISOString()),
        },
      ]);
    } catch (error) {
      const text = error instanceof Error ? error.message : '建议生成失败';
      upsertMessage(assistantId, `失败: ${text}`);
      setToolItems((prev) => [
        ...prev.slice(-79),
        {
          id: createRequestId('tool'),
          toolName: 'dispatch_assistant',
          status: 'error',
          message: text,
          time: normalizeTime(new Date().toISOString()),
        },
      ]);
    } finally {
      setSending(false);
    }
  };
  const sendAssistantQuestionRef = useRef(sendAssistantQuestion);
  const refreshConflictsRef = useRef(refreshConflicts);

  useEffect(() => {
    sendAssistantQuestionRef.current = sendAssistantQuestion;
    refreshConflictsRef.current = refreshConflicts;
  });

  useEffect(() => {
    const bridge = {
      openDrawer: (
        tab: 'assistant' | 'conflict' = 'assistant',
        options?: { refresh?: boolean; context?: Record<string, unknown> },
      ) => {
        setOpen(true);
        setActiveTab(tab === 'conflict' ? 'conflict' : 'assistant');
        if (options?.context && typeof options.context === 'object') {
          setContextPayload(options.context);
        }
        if (tab === 'conflict' && options?.refresh !== false) {
          void refreshConflictsRef.current();
        }
      },
      closeDrawer: () => setOpen(false),
      setActiveTab: (tab: 'assistant' | 'conflict', options?: { refresh?: boolean }) => {
        const normalized = tab === 'conflict' ? 'conflict' : 'assistant';
        setActiveTab(normalized);
        if (normalized === 'conflict' && options?.refresh !== false) {
          void refreshConflictsRef.current();
        }
      },
      setContext: (payload: Record<string, unknown>) => {
        setContextPayload(payload || null);
      },
      sendQuestion: async (content: string) => {
        try {
          setOpen(true);
          setActiveTab('assistant');
          await sendAssistantQuestionRef.current(content);
        } catch (error) {
          const text = error instanceof Error ? error.message : '发送问题失败';
          message.error(text);
          console.error('Dispatch AI bridge sendQuestion failed:', error);
        }
      },
    };
    window.DISPATCH_AI_BRIDGE = bridge;
    return () => {
      if (window.DISPATCH_AI_BRIDGE === bridge) {
        delete window.DISPATCH_AI_BRIDGE;
      }
    };
  }, []);

  return (
    <>
      <Badge count={filteredConflicts.length > 99 ? '99+' : filteredConflicts.length} offset={[-2, 5]}>
        <FloatButton
          icon={<RobotOutlined />}
          type="primary"
          tooltip="派工 AI"
          style={{ right: 22, bottom: 146 }}
          onClick={() => setOpen(true)}
        />
      </Badge>

      <Drawer
        open={open}
        width={640}
        onClose={() => setOpen(false)}
        title={
          <Space>
            <Typography.Text>智能排班与冲突治理</Typography.Text>
            {!canView ? <Tag color="warning">缺少 ai:view/dispatch:view</Tag> : null}
          </Space>
        }
        extra={
          <Button
            onClick={() => {
              setMessages([]);
              setToolItems([]);
              setReplanPreviewRows([]);
              setReplanPreviewRequest(null);
              setReplanProposals([]);
            }}
          >
            清空
          </Button>
        }
      >
        <Tabs
          activeKey={activeTab}
          onChange={(tab) => setActiveTab(tab === 'conflict' ? 'conflict' : 'assistant')}
          items={[
            {
              key: 'assistant',
              label: '智能建议',
              children: (
                <Space direction="vertical" style={{ width: '100%' }} size={12}>
                  {!canChat ? (
                    <Card size="small">
                      <Typography.Text type="secondary">
                        当前账号缺少 `ai:chat` 权限，助手输入已禁用。
                      </Typography.Text>
                    </Card>
                  ) : null}
                  <Card size="small">
                    <Space wrap style={{ width: '100%' }}>
                      <Select
                        value={objective}
                        style={{ width: 220 }}
                        onChange={setObjective}
                        options={[
                          { label: '优先清空待派工', value: 'clear_pending' },
                          { label: '优先消解资源冲突', value: 'resolve_conflicts' },
                          { label: '优先均衡负载', value: 'balance_load' },
                          { label: '优先预防延误', value: 'delay_prevention' },
                        ]}
                      />
                      <Input
                        value={question}
                        onChange={(event) => setQuestion(event.target.value)}
                        onPressEnter={() => sendAssistantQuestion(question)}
                        placeholder="输入调度目标，例如：优先处理 2 小时内出港航班"
                        disabled={!canChat || sending}
                      />
                      <Button type="primary" loading={sending} disabled={!canChat} onClick={() => sendAssistantQuestion(question)}>
                        生成建议
                      </Button>
                    </Space>
                  </Card>
                  <AiChatShell
                    testId="dispatch-chat-shell"
                    title="Dispatch AI"
                    messages={messages}
                    inputValue={question}
                    onInputChange={setQuestion}
                    onSend={sendAssistantQuestion}
                    sending={sending}
                    onClear={() => { setMessages([]); setToolItems([]); }}
                    toolItems={toolItems}
                    insightMarkdown=""
                  />
                  <Card title="重排建议" size="small">
                    <Space direction="vertical" style={{ width: '100%' }} size={8}>
                      <Button loading={replanProposalsLoading} onClick={loadReplanProposals}>
                        加载建议
                      </Button>
                      {replanProposals.length > 0 ? (
                        replanProposals.map((proposal) => {
                          const proposalId = String(proposal.id || proposal.proposal_id || '');
                          const hasRealId = Boolean(proposal.id || proposal.proposal_id);
                          const objectId = String(proposal.object_id || '-');
                          const reasoning = String(proposal.reasoning || proposal.reason || '-');
                          const confidence = typeof proposal.confidence === 'number' ? `${Math.round(proposal.confidence * 100)}%` : '-';
                          return (
                            <Card key={proposalId || `idx_${objectId}`} size="small">
                              <Space direction="vertical" style={{ width: '100%' }} size={4}>
                                <Typography.Text strong>工单ID: {objectId}</Typography.Text>
                                <Typography.Text>{reasoning}</Typography.Text>
                                <Typography.Text type="secondary">置信度: {confidence}</Typography.Text>
                                {canExecute && hasRealId ? (
                                  <Space>
                                    <Button
                                      size="small"
                                      type="primary"
                                      onClick={async () => {
                                        try {
                                          await approveProposal(proposalId);
                                          setReplanProposals((prev) => prev.filter((p) => String(p.id || p.proposal_id || '') !== proposalId));
                                          message.success('已批准');
                                          await refreshConflicts();
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
                                      onClick={async () => {
                                        try {
                                          await rejectProposal(proposalId, '人工拒绝');
                                          setReplanProposals((prev) => prev.filter((p) => String(p.id || p.proposal_id || '') !== proposalId));
                                          message.info('已拒绝');
                                          await refreshConflicts();
                                        } catch (error) {
                                          message.error(error instanceof Error ? error.message : '拒绝失败');
                                        }
                                      }}
                                    >
                                      拒绝
                                    </Button>
                                  </Space>
                                ) : null}
                              </Space>
                            </Card>
                          );
                        })
                      ) : (
                        <Typography.Text type="secondary">暂无待审批重排建议</Typography.Text>
                      )}
                    </Space>
                  </Card>
                </Space>
              ),
            },
            {
              key: 'conflict',
              label: `冲突治理 (${filteredConflicts.length})`,
              children: (
                <Space direction="vertical" style={{ width: '100%' }} size={12}>
                  {!canExecute ? (
                    <Card size="small">
                      <Typography.Text type="secondary">
                        当前账号缺少 `ai:execute` 权限，冲突重排操作已禁用。
                      </Typography.Text>
                    </Card>
                  ) : null}
                  <Card size="small">
                    <Space wrap>
                      <Select
                        value={severityFilter}
                        onChange={setSeverityFilter}
                        options={[
                          { label: '全部级别', value: 'all' },
                          { label: 'Critical', value: 'critical' },
                          { label: 'High', value: 'high' },
                          { label: 'Medium', value: 'medium' },
                          { label: 'Low', value: 'low' },
                        ]}
                      />
                      <Input
                        value={queryFilter}
                        onChange={(event) => setQueryFilter(event.target.value)}
                        placeholder="搜索资源 / 冲突描述 / 工单ID"
                      />
                      <Button onClick={() => refreshConflicts()} loading={conflictsLoading}>
                        刷新冲突
                      </Button>
                      <Button
                        onClick={async () => {
                          setReplanLoading(true);
                          try {
                            const request: DispatchReplanRequest = {
                              strategy: 'balanced',
                              max_suggestions: 20,
                              scope: contextPayload || undefined,
                            };
                            const payload = await previewReplan(request);
                            const rows = Array.isArray(payload.suggestions) ? (payload.suggestions as Array<Record<string, unknown>>) : [];
                            setReplanPreviewRows(rows);
                            setReplanPreviewRequest(request);
                            message.success(`重排预览完成: ${rows.length} 条建议`);
                          } catch (error) {
                            message.error(error instanceof Error ? error.message : '预览失败');
                            setReplanPreviewRows([]);
                            setReplanPreviewRequest(null);
                          } finally {
                            setReplanLoading(false);
                          }
                        }}
                        loading={replanLoading}
                        disabled={!canExecute}
                      >
                        预览重排
                      </Button>
                      <Button
                        type="primary"
                        disabled={!canExecute || replanPreviewRows.length === 0 || !replanPreviewRequest}
                        loading={replanLoading}
                        onClick={async () => {
                          if (!replanPreviewRequest) {
                            message.warning('请先生成重排预览');
                            return;
                          }
                          setReplanLoading(true);
                          try {
                            await applyReplan(replanPreviewRequest);
                            message.success('重排已应用');
                            setReplanPreviewRows([]);
                            setReplanPreviewRequest(null);
                            await refreshConflicts();
                          } catch (error) {
                            if (error instanceof Error && error.message.includes('409')) {
                              message.warning('重排冲突，请刷新后重试');
                            } else {
                              message.error(error instanceof Error ? error.message : '应用失败');
                            }
                          } finally {
                            setReplanLoading(false);
                          }
                        }}
                      >
                        应用重排
                      </Button>
                    </Space>
                  </Card>

                  <Space wrap>
                    <Card size="small" style={{ minWidth: 160 }}>
                      <Statistic title="冲突总数" value={conflicts.length} />
                    </Card>
                    <Card size="small" style={{ minWidth: 160 }}>
                      <Statistic
                        title="高优先级"
                        value={conflicts.filter((row) => row.severity === 'critical' || row.severity === 'high').length}
                      />
                    </Card>
                    <Card size="small" style={{ minWidth: 160 }}>
                      <Statistic title="重排预览" value={replanPreviewRows.length} />
                    </Card>
                  </Space>

                  <Table
                    data-testid="dispatch-conflicts-table"
                    rowKey="key"
                    size="small"
                    loading={conflictsLoading}
                    dataSource={filteredConflicts}
                    pagination={{ pageSize: 20, showSizeChanger: false }}
                    scroll={{ y: 280 }}
                    columns={[
                      {
                        title: '冲突描述',
                        dataIndex: 'message',
                        ellipsis: true,
                      },
                      {
                        title: '级别',
                        dataIndex: 'severity',
                        width: 110,
                        render: (value: string) => (
                          <Tag color={value === 'critical' ? 'red' : value === 'high' ? 'orange' : value === 'medium' ? 'gold' : 'default'}>
                            {value}
                          </Tag>
                        ),
                      },
                      {
                        title: '类型',
                        dataIndex: 'conflictType',
                        width: 150,
                      },
                      {
                        title: '资源',
                        dataIndex: 'resource',
                        width: 140,
                        ellipsis: true,
                      },
                      {
                        title: '关联工单',
                        dataIndex: 'orderIds',
                        render: (value: string[]) => (value.length > 0 ? value.join(', ') : '-'),
                      },
                    ]}
                  />
                </Space>
              ),
            },
          ]}
        />
      </Drawer>
    </>
  );
}
