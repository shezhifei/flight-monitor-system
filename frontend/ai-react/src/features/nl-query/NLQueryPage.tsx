import { useEffect, useMemo, useRef, useState } from 'react';
import { Button, Card, Select, Space, Steps, Tag, Timeline, Typography, message } from 'antd';
import { BulbOutlined, FileSearchOutlined, PlusOutlined, DeleteOutlined, MessageOutlined } from '@ant-design/icons';
import { AiChatShell, ChatMessage } from '@/components/chat/AiChatShell';
import { AiPageNavigation } from '@/components/shell/AiPageNavigation';
import { applyPlanToolEvent, type PlanBoardModel } from '@/components/chat/planBoardModel';
import { toCompressionNotice, type CompressionNoticeModel } from '@/components/chat/runResume';
import {
  applyDelegateToolEvent,
  applySubagentStreamEvent,
  type SubagentNodeModel,
} from '@/components/chat/subagentTreeModel';
import { approvePendingAction, rejectPendingAction } from '@/lib/api/aiApi';
import { deleteConversation, listConversationMessages, listConversations, listSuggestions, streamQuery } from '@/lib/api/nlQueryApi';
import { resolveEntryFeatures } from '@/lib/shell/entryRegistry';
import { EventSourceClient } from '@/lib/sse/eventSourceClient';
import { ExecutionPollFallback } from '@/lib/sse/executionPollFallback';
import { createRequestId, normalizeTime } from '@/lib/utils';
import { upsertToolTimelineEntry } from '@/features/nl-query/toolTimeline';

interface ConversationItem {
  id: string;
  title: string;
  updatedAt?: string;
}

function normalizeConversationUpdatedAt(rawValue?: string): string {
  return normalizeTime(rawValue || new Date().toISOString());
}

function normalizeConversationId(row: Record<string, unknown>): string {
  return String(row.conversation_id || row.id || '').trim();
}

function normalizeConversationTitle(row: Record<string, unknown>, conversationId: string): string {
  return String(row.title || conversationId || '未命名会话');
}

function toChatMessage(row: Record<string, unknown>): ChatMessage {
  const messageIndex = String(row.message_index ?? Date.now());
  const role = String(row.role || '').toLowerCase();
  const normalizedRole: ChatMessage['role'] =
    role === 'user' || role === 'assistant' || role === 'system'
      ? role
      : 'assistant';

  return {
    id: `history_${messageIndex}`,
    role: normalizedRole,
    content: String(row.content_text || row.content_raw || ''),
  };
}

function buildConversationPreviewTitle(rawQuestion: string, fallbackTitle?: string): string {
  const normalizedFallback = String(fallbackTitle || '').trim();
  if (normalizedFallback && normalizedFallback !== '未命名会话') {
    return normalizedFallback;
  }

  const normalizedQuestion = String(rawQuestion || '').trim().replace(/\s+/g, ' ');
  if (!normalizedQuestion) {
    return normalizedFallback || '未命名会话';
  }

  return normalizedQuestion.length > 36
    ? `${normalizedQuestion.slice(0, 36)}...`
    : normalizedQuestion;
}

