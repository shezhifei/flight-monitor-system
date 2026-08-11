import { Alert, Button, Card, Space, Typography } from 'antd';

export interface PendingActionCardModel {
  actionId: string;
  toolName?: string;
  status?: string;
  message?: string;
  createdAt?: string;
  expiresAt?: string;
}

export function PendingActionCard(props: {
  action: PendingActionCardModel;
  busy?: boolean;
  onApprove: (actionId: string) => Promise<void> | void;
  onReject: (actionId: string) => Promise<void> | void;
}): JSX.Element {
  const { action, busy = false, onApprove, onReject } = props;
  return (
    <Card size="small" title={action.toolName || action.actionId}>
      <Space direction="vertical" style={{ width: '100%' }} size={8}>
        {action.message ? <Alert type="warning" message={action.message} showIcon /> : null}
        <Typography.Text type="secondary">状态: {action.status || 'pending'}</Typography.Text>
        {action.createdAt ? <Typography.Text type="secondary">创建: {action.createdAt}</Typography.Text> : null}
        {action.expiresAt ? <Typography.Text type="secondary">过期: {action.expiresAt}</Typography.Text> : null}
        <Space>
          <Button type="primary" loading={busy} onClick={() => onApprove(action.actionId)}>
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

