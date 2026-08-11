import { useCallback, useMemo, useState } from 'react';
import type { ComponentType } from 'react';
import { Bubble, Sender } from '@ant-design/x';
import { Button, Collapse, Space, Typography } from 'antd';
import { ToolOutlined, BulbOutlined, ExclamationCircleOutlined } from '@ant-design/icons';
import { InsightRenderer } from '@/components/chat/InsightRenderer';
import { PendingActionCard, PendingActionCardModel } from '@/components/chat/PendingActionCard';
import { ToolCallTimeline, ToolTimelineItem } from '@/components/chat/ToolCallTimeline';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
}

export function AiChatShell(props: {
  testId?: string;
  title: string;
  subtitle?: string;
  messages: ChatMessage[];
  inputValue: string;
  sending?: boolean;
  onInputChange: (value: string) => void;
  onSend: (value: string) => void;
  onClear?: () => void;
  toolItems?: ToolTimelineItem[];
  pendingActions?: PendingActionCardModel[];
  onApproveAction?: (actionId: string) => Promise<void> | void;
  onRejectAction?: (actionId: string) => Promise<void> | void;
  insightMarkdown?: string;
  insightJson?: unknown;
}): JSX.Element {
  const {
    testId,
    title,
    messages,
    inputValue,
    sending = false,
    onInputChange,
    onSend,
    onClear,
    toolItems = [],
    pendingActions = [],
    onApproveAction,
    onRejectAction,
    insightMarkdown,
    insightJson,
  } = props;

  const bubbleItems = useMemo(
    () =>
      messages.map((message) => ({
        key: message.id,
        role: message.role === 'assistant' ? 'x' : 'user',
        content: message.content || '-',
      })),
    [messages],
  );

  const SenderAny = Sender as unknown as ComponentType<Record<string, unknown>>;
  const BubbleListAny = Bubble.List as unknown as ComponentType<Record<string, unknown>>;

  const [busyActionId, setBusyActionId] = useState<string | null>(null);

  const hasToolItems = toolItems.length > 0;
  const hasInsight = Boolean(insightMarkdown || insightJson);
  const hasPending = pendingActions.length > 0 && onApproveAction && onRejectAction;

  const handleApprove = useCallback(async (actionId: string) => {
    if (!onApproveAction) return;
    setBusyActionId(actionId);
    try {
      await onApproveAction(actionId);
    } finally {
      setBusyActionId((prev) => (prev === actionId ? null : prev));
    }
  }, [onApproveAction]);

  const handleReject = useCallback(async (actionId: string) => {
    if (!onRejectAction) return;
    setBusyActionId(actionId);
    try {
      await onRejectAction(actionId);
    } finally {
      setBusyActionId((prev) => (prev === actionId ? null : prev));
    }
  }, [onRejectAction]);

  // Build collapse items for auxiliary panels
  const collapseItems = useMemo(() => {
    const items: Array<{ key: string; label: React.ReactNode; children: React.ReactNode }> = [];
    if (hasToolItems) {
      items.push({
        key: 'tools',
        label: (
          <span style={{ fontSize: 12, color: 'var(--ai-muted)', textTransform: 'uppercase', letterSpacing: '0.04em' }}>
            <ToolOutlined style={{ marginRight: 6 }} />
            工具调用轨迹 ({toolItems.length})
          </span>
        ),
        children: <ToolCallTimeline items={toolItems} />,
      });
    }
    if (hasInsight) {
      items.push({
        key: 'insight',
        label: (
          <span style={{ fontSize: 12, color: 'var(--ai-muted)', textTransform: 'uppercase', letterSpacing: '0.04em' }}>
            <BulbOutlined style={{ marginRight: 6 }} />
            洞察结果
          </span>
        ),
        children: <InsightRenderer markdown={insightMarkdown} jsonPayload={insightJson} />,
      });
    }
    return items;
  }, [hasToolItems, hasInsight, toolItems, insightMarkdown, insightJson]);

  return (
    <div className="ai-chat-immersive" data-testid={testId}>
      {/* ---- Chat Messages ---- */}
      <div className="ai-chat-immersive__messages">
        <BubbleListAny
          items={bubbleItems as unknown as Record<string, unknown>[]}
          role={
            {
              x: { placement: 'start' },
              user: { placement: 'end' },
            } as unknown as Record<string, unknown>
          }
        />
      </div>

      {/* ---- Pending Actions (float above input when present) ---- */}
      {hasPending ? (
        <Space direction="vertical" size={8} style={{ width: '100%', marginBottom: 8 }}>
          {pendingActions.map((action) => (
            <PendingActionCard
              key={action.actionId}
              action={action}
              busy={action.actionId === busyActionId}
              onApprove={handleApprove}
              onReject={handleReject}
            />
          ))}
        </Space>
      ) : null}

      {/* ---- Input Area ---- */}
      <div className="ai-chat-immersive__input">
        <SenderAny
          value={inputValue}
          onChange={onInputChange}
          onSubmit={onSend}
          loading={sending}
          placeholder="输入问题并发送"
          data-testid={testId ? `${testId}-sender` : undefined}
        />
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
          {onClear ? (
            <Button size="small" onClick={onClear}>清空</Button>
          ) : null}
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            Enter 发送，Shift+Enter 换行
          </Typography.Text>
        </div>
      </div>

      {/* ---- Collapsible Auxiliary Panels ---- */}
      {collapseItems.length > 0 ? (
        <div style={{ marginTop: 16 }}>
          <Collapse
            ghost
            size="small"
            items={collapseItems}
            defaultActiveKey={hasInsight ? ['insight'] : []}
            style={{
              background: 'var(--ai-surface)',
              borderRadius: 'var(--ai-radius)',
              border: '1px solid var(--ai-border)',
            }}
          />
        </div>
      ) : null}
    </div>
  );
}
