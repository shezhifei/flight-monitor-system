/**
 * @vitest-environment jsdom
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import LlmEvalLab from './LlmEvalLab.vue';

const listEvalJobs = vi.fn();
const getEvalJob = vi.fn();
const createEvalJob = vi.fn();
const cancelEvalJob = vi.fn();
const showToast = vi.fn();

vi.mock('@/composables/useApi', () => ({ useApi: () => ({}) }));
vi.mock('@/composables/useAuth', () => ({
  useAuth: () => ({
    getUser: () => ({ username: 'admin', is_admin: true }),
    logout: vi.fn(),
  }),
}));
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ showToast }) }));
vi.mock('@/lib/ai/api', () => ({
  createLlmEvalApi: () => ({ listEvalJobs, getEvalJob, createEvalJob, cancelEvalJob }),
}));
// useTheme 在模块级读 localStorage，node 内置 localStorage 未初始化时会炸，直接换掉组件
vi.mock('@/components/ui/ThemeToggle.vue', () => ({ default: { template: '<div />' } }));

function makeJob(overrides: Record<string, unknown> = {}) {
  return {
    job_id: 'job-1',
    name: 'agent eval',
    dataset_path: 'docs/fixtures/agent_query_ops_eval.jsonl',
    status: 'completed',
    completed_runs: 6,
    total_runs: 6,
    created_at: '2026-08-01T02:00:00Z',
    ...overrides,
  };
}

function mountPage() {
  return mount(LlmEvalLab, {
    global: {
      stubs: { teleport: true, SvgIcon: true, ThemeToggle: true },
    },
  });
}

describe('LlmEvalLab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listEvalJobs.mockResolvedValue([makeJob()]);
    getEvalJob.mockResolvedValue(makeJob({ gates: [] }));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('loads jobs on mount and renders the table', async () => {
    const wrapper = mountPage();
    await flushPromises();
    expect(listEvalJobs).toHaveBeenCalledWith(30);
    const rows = wrapper.findAll('tbody tr');
    expect(rows).toHaveLength(1);
    expect(rows[0]!.text()).toContain('job-1');
  });

  it('maps job status to signal pill tones', async () => {
    listEvalJobs.mockResolvedValue([
      makeJob({ job_id: 'a', status: 'completed' }),
      makeJob({ job_id: 'b', status: 'running' }),
      makeJob({ job_id: 'c', status: 'failed' }),
      makeJob({ job_id: 'd', status: 'pending' }),
    ]);
    const wrapper = mountPage();
    await flushPromises();
    const tones = wrapper.findAll('tbody .ui-pill').map((n) => n.attributes('data-tone'));
    expect(tones[0]).toBe('ok');
    expect(tones[1]).toBe('act');
    expect(tones[2]).toBe('danger');
    expect(tones[3]).toBe('mute');
  });

  it('does not poll when no job is active', async () => {
    vi.useFakeTimers();
    mountPage();
    await flushPromises();
    expect(listEvalJobs).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(12000);
    expect(listEvalJobs).toHaveBeenCalledTimes(1);
  });

  it('polls every 4s while a job is active', async () => {
    vi.useFakeTimers();
    listEvalJobs.mockResolvedValue([makeJob({ status: 'running' })]);
    mountPage();
    await flushPromises();
    expect(listEvalJobs).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(4000);
    expect(listEvalJobs).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(4000);
    expect(listEvalJobs).toHaveBeenCalledTimes(3);
  });

  it('loads detail with gates when clicking 查看', async () => {
    getEvalJob.mockResolvedValue(
      makeJob({
        gates: [
          { metric_name: 'evidence_coverage', value: 0.95, threshold: 0.9, status: 'pass' },
          { metric_name: 'tool_policy', value: 0.5, threshold: 0.8, status: 'fail' },
        ],
      }),
    );
    const wrapper = mountPage();
    await flushPromises();
    await wrapper.find('tbody .ui-btn').trigger('click');
    await flushPromises();
    expect(getEvalJob).toHaveBeenCalledWith('job-1');
    const gateRows = wrapper.findAll('.eval-gates-head ~ .table-container tbody tr');
    expect(gateRows).toHaveLength(2);
    expect(gateRows[0]!.text()).toContain('0.950');
    expect(gateRows[0]!.text()).toContain('0.90');
    expect(gateRows[0]!.find('.ui-pill').attributes('data-tone')).toBe('ok');
    expect(gateRows[1]!.find('.ui-pill').attributes('data-tone')).toBe('danger');
  });

  it('cancels an active job and refreshes the list', async () => {
    listEvalJobs.mockResolvedValue([makeJob({ status: 'running' })]);
    cancelEvalJob.mockResolvedValue({});
    const wrapper = mountPage();
    await flushPromises();
    const cancelBtn = wrapper.find("tbody .ui-btn[data-variant='danger']");
    expect(cancelBtn.attributes('disabled')).toBeUndefined();
    await cancelBtn.trigger('click');
    await flushPromises();
    expect(cancelEvalJob).toHaveBeenCalledWith('job-1');
    expect(showToast).toHaveBeenCalledWith('success', '评测任务已取消');
  });

  it('disables cancel for finished jobs', async () => {
    const wrapper = mountPage();
    await flushPromises();
    expect(wrapper.find("tbody .ui-btn[data-variant='danger']").attributes('disabled')).toBeDefined();
  });

  it('creates a job from the drawer form, then opens its detail', async () => {
    createEvalJob.mockResolvedValue({ job_id: 'job-new', status: 'pending' });
    const wrapper = mountPage();
    await flushPromises();
    await wrapper.find(".header-actions .ui-btn[data-variant='primary']").trigger('click');
    const form = wrapper.find('form.eval-form');
    expect(form.exists()).toBe(true);
    await form.find('input[type="text"]').setValue('nightly eval');
    await form.trigger('submit');
    await flushPromises();
    expect(createEvalJob).toHaveBeenCalledWith({
      name: 'nightly eval',
      dataset_path: 'docs/fixtures/agent_query_ops_eval.jsonl',
      description: '',
      run: true,
    });
    expect(showToast).toHaveBeenCalledWith('success', '评测任务已创建: job-new');
    expect(getEvalJob).toHaveBeenCalledWith('job-new');
    expect(wrapper.find('form.eval-form').exists()).toBe(false);
  });

  it('shows an error toast when job loading fails', async () => {
    listEvalJobs.mockRejectedValue(new Error('boom'));
    mountPage();
    await flushPromises();
    expect(showToast).toHaveBeenCalledWith('error', 'boom');
  });
});
