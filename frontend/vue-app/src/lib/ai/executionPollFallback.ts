// 搬运自 frontend/ai-react/src/lib/sse/executionPollFallback.ts。
// 适配点：原先直接 import aiApi 的 getExecutionDetail；这里改为构造函数注入
// fetchDetail（页面侧用 createAiApi(useApi()).getExecutionDetail 传入），
// 构造函数参数顺序因此变为 (executionId, onData, fetchDetail, intervalMs)。
// 轮询/退避/去重逻辑不变。
export type ExecutionDetailFetcher = (executionId: string) => Promise<Record<string, unknown>>;

export class ExecutionPollFallback {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readonly executionId: string;
  private readonly onData: (payload: Record<string, unknown>) => void;
  private readonly fetchDetail: ExecutionDetailFetcher;
  private readonly intervalMs: number;
  private readonly maxIntervalMs: number;
  private closed = false;
  private inFlight = false;
  private nextIntervalMs: number;
  private lastSignature: string | null = null;
  private consecutiveNoops = 0;

  constructor(
    executionId: string,
    onData: (payload: Record<string, unknown>) => void,
    fetchDetail: ExecutionDetailFetcher,
    intervalMs = 4000,
  ) {
    this.executionId = executionId;
    this.onData = onData;
    this.fetchDetail = fetchDetail;
    this.intervalMs = intervalMs;
    this.maxIntervalMs = intervalMs * 4;
    this.nextIntervalMs = intervalMs;
  }

  start(): void {
    if (this.timer !== null || !this.executionId) {
      return;
    }
    this.closed = false;
    this.scheduleNextPoll(this.intervalMs);
  }

  private scheduleNextPoll(delayMs: number): void {
    if (this.closed) {
      return;
    }
    this.timer = globalThis.setTimeout(() => {
      this.timer = null;
      void this.pollOnce();
    }, delayMs);
  }

  private buildSignature(detail: Record<string, unknown>): string {
    return JSON.stringify({
      status: detail.status ?? null,
      tool_name: detail.tool_name ?? null,
      message: detail.message ?? null,
    });
  }

  private shouldEmit(detail: Record<string, unknown>): boolean {
    const signature = this.buildSignature(detail);
    if (signature === this.lastSignature) {
      return false;
    }
    this.lastSignature = signature;
    return true;
  }

  private getTerminalStatus(detail: Record<string, unknown>): string {
    return String(detail.status || '').toLowerCase();
  }

  private async pollOnce(): Promise<void> {
    if (this.closed || this.inFlight) {
      return;
    }

    this.inFlight = true;
    try {
      const detail = await this.fetchDetail(this.executionId);
      const changed = this.shouldEmit(detail);
      const status = this.getTerminalStatus(detail);

      if (changed) {
        this.onData(detail);
        this.consecutiveNoops = 0;
        this.nextIntervalMs = this.intervalMs;
      } else {
        this.consecutiveNoops += 1;
        this.nextIntervalMs = Math.min(this.maxIntervalMs, this.intervalMs * 2);
      }

      if (['success', 'error', 'failed', 'cancelled', 'timeout'].includes(status)) {
        this.stop();
        return;
      }
    } catch (_error) {
      this.nextIntervalMs = Math.min(this.maxIntervalMs, Math.max(this.intervalMs * 2, this.nextIntervalMs * 2));
    } finally {
      this.inFlight = false;
    }

    this.scheduleNextPoll(this.nextIntervalMs);
  }

  stop(): void {
    this.closed = true;
    if (this.timer !== null) {
      globalThis.clearTimeout(this.timer);
      this.timer = null;
    }
  }
}
