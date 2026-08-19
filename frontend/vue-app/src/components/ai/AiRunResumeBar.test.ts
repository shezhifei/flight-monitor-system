import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import AiRunResumeBar from './AiRunResumeBar.vue';

const listCheckpoints = vi.fn();
const resumeAiRun = vi.fn();
const cancelAiJob = vi.fn();
const showToast = vi.fn();

vi.mock('@/lib/ai/api', () => ({
  createAiApi: () => ({
    listAiRunCheckpoints: listCheckpoints,
    resumeAiRun,
    cancelAiJob,
  }),
}));
vi.mock('@/composables/useApi', () => ({ useApi: () => ({}) }));
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ showToast }) }));

const CKPT = { checkpoint_id: 'cp-3', sequence_no: 3, checkpoint_type: 'after_tool', created_at: 't' };

describe('AiRunResumeBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listCheckpoints.mockResolvedValue([CKPT]);
    resumeAiRun.mockResolvedValue({});
    cancelAiJob.mockResolvedValue({});
  });

  it('loads checkpoints and shows latest recoverable', async () => {
    const wrapper = mount(AiRunResumeBar, { props: { runId: 'run-1', jobId: 'job-1' } });
    await flushPromises();
    expect(listCheckpoints).toHaveBeenCalledWith('job-1', 'run-1');
    expect(wrapper.text()).toContain('最近 checkpoint: after_tool #3');
  });

  it('resumes from latest checkpoint and emits resumed', async () => {
    const wrapper = mount(AiRunResumeBar, { props: { runId: 'run-1', jobId: 'job-1' } });
    await flushPromises();
    await wrapper.find('.is-resume').trigger('click');
    await flushPromises();
    expect(resumeAiRun).toHaveBeenCalledWith('run-1', 'cp-3');
    expect(wrapper.emitted('resumed')).toHaveLength(1);
  });

  it('cancels job and emits cancelled', async () => {
    const wrapper = mount(AiRunResumeBar, { props: { runId: 'run-1', jobId: 'job-1' } });
    await flushPromises();
    await wrapper.find('.is-cancel').trigger('click');
    await flushPromises();
    expect(cancelAiJob).toHaveBeenCalledWith('job-1');
    expect(wrapper.emitted('cancelled')).toHaveLength(1);
  });

  it('hides cancel button without jobId', async () => {
    const wrapper = mount(AiRunResumeBar, { props: { runId: 'run-1' } });
    await flushPromises();
    expect(wrapper.find('.is-cancel').exists()).toBe(false);
    expect(wrapper.text()).toContain('未发现可恢复 checkpoint');
  });

  it('shows error toast when resume fails', async () => {
    resumeAiRun.mockRejectedValue(new Error('恢复失败'));
    const wrapper = mount(AiRunResumeBar, { props: { runId: 'run-1', jobId: 'job-1' } });
    await flushPromises();
    await wrapper.find('.is-resume').trigger('click');
    await flushPromises();
    expect(showToast).toHaveBeenCalledWith('error', '恢复失败');
    expect(wrapper.emitted('resumed')).toBeUndefined();
  });
});
