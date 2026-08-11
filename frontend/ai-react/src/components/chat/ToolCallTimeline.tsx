import { Card, Empty, Space, Tag, Timeline, Typography } from 'antd';

export interface ToolTimelineItem {
  id: string;
  toolName: string;
  status: string;
  message?: string;
  time?: string;
}

function statusTone(status: string): string {
  const normalized = String(status || '').toLowerCase();
  if (normalized.includes('success')) return 'success';
  if (normalized.includes('error') || normalized.includes('fail')) return 'error';
  if (normalized.includes('approval')) return 'warning';
  return 'processing';
}

export function ToolCallTimeline(props: { items: ToolTimelineItem[]; title?: string }): JSX.Element {
  const { items, title = '工具调用轨迹' } = props;

  return (
    <Card title={title} size="small">
      {items.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无工具调用" />
      ) : (
        <Timeline
          items={items.map((item) => ({
            key: item.id,
            color: statusTone(item.status),
            children: (
              <Space direction="vertical" size={2}>
                <Space wrap>
                  <Typography.Text strong>{item.toolName || 'unknown_tool'}</Typography.Text>
                  <Tag color={statusTone(item.status)}>{item.status || 'in_progress'}</Tag>
                  {item.time ? <Typography.Text type="secondary">{item.time}</Typography.Text> : null}
                </Space>
                {item.message ? <Typography.Text type="secondary">{item.message}</Typography.Text> : null}
              </Space>
            ),
          }))}
        />
      )}
    </Card>
  );
}

