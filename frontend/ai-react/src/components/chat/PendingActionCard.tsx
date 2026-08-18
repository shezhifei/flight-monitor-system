import { Alert, Button, Card, Space, Table, Tag, Typography } from 'antd';
import type { PendingActionCardModel, PendingActionConstraint, PendingActionDiffRow } from '@/components/chat/pendingActionDiff';

export type { PendingActionCardModel } from '@/components/chat/pendingActionDiff';

function constraintText(item: PendingActionConstraint): string {
  return item.message ? `${item.name}: ${item.message}` : item.name;
}

export function PendingActionCard(props: {
  action: PendingActionCardModel;
  busy?: boolean;
  onApprove: (actionId: string) => Promise<void> | void;
  onReject: (actionId: string) => Promise<void> | void;
}): JSX.Element {
  const { action, busy = false, onApprove, onReject } = props;
  const hardViolations = Array.isArray(action.hardViolations) ? action.hardViolations : [];
  const softViolations = Array.isArray(action.softViolations) ? action.softViolations : [];
  const diffRows = Array.isArray(action.diffRows) ? action.diffRows : [];
  const hasHardViolations = hardViolations.length > 0;

  const diffColumns = [
    { title: '字段', dataIndex: 'field', key: 'field', width: 110 },
    { title: '变更前', dataIndex: 'before', key: 'before', ellipsis: true },
    { title: '变更后', dataIndex: 'after', key: 'after', ellipsis: true },
  ];

  return (
    <Card
      size="small"
      title={
        <Space size={6} wrap>
          <span>{action.toolName || action.actionId}</span>
          {action.irreversible ? <Tag color="red">不可逆操作</Tag> : null}
          {hasHardViolations ? <Tag color="red">硬约束违规</Tag> : null}
        </Space>
      }
    >
      <Space direction="vertical" style={{ width: '100%' }} size={8}>
        {action.message ? <Alert type="warning" message={action.message} showIcon /> : null}

        {action.objectType || action.objectId ? (
          <Typography.Text type="secondary">
            对象: {action.objectType || 'Unknown'} / {action.objectId || '-'}
          </Typography.Text>
        ) : null}

        {hardViolations.length > 0 ? (
          <Alert
            type="error"
            showIcon
            message="硬约束违规"
            description={
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                {hardViolations.map((item, index) => (
                  <span key={`${item.name}_${index}`}>{constraintText(item)}</span>
                ))}
              </Space>
            }
          />
        ) : null}

        {softViolations.length > 0 ? (
          <Alert
            type="warning"
            showIcon
            message="软约束提示"
            description={
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                {softViolations.map((item, index) => (
                  <span key={`${item.name}_${index}`}>{constraintText(item)}</span>
                ))}
              </Space>
            }
          />
        ) : null}

        {diffRows.length > 0 ? (
          <Table<PendingActionDiffRow>
            size="small"
            rowKey={(row) => row.field}
            columns={diffColumns}
            dataSource={diffRows}
            pagination={false}
            scroll={{ x: 'max-content' }}
          />
        ) : null}

        <Typography.Text type="secondary">状态: {action.status || 'pending'}</Typography.Text>
        {action.sourceRunId || action.sourceTool ? (
          <Typography.Text type="secondary">
            来源: {[action.sourceTool, action.sourceRunId].filter(Boolean).join(' / ')}
          </Typography.Text>
        ) : null}
        {action.createdAt ? <Typography.Text type="secondary">创建: {action.createdAt}</Typography.Text> : null}
        {action.expiresAt ? <Typography.Text type="secondary">过期: {action.expiresAt}</Typography.Text> : null}
        <Space>
          <Button
            type="primary"
            danger={hasHardViolations}
            loading={busy}
            onClick={() => onApprove(action.actionId)}
          >
            批准
          </Button>
          <Button danger loading={busy} onClick={() => onReject(action.actionId)}>
            拒绝
          </Button>
        </Space>
      </Space>
    </Card>
  );
}
