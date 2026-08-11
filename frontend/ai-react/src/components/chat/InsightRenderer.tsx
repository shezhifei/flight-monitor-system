import { Card, Empty, Typography } from 'antd';

export function InsightRenderer(props: {
  title?: string;
  markdown?: string;
  jsonPayload?: unknown;
}): JSX.Element {
  const { title = '洞察结果', markdown, jsonPayload } = props;
  const hasMarkdown = Boolean(markdown && markdown.trim());
  const hasJson = jsonPayload !== null && jsonPayload !== undefined;

  return (
    <Card size="small" title={title}>
      {!hasMarkdown && !hasJson ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无洞察结果" /> : null}
      {hasMarkdown ? (
        <Typography.Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 12 }}>
          {markdown}
        </Typography.Paragraph>
      ) : null}
      {hasJson ? (
        <Typography.Paragraph>
          <pre style={{ margin: 0, whiteSpace: 'pre-wrap' }}>{JSON.stringify(jsonPayload, null, 2)}</pre>
        </Typography.Paragraph>
      ) : null}
    </Card>
  );
}

