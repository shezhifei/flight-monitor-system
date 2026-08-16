import { Card, Empty, Space, Tag, Timeline, Typography } from 'antd';

export interface ToolTimelineItem {
  id: string;
  toolName: string;
  status: string;
  message?: string;
  time?: string;
  /** Governance rejection gate: snapshot | hook | acl | lease | budget */
  blockedBy?: string;
  /** Concrete rule id (hook class name, TOOL_NOT_IN_ALLOWED_SET, denial code, …) */
  rule?: string;
  /** One-line human-readable rejection detail */
  detail?: string;
}

function statusTone(status: string): string {
  const normalized = String(status || '').toLowerCase();
  if (normalized.includes('success')) return 'success';
  if (normalized.includes('error') || normalized.includes('fail')) return 'error';
  if (normalized.includes('approval')) return 'warning';
  return 'processing';
}

function rejectionSummary(item: ToolTimelineItem): string | null {
  if (!item.blockedBy && !item.rule && !item.detail) {
    return null;
  }
  const parts: string[] = [];
  if (item.blockedBy) {
    parts.push(`blocked_by=${item.blockedBy}`);
  }
  if (item.rule) {
    parts.push(`rule=${item.rule}`);
  }
  if (item.detail) {
    parts.push(item.detail);
  }
  return parts.join(' · ');
}

export function ToolCallTimeline(props: { items: ToolTimelineItem[]; title?: string }): JSX.Element {
  const { items, title = '工具调用轨迹' } = props;

  return (
    <Card title={title} size="small">
      {items.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无工具调用" />
      ) : (
        <Timeline
          items={items.map((item) => {
            const rejection = rejectionSummary(item);
            return {
              key: item.id,
              color: statusTone(item.status),
              children: (
                <Space direction="vertical" size={2}>
                  <Space wrap>
                    <Typography.Text strong>{item.toolName || 'unknown_tool'}</Typography.Text>
                    <Tag color={statusTone(item.status)}>{item.status || 'in_progress'}</Tag>
                    {item.blockedBy ? <Tag color="error">{item.blockedBy}</Tag> : null}
                    {item.time ? <Typography.Text type="secondary">{item.time}</Typography.Text> : null}
                  </Space>
                  {item.message ? <Typography.Text type="secondary">{item.message}</Typography.Text> : null}
                  {rejection ? (
                    <Typography.Text type="danger" style={{ fontSize: 12 }}>
                      {rejection}
                    </Typography.Text>
                  ) : null}
                </Space>
              ),
            };
          })}
        />
      )}
    </Card>
  );
}

