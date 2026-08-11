import { useState } from 'react';
import { Card, Empty, Select, Space, Steps, Tag, Typography } from 'antd';

export interface EvidenceItem {
  object_type: string;
  object_id: string;
  field?: string;
  source: string;
}

export interface ReasoningStep {
  step: string;
  summary: string;
}

interface EvidencePanelProps {
  evidence?: EvidenceItem[];
  reasoningSteps?: ReasoningStep[];
}

export function EvidencePanel(props: EvidencePanelProps): JSX.Element {
  const { evidence = [], reasoningSteps = [] } = props;
  const [typeFilter, setTypeFilter] = useState<string>('all');

  const objectTypes = Array.from(new Set(evidence.map((e) => e.object_type))).sort();

  const filteredEvidence = typeFilter === 'all'
    ? evidence
    : evidence.filter((e) => e.object_type === typeFilter);

  return (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      {/* Evidence Section */}
      <Card size="small" title="Evidence 来源">
        {evidence.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无 Evidence 数据" />
        ) : (
          <Space direction="vertical" size={8} style={{ width: '100%' }}>
            <Select
              value={typeFilter}
              onChange={setTypeFilter}
              style={{ width: 200 }}
              options={[
                { label: '全部类型', value: 'all' },
                ...objectTypes.map((t) => ({ label: t, value: t })),
              ]}
            />
            {filteredEvidence.map((item, idx) => (
              <div
                key={`${item.object_type}_${item.object_id}_${idx}`}
                style={{
                  padding: '8px 12px',
                  borderRadius: 6,
                  background: 'var(--ai-surface-raised, #f5f5f5)',
                  border: '1px solid var(--ai-border, #e0e0e0)',
                }}
              >
                <Space size={8} wrap>
                  <Tag color="blue">{item.object_type}</Tag>
                  <Typography.Text code style={{ fontSize: 12 }}>{item.object_id}</Typography.Text>
                  {item.field ? <Tag>{item.field}</Tag> : null}
                  <Typography.Text type="secondary" style={{ fontSize: 11 }}>
                    {item.source}
                  </Typography.Text>
                </Space>
              </div>
            ))}
          </Space>
        )}
      </Card>

      {/* Reasoning Steps Section */}
      {reasoningSteps.length > 0 ? (
        <Card size="small" title="推理步骤">
          <Steps
            direction="vertical"
            size="small"
            current={reasoningSteps.length - 1}
            items={reasoningSteps.map((rs) => ({
              title: rs.step,
              description: rs.summary,
            }))}
          />
        </Card>
      ) : null}
    </Space>
  );
}
