import { useCallback, useEffect, useState } from 'react';
import { Alert, Button, Card, Space, Tag, Typography, message } from 'antd';
import { PlayCircleOutlined, StopOutlined } from '@ant-design/icons';
import { cancelAiJob, listAiRunCheckpoints, resumeAiRun } from '@/lib/api/aiApi';
import { latestRecoverableCheckpoint, normalizeCheckpoint, type RunCheckpointItem } from '@/components/chat/runResume';

/**
 * Resume entry for an interrupted / recoverable run (Task C5).
 *
 * Uses the existing control-plane routes only:
 * - POST /api/v2/ai/runs/{run_id}/resume
 * - GET  /api/v2/ai/jobs/{job_id}/runs/{run_id}/checkpoints (latest checkpoint label)
 * - DELETE /api/v2/ai/jobs/{job_id} (cancel via command queue)
 */
export function RunResumeBar(props: {
  runId: string;
  jobId?: string;
  onResumed?: () => void;
  onCancelled?: () => void;
}): JSX.Element {
  const { runId, jobId, onResumed, onCancelled } = props;
  const [checkpoints, setCheckpoints] = useState<RunCheckpointItem[]>([]);
  const [busy, setBusy] = useState<'resume' | 'cancel' | null>(null);

  useEffect(() => {
    if (!jobId || !runId) {
      setCheckpoints([]);
      return;
    }
    let cancelled = false;
    listAiRunCheckpoints(jobId, runId)
      .then((rows) => {
        if (!cancelled) {
          setCheckpoints(rows.map(normalizeCheckpoint));
        }
      })
      .catch(() => {
        if (!cancelled) {
          setCheckpoints([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [jobId, runId]);

  const latest = latestRecoverableCheckpoint(checkpoints);

  const handleResume = useCallback(async () => {
    setBusy('resume');
    try {
      await resumeAiRun(runId, latest?.checkpointId || undefined);
      message.success(`已请求从 checkpoint 恢复运行 ${runId}`);
      onResumed?.();
    } catch (error) {
      message.error(error instanceof Error ? error.message : '恢复运行失败');
    } finally {
      setBusy(null);
    }
  }, [runId, latest?.checkpointId, onResumed]);

  const handleCancel = useCallback(async () => {
    if (!jobId) return;
    setBusy('cancel');
    try {
      await cancelAiJob(jobId);
      message.warning(`已请求取消任务 ${jobId}`);
      onCancelled?.();
    } catch (error) {
      message.error(error instanceof Error ? error.message : '取消失败');
    } finally {
      setBusy(null);
    }
  }, [jobId, onCancelled]);

  return (
    <Card size="small" data-testid="run-resume-bar" style={{ borderColor: 'var(--ai-warning)' }}>
      <Space direction="vertical" size={8} style={{ width: '100%' }}>
        <Alert
          type="warning"
          showIcon
          message="运行已中断，可从最近的 checkpoint 恢复"
          description={
            <Space size={6} wrap>
              <Typography.Text type="secondary" style={{ fontSize: 12, fontFamily: 'var(--ai-font-mono)' }}>
                run: {runId}
              </Typography.Text>
              {latest ? (
                <Tag color="blue" style={{ fontSize: 11 }}>
                  最近 checkpoint: {latest.checkpointType} #{latest.sequenceNo}
                </Tag>
              ) : (
                <Tag style={{ fontSize: 11 }}>未发现可恢复 checkpoint</Tag>
              )}
            </Space>
          }
        />
        <Space>
          <Button
            type="primary"
            size="small"
            icon={<PlayCircleOutlined />}
            loading={busy === 'resume'}
            onClick={handleResume}
          >
            恢复运行
          </Button>
          {jobId ? (
            <Button
              danger
              size="small"
              icon={<StopOutlined />}
              loading={busy === 'cancel'}
              onClick={handleCancel}
            >
              取消运行
            </Button>
          ) : null}
        </Space>
      </Space>
    </Card>
  );
}
