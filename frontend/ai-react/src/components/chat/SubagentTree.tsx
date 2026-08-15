import { Card, Empty, Space, Tag, Typography } from 'antd';
import { ApartmentOutlined } from '@ant-design/icons';
import type { SubagentNodeModel, SubagentNodeStatus } from '@/components/chat/subagentTreeModel';

function statusTone(status: SubagentNodeStatus): string {
  if (status === 'done') return 'success';
  if (status === 'error') return 'error';
  return 'processing';
}

function statusLabel(status: SubagentNodeStatus): string {
  if (status === 'done') return '已完成';
  if (status === 'error') return '失败';
  return '运行中';
}

export function SubagentTree(props: { nodes: SubagentNodeModel[]; title?: string }): JSX.Element {
  const { nodes, title = '子代理树' } = props;

  return (
    <Card title={title} size="small" data-testid="subagent-tree">
      {nodes.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无子代理" />
      ) : (
        <Space direction="vertical" size={6} style={{ width: '100%' }}>
          {nodes.map((node) => (
            <div
              key={node.id}
              style={{
                paddingLeft: Math.max(0, node.depth - 1) * 20,
                fontSize: 12,
                lineHeight: '22px',
              }}
            >
              <Space size={6} wrap>
                <ApartmentOutlined style={{ color: 'var(--ai-muted)' }} />
                <Typography.Text strong style={{ fontSize: 12 }}>{node.label || node.id}</Typography.Text>
                <Tag color={statusTone(node.status)} style={{ fontSize: 11 }}>{statusLabel(node.status)}</Tag>
                {node.proposalOnly ? <Tag color="warning" style={{ fontSize: 11 }}>proposal_only</Tag> : null}
                {node.toolCalls > 0 ? (
                  <Typography.Text type="secondary" style={{ fontSize: 11 }}>
                    工具调用 {node.toolCalls}
                  </Typography.Text>
                ) : null}
                {node.lastActivity ? (
                  <Typography.Text type="secondary" style={{ fontSize: 11, fontFamily: 'var(--ai-font-mono)' }}>
                    {node.lastActivity}
                  </Typography.Text>
                ) : null}
              </Space>
            </div>
          ))}
        </Space>
      )}
    </Card>
  );
}
