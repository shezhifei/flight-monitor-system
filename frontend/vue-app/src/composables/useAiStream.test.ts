// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAiStream } from './useAiStream';

/**
 * Task C5: the Vue stream composable must tolerate the hybrid-agent event
 * payloads (subagent_event / context.compressed / plan tool calls) that now
 * flow over the shared `ai_execution` SSE topic, without duplicating any
 * chat-state logic (that lives in ai-react).
 */

type Listener = (event: MessageEvent) => void;

class MockEventSource {
  static instances: MockEventSource[] = [];

  readonly url: string;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  closed = false;
  private listeners = new Map<string, Listener[]>();

  constructor(url: string | URL) {
    this.url = String(url);
    MockEventSource.instances.push(this);
  }

  addEventListener(name: string, listener: Listener): void {
    const list = this.listeners.get(name) || [];
    list.push(listener);
    this.listeners.set(name, list);
  }

  removeEventListener(): void {}

  close(): void {
    this.closed = true;
  }

  emit(name: string, data: string): void {
    (this.listeners.get(name) || []).forEach((listener) =>
      listener({ data } as MessageEvent),
    );
  }
}

const getEventSource = vi.fn((url: string) => new MockEventSource(url) as unknown as EventSource);

vi.mock('./useAuth', () => ({
  useAuth: () => ({
    getEventSource,
  }),
}));

vi.mock('./useToast', () => ({
  useToast: () => ({
    show: vi.fn(),
    showToast: vi.fn(),
  }),
}));

describe('useAiStream hybrid-agent event handling', () => {
  beforeEach(() => {
    MockEventSource.instances = [];
    getEventSource.mockClear();
  });

  it('appends a subagent_event payload received on the ai_execution topic', () => {
    const stream = useAiStream();
    stream.startStream(['ai_execution']);
    const source = MockEventSource.instances[0];

    source.emit(
      'ai_execution',
      JSON.stringify({
        type: 'subagent_event',
        event_type: 'tool_call',
        parent_run_id: 'run-parent',
        subagent_depth: 1,
        tool_name: 'flight_status_lookup',
        tool_type: 'read_only',
        timestamp: '2026-08-15T10:00:00Z',
      }),
    );

    expect(stream.messages.value).toHaveLength(1);
    const message = stream.messages.value[0];
    // payload.type is preserved verbatim; the ai_execution topic frame stays intact
    expect(message.type).toBe('subagent_event');
    expect(message.data?.parent_run_id).toBe('run-parent');
    expect(message.data?.tool_name).toBe('flight_status_lookup');
  });

  it('appends a context.compressed payload without breaking the stream', () => {
    const stream = useAiStream();
    stream.startStream(['ai_execution']);
    const source = MockEventSource.instances[0];

    source.emit(
      'ai_execution',
      JSON.stringify({
        type: 'context.compressed',
        run_id: 'run-1',
        strategy: 'summarize',
        before_tokens: 12000,
        after_tokens: 3000,
      }),
    );

    expect(stream.messages.value).toHaveLength(1);
    expect(stream.messages.value[0].data?.strategy).toBe('summarize');
    expect(stream.error.value).toBe('');
  });

  it('appends plan tool call payloads (update_plan) as tool-call messages', () => {
    const stream = useAiStream();
    stream.startStream(['ai_execution']);
    const source = MockEventSource.instances[0];

    source.emit(
      'ai_execution',
      JSON.stringify({
        type: 'tool.call',
        tool_name: 'update_plan',
        arguments: {
          plan_description: 'p',
          steps: [{ id: 's1', description: '查询航班状态' }],
        },
      }),
    );

    expect(stream.messages.value).toHaveLength(1);
    expect(stream.messages.value[0].type).toBe('tool.call');
    expect(stream.messages.value[0].data?.tool_name).toBe('update_plan');
  });

  it('keeps non-JSON frames as plain text messages', () => {
    const stream = useAiStream();
    stream.startStream(['ai_execution']);
    const source = MockEventSource.instances[0];

    source.emit('ai_execution', 'not-json-payload');

    expect(stream.messages.value).toHaveLength(1);
    expect(stream.messages.value[0].type).toBe('ai_tool_call');
    expect(stream.messages.value[0].content).toBe('not-json-payload');
  });
});