export function NLQueryPage(): JSX.Element {
  const [messagesState, setMessagesState] = useState<ChatMessage[]>([]);
  const [inputValue, setInputValue] = useState('');
  const [sending, setSending] = useState(false);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [conversations, setConversations] = useState<ConversationItem[]>([]);
  const [suggestions, setSuggestions] = useState<Array<{ label?: string; text?: string }>>([]);
  const [toolItems, setToolItems] = useState<Array<{ id: string; toolName: string; status: string; message?: string; time?: string }>>([]);
  const [pendingActions, setPendingActions] = useState<Array<{ actionId: string; toolName?: string; status?: string; message?: string }>>([]);
  const [insightText, setInsightText] = useState('');
  const [insightJson, setInsightJson] = useState<unknown>(null);
  const [evidenceObjectTypeFilter, setEvidenceObjectTypeFilter] = useState<string>('all');
  const [sidebarExpanded, setSidebarExpanded] = useState(false);
  const [planBoard, setPlanBoard] = useState<PlanBoardModel | null>(null);
  const [subagents, setSubagents] = useState<SubagentNodeModel[]>([]);
  const [compressionNotice, setCompressionNotice] = useState<CompressionNoticeModel | null>(null);
  const [resumableRun, setResumableRun] = useState<{ runId: string; jobId?: string } | null>(null);
  const activeAssistantIdRef = useRef<string | null>(null);
  const activeExecutionIdRef = useRef<string>('');
  const activeRunIdRef = useRef<string>('');
  const activeJobIdRef = useRef<string>('');
  const pollFallbackRef = useRef<ExecutionPollFallback | null>(null);
  const historyLoadedRef = useRef(false);
  const features = useMemo(() => resolveEntryFeatures('nl_query'), []);

  const resetConversationView = (): void => {
    setMessagesState([]);
    setInsightText('');
    setInsightJson(null);
    setToolItems([]);
    setPendingActions([]);
    setPlanBoard(null);
    setSubagents([]);
    setCompressionNotice(null);
    setResumableRun(null);
  };

  // Fold one runtime payload (nl-query stream or broadcast stream) into the
  // playground panels. Event contracts:
  // - tool.call / tool.result frames (tool_name, arguments?, result_status?)
  //   feed the plan board and the delegate/handoff nodes of the subagent tree.
  // - subagent_event frames bubble child-run activity.
  // - context.compressed frames surface the compression notice.
  const ingestPlaygroundEvent = (semantic: string, record: Record<string, unknown>): void => {
    const runId = String(record.run_id || '').trim();
    if (runId) activeRunIdRef.current = runId;
    const jobId = String(record.job_id || '').trim();
    if (jobId) activeJobIdRef.current = jobId;

    if (semantic === 'subagent_event' || semantic === 'subagent.event') {
      if (features.has('subagent-tree')) {
        setSubagents((prev) => applySubagentStreamEvent(prev, record));
      }
      return;
    }
    if (semantic === 'context.compressed' || semantic === 'context_compressed') {
      if (features.has('compression-notice')) {
        setCompressionNotice(toCompressionNotice(record));
      }
      return;
    }
    if (semantic.includes('tool')) {
      const toolName = String(record.tool_name || '').trim();
      if (!toolName) return;
      const phase = semantic.includes('result') ? 'result' : 'call';
      const status = String(record.result_status || record.status || '');
      if (features.has('plan-board')) {
        setPlanBoard((prev) => {
          const next = applyPlanToolEvent(prev, { toolName, phase, status, args: record.arguments, result: record.result });
          return next === undefined ? prev : next;
        });
      }
      if (features.has('subagent-tree')) {
        setSubagents((prev) => applyDelegateToolEvent(prev, { toolName, phase, status, args: record.arguments }) ?? prev);
      }
    }
    if (features.has('run-resume') && (semantic === 'run.fail' || semantic === 'run.cancelled' || semantic === 'cancelled')) {
      const failedRunId = activeRunIdRef.current;
      if (failedRunId) {
        setResumableRun({ runId: failedRunId, jobId: activeJobIdRef.current || undefined });
      }
    }
  };

  const appendMessage = (messageItem: ChatMessage): void => {
    setMessagesState((prev) => [...prev.slice(-119), messageItem]);
  };

  const upsertAssistantText = (id: string, text: string): void => {
    setMessagesState((prev) =>
      prev.map((item) => (item.id === id ? { ...item, content: text } : item)),
    );
  };

  const refreshHistory = async (): Promise<void> => {
    try {
      const rows = await listConversations(50, 0);
      historyLoadedRef.current = true;
      setConversations(
        rows
          .filter((row) => String(row.status || '').toLowerCase() !== 'ended')
          .map((row) => {
            const id = normalizeConversationId(row);
            return {
              id,
              title: normalizeConversationTitle(row, id),
              updatedAt: normalizeTime(String(row.updated_at || row.last_activity_at || '')),
            };
          })
          .filter((item) => item.id),
      );
    } catch (error) {
      console.error('Failed to refresh history', error);
      message.error(error instanceof Error ? error.message : '加载历史会话失败');
    }
  };

  const upsertConversationSummary = (
    nextConversationId: string,
    rawQuestion: string,
    fallbackTitle?: string,
  ): void => {
    const normalizedId = String(nextConversationId || '').trim();
    if (!normalizedId) {
      return;
    }

    const updatedAt = normalizeConversationUpdatedAt();
    const title = buildConversationPreviewTitle(rawQuestion, fallbackTitle);
    setConversations((prev) => {
      const nextItem: ConversationItem = {
        id: normalizedId,
        title,
        updatedAt,
      };
      const current = prev.find((item) => item.id === normalizedId);
      const merged = current
        ? {
            ...current,
            title: current.title || nextItem.title,
            updatedAt,
          }
        : nextItem;

      return [merged, ...prev.filter((item) => item.id !== normalizedId)];
    });
  };

  const loadConversation = async (targetConversationId: string): Promise<void> => {
    const id = String(targetConversationId || '').trim();
    if (!id) {
      return;
    }

    const rows = await listConversationMessages(id, 100, 0, 'asc');
    setConversationId(id);
    resetConversationView();
    setMessagesState(rows.map(toChatMessage).filter((item) => item.content.trim()));
  };

  useEffect(() => {
    listSuggestions()
      .then((rows) => setSuggestions(rows))
      .catch((error) => {
        setSuggestions([]);
        message.warning(error instanceof Error ? error.message : '推荐问题加载失败');
      });
  }, []);

  useEffect(() => {
    if (!sidebarExpanded || historyLoadedRef.current) {
      return;
    }
    void refreshHistory();
  }, [sidebarExpanded]);

  useEffect(() => {
    const client = new EventSourceClient({
      url: '/api/v2/ai/events/stream',
      eventNames: ['ai_execution'],
      clientScope: 'nl_query_react',
      onEvent: (_eventName, payload) => {
        const runtime = payload.payload && typeof payload.payload === 'object'
          ? (payload.payload as Record<string, unknown>)
          : payload;
        const runtimeRecord = runtime && typeof runtime === 'object'
          ? runtime as Record<string, unknown>
          : {};
        const semantic = String(runtimeRecord.event || payload.type || '').toLowerCase();
        ingestPlaygroundEvent(semantic, runtimeRecord);
        const action = runtimeRecord.pending_action && typeof runtimeRecord.pending_action === 'object'
          ? (runtimeRecord.pending_action as Record<string, unknown>)
          : null;
        if (semantic === 'approval_required' && action) {
          setPendingActions((prev) => {
            const actionId = String(action.action_id || '');
            const filtered = prev.filter((item) => item.actionId !== actionId);
            return [
              ...filtered,
              {
                actionId,
                toolName: String(action.tool_name || ''),
                status: String(action.status || 'pending'),
                message: String(action.message || ''),
              },
            ];
          });
        }
        if (semantic === 'approval_result' && action) {
          const actionId = String(action.action_id || '');
          setPendingActions((prev) => prev.filter((item) => item.actionId !== actionId));
        }
        const executionId = String(runtimeRecord.execution_id || payload.execution_id || '').trim();
        if (executionId) {
          activeExecutionIdRef.current = executionId;
        }
      },
      onError: () => {
        const executionId = activeExecutionIdRef.current;
        if (!executionId || pollFallbackRef.current) {
          return;
        }
        message.warning('实时结果流断开，已切换为轮询模式');
        const poller = new ExecutionPollFallback(executionId, (detail) => {
          const status = String(detail.status || 'in_progress').toLowerCase();
          setToolItems((prev) => upsertToolTimelineEntry(prev, {
            id: createRequestId('tool'),
            toolName: String(detail.tool_name || 'execution_poll'),
            status,
            message: String(detail.message || `SSE 中断，已切换轮询 (${status})`),
            time: new Date().toLocaleTimeString(),
          }));
        });
        pollFallbackRef.current = poller;
        poller.start();
      },
    });
    client.connect();
    return () => {
      client.disconnect();
      if (pollFallbackRef.current) {
        pollFallbackRef.current.stop();
        pollFallbackRef.current = null;
      }
    };
  }, []);

  const sendQuestion = async (value: string): Promise<void> => {
    const question = String(value || '').trim();
    if (!question || sending) {
      return;
    }
    setSending(true);
    setInputValue('');
    setInsightText('');
    setInsightJson(null);
    setPlanBoard(null);
    setSubagents([]);
    setCompressionNotice(null);
    setResumableRun(null);
    activeRunIdRef.current = '';
    activeJobIdRef.current = '';
    appendMessage({ id: `u_${Date.now()}`, role: 'user', content: question });
    const assistantId = `a_${Date.now()}`;
    activeAssistantIdRef.current = assistantId;
    appendMessage({ id: assistantId, role: 'assistant', content: '正在分析中...' });
    let assistantText = '';
    setToolItems((prev) => [...prev.slice(-79), { id: createRequestId('tool'), toolName: 'nl_query', status: 'in_progress', message: '请求已发送', time: new Date().toLocaleTimeString() }]);

    try {
      const requestId = createRequestId('nl');
      const requestConversationId = conversationId || createRequestId('conversation');
      const result = await streamQuery(
        {
          question,
          request_id: requestId,
          conversation_id: requestConversationId,
          context: {
            source_page: 'nl_query',
            scope_mode: 'global',
          },
        },
        (eventName, payload) => {
          const semantic = String(payload.event || eventName || '').toLowerCase();
          const executionId = String(payload.execution_id || '').trim();
          if (executionId) {
            activeExecutionIdRef.current = executionId;
          }
          ingestPlaygroundEvent(semantic, payload);
          if (semantic === 'text_delta') {
            const delta = String(payload.delta || payload.text || '');
            assistantText = `${assistantText}${delta}`;
            upsertAssistantText(assistantId, assistantText || '正在分析中...');
            return;
          }
          if (semantic.includes('tool')) {
            setToolItems((prev) => upsertToolTimelineEntry(prev, {
              id: createRequestId('tool'),
              toolName: String(payload.tool_name || 'tool'),
              status: String(payload.status || semantic),
              message: String(payload.message || payload.error || semantic),
              time: new Date().toLocaleTimeString(),
              blockedBy: payload.blocked_by ? String(payload.blocked_by) : undefined,
              rule: payload.rule ? String(payload.rule) : undefined,
              detail: payload.detail ? String(payload.detail) : undefined,
            }));
          }
        },
      );
      const resolvedConversationId = String(result.conversation_id || requestConversationId).trim();
      setConversationId(resolvedConversationId);
      upsertConversationSummary(
        resolvedConversationId,
        question,
        conversations.find((item) => item.id === resolvedConversationId)?.title,
      );
      upsertAssistantText(assistantId, result.summary || '已完成查询');
      setInsightText(String(result.summary || ''));
      setInsightJson(result.structured_data || null);
      setToolItems((prev) => upsertToolTimelineEntry(prev, {
        id: createRequestId('tool'),
        toolName: 'nl_query',
        status: 'success',
        message: '查询完成',
        time: new Date().toLocaleTimeString(),
      }));
    } catch (error) {
      const msg = error instanceof Error ? error.message : '查询失败';
      upsertAssistantText(assistantId, `查询失败：${msg}`);
      setToolItems((prev) => upsertToolTimelineEntry(prev, {
        id: createRequestId('tool'),
        toolName: 'nl_query',
        status: 'error',
        message: msg,
        time: new Date().toLocaleTimeString(),
      }));
      if (features.has('run-resume') && activeRunIdRef.current) {
        setResumableRun({ runId: activeRunIdRef.current, jobId: activeJobIdRef.current || undefined });
      }
    } finally {
      setSending(false);
      activeAssistantIdRef.current = null;
      if (pollFallbackRef.current) {
        pollFallbackRef.current.stop();
        pollFallbackRef.current = null;
      }
      activeExecutionIdRef.current = '';
    }
  };

  const pendingCards = useMemo(
    () =>
      pendingActions.map((item) => ({
        actionId: item.actionId,
        toolName: item.toolName,
        status: item.status,
        message: item.message,
      })),
    [pendingActions],
  );

  const structuredOutput = insightJson as Record<string, unknown> | null;
  const evidenceItems = (Array.isArray(structuredOutput?.evidence) ? structuredOutput.evidence : []) as Array<Record<string, string>>;
  const reasoningSteps = (Array.isArray(structuredOutput?.reasoning_steps) ? structuredOutput.reasoning_steps : []) as Array<Record<string, string>>;
  const filteredEvidence = evidenceObjectTypeFilter === 'all' ? evidenceItems : evidenceItems.filter((e) => e.object_type === evidenceObjectTypeFilter);
  const uniqueObjectTypes = [...new Set(evidenceItems.map((e) => e.object_type))];

  return (
    <div className="ai-page-shell">
      <Space direction="vertical" size={16} style={{ width: '100%' }}>
        {/* ---- Header ---- */}
        <div className="ai-toolbar ai-reveal">
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              智能查询
            </Typography.Title>
            {conversationId
              ? <Tag style={{ fontFamily: 'var(--ai-font-mono)', fontSize: 10 }} color="blue">{conversationId}</Tag>
              : <Tag style={{ fontSize: 10 }}>新会话</Tag>
            }
          </div>
          <Space>
            <Button
              icon={<PlusOutlined />}
              onClick={() => {
                setConversationId(null);
                resetConversationView();
              }}
            >
              新会话
            </Button>
            <Button
              onClick={() => setSidebarExpanded(!sidebarExpanded)}
              icon={<MessageOutlined />}
            >
              {sidebarExpanded ? '收起历史' : '历史会话'}
            </Button>
          </Space>
        </div>

        {/* ---- Navigation ---- */}
        <AiPageNavigation currentKey="nl-query" />

        {/* ---- Main Area ---- */}
        <div
          className="ai-reveal ai-reveal-2"
          style={{
            display: 'grid',
            gridTemplateColumns: sidebarExpanded ? 'minmax(220px, 280px) 1fr' : '1fr',
            gap: 16,
            transition: 'grid-template-columns 0.35s cubic-bezier(0.22,1,0.36,1)',
          }}
        >
          {/* ---- Stealth Sidebar ---- */}
          {sidebarExpanded && (
            <div
              className="ai-page-card"
              style={{ padding: 12, overflow: 'hidden', maxHeight: 'calc(100vh - 240px)', overflowY: 'auto' }}
            >
              {/* Suggestions as Pills */}
              {suggestions.length > 0 && (
                <div style={{ marginBottom: 16 }}>
                  <Typography.Text style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--ai-muted)', fontWeight: 600, display: 'block', marginBottom: 8 }}>
                    推荐提问
                  </Typography.Text>
                  <Space wrap size={[6, 6]}>
                    {suggestions.map((item, idx) => (
                      <Tag
                        key={`${item.label || item.text || 's'}_${idx}`}
                        style={{
                          cursor: 'pointer',
                          borderColor: 'var(--ai-border-strong)',
                          background: 'var(--ai-surface-raised)',
                          color: 'var(--ai-text)',
                          borderRadius: 20,
                          padding: '4px 12px',
                          fontSize: 12,
                          transition: 'all 150ms ease',
                        }}
                        onClick={() => setInputValue(String(item.text || item.label || ''))}
                      >
                        {item.label || item.text || `建议 ${idx + 1}`}
                      </Tag>
                    ))}
                  </Space>
                </div>
              )}

              {/* Conversation History */}
              <Typography.Text style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--ai-muted)', fontWeight: 600, display: 'block', marginBottom: 8 }}>
                历史会话
              </Typography.Text>
              <div className="ai-island-list">
                {conversations.map((item) => (
                  <div
                    key={item.id}
                    className={`ai-island-item${conversationId === item.id ? ' is-active' : ''}`}
                    style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 4 }}
                  >
                    <div
                      style={{ flex: 1, minWidth: 0, cursor: 'pointer' }}
                      onClick={async () => {
                        try {
                          await loadConversation(item.id);
                        } catch (error) {
                          message.error(error instanceof Error ? error.message : '加载历史会话失败');
                        }
                      }}
                    >
                      <div style={{ fontSize: 12, fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {item.title}
                      </div>
                      <div style={{ fontSize: 10, color: 'var(--ai-muted)' }}>{item.updatedAt || '-'}</div>
                    </div>
                    <Button
                      type="text"
                      size="small"
                      danger
                      icon={<DeleteOutlined style={{ fontSize: 11 }} />}
                      onClick={async (e) => {
                        e.stopPropagation();
                        try {
                          await deleteConversation(item.id);
                          if (conversationId === item.id) {
                            setConversationId(null);
                            resetConversationView();
                          }
                          setConversations((prev) => prev.filter((conversation) => conversation.id !== item.id));
                        } catch (error) {
                          message.error(error instanceof Error ? error.message : '删除失败');
                        }
                      }}
                    />
                  </div>
                ))}
                {conversations.length === 0 && (
                  <Typography.Text type="secondary" style={{ fontSize: 12, padding: 12, display: 'block' }}>暂无历史会话</Typography.Text>
                )}
              </div>
            </div>
          )}

          {/* ---- Chat Main Area ---- */}
          <div className={sending ? 'ai-glow-border' : ''} style={{ borderRadius: 'var(--ai-radius)' }}>
            <AiChatShell
              testId="nl-query-chat-shell"
              title="NL Query"
              messages={messagesState}
              inputValue={inputValue}
              sending={sending}
              onInputChange={setInputValue}
              onSend={sendQuestion}
              onClear={() => resetConversationView()}
              toolItems={toolItems}
              pendingActions={pendingCards}
              onApproveAction={async (actionId) => {
                try {
                  await approvePendingAction(actionId);
                  setPendingActions((prev) => prev.filter((item) => item.actionId !== actionId));
                } catch (error) {
                  message.error(error instanceof Error ? error.message : '批准失败');
                }
              }}
              onRejectAction={async (actionId) => {
                try {
                  await rejectPendingAction(actionId, 'nl_query_react_reject');
                  setPendingActions((prev) => prev.filter((item) => item.actionId !== actionId));
                } catch (error) {
                  message.error(error instanceof Error ? error.message : '拒绝失败');
                }
              }}
              insightMarkdown={insightText}
              insightJson={insightJson}
              planBoard={features.has('plan-board') ? planBoard : null}
              subagents={features.has('subagent-tree') ? subagents : []}
              compressionNotice={features.has('compression-notice') ? compressionNotice : null}
              resumableRun={features.has('run-resume') ? resumableRun : null}
              onRunResumed={() => setResumableRun(null)}
              onRunCancelled={() => setResumableRun(null)}
            />
          </div>
        </div>

        {reasoningSteps.length > 0 && (
          <Card
            title={<><BulbOutlined style={{ marginRight: 8 }} />推理步骤</>}
            size="small"
            className="ai-reveal ai-reveal-3"
          >
            <Steps
              direction="vertical"
              size="small"
              current={reasoningSteps.length}
              items={reasoningSteps.map((step, idx) => ({
                key: String(idx),
                title: step.step || `步骤 ${idx + 1}`,
                description: step.summary || '',
              }))}
            />
          </Card>
        )}

        {evidenceItems.length > 0 && (
          <Card
            title={<><FileSearchOutlined style={{ marginRight: 8 }} />证据来源</>}
            size="small"
            className="ai-reveal ai-reveal-3"
            extra={
              <Select
                value={evidenceObjectTypeFilter}
                onChange={setEvidenceObjectTypeFilter}
                size="small"
                style={{ width: 140 }}
                options={[
                  { value: 'all', label: '全部类型' },
                  ...uniqueObjectTypes.map((t) => ({ value: t, label: t })),
                ]}
              />
            }
          >
            <Timeline
              items={filteredEvidence.map((item, idx) => ({
                key: idx,
                children: (
                  <div style={{ fontSize: 12, lineHeight: '20px' }}>
                    <Tag color="blue" style={{ fontSize: 11 }}>{item.object_type}</Tag>
                    <span style={{ fontFamily: 'var(--ai-font-mono)', color: 'var(--ai-text)' }}>{item.object_id}</span>
                    {item.field && (
                      <span style={{ color: 'var(--ai-muted)', marginLeft: 8 }}>字段: {item.field}</span>
                    )}
                    {item.source && (
                      <div style={{ color: 'var(--ai-muted)', marginTop: 2 }}>来源: {item.source}</div>
                    )}
                  </div>
                ),
              }))}
            />
          </Card>
        )}
      </Space>
    </div>
  );
}
