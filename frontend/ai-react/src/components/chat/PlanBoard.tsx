import { Card, Empty, Steps, Tag, Typography } from 'antd';
import type { PlanBoardModel, PlanStepStatus } from '@/components/chat/planBoardModel';

function stepTone(status: PlanStepStatus): 'wait' | 'process' | 'finish' | 'error' {
  if (status === 'in_progress') return 'process';
  if (status === 'done') return 'finish';
  if (status === 'blocked') return 'error';
  return 'wait';
}

function statusLabel(status: PlanStepStatus): string {
  if (status === 'in_progress') return '进行中';
  if (status === 'done') return '已完成';
  if (status === 'blocked') return '受阻';
  return '待执行';
}

export function PlanBoard(props: { board: PlanBoardModel; title?: string }): JSX.Element {
  const { board, title = '执行计划' } = props;

  return (
    <Card title={title} size="small" data-testid="plan-board">
      {board.steps.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无计划步骤" />
      ) : (
        <>
          {board.description ? (
            <Typography.Paragraph type="secondary" style={{ fontSize: 12, marginBottom: 12 }}>
              {board.description}
            </Typography.Paragraph>
          ) : null}
          <Steps
            direction="vertical"
            size="small"
            current={board.steps.findIndex((step) => step.status === 'in_progress')}
            items={board.steps.map((step) => ({
              key: step.id,
              status: stepTone(step.status),
              title: (
                <span style={{ fontSize: 13 }}>
                  {step.description || step.id}
                  {step.assignedTo ? (
                    <Tag style={{ marginLeft: 6, fontSize: 11 }}>{step.assignedTo}</Tag>
                  ) : null}
                </span>
              ),
              description: (
                <span style={{ fontSize: 11 }}>
                  <Tag color={step.status === 'done' ? 'success' : step.status === 'blocked' ? 'error' : step.status === 'in_progress' ? 'processing' : 'default'}>
                    {statusLabel(step.status)}
                  </Tag>
                  {step.error ? (
                    <Typography.Text type="danger" style={{ fontSize: 11 }}>{step.error}</Typography.Text>
                  ) : null}
                </span>
              ),
            }))}
          />
        </>
      )}
    </Card>
  );
}
