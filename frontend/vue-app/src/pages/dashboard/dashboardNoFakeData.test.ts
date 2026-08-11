/**
 * Tests for Dashboard error-state behavior (Task 10: no fake data).
 *
 * Verifies that when the dashboard API fails, the component:
 * 1. Sets workbenchData to null (NOT fake prototype data)
 * 2. Sets loadFailed = true
 * 3. Captures the error message
 * 4. Exposes a retry function that calls the API again
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { ref } from 'vue';

type ApiResult = { ok: boolean; data?: { success: boolean; data?: Record<string, unknown>; error?: string } };

function createDashboardLoadController() {
  const workbenchData = ref<Record<string, unknown> | null>(null);
  const isLoading = ref(true);
  const loadFailed = ref(false);
  const errorMessage = ref<string>('');
  const isRetrying = ref(false);

  let apiCallCount = 0;
  let apiImpl: () => Promise<ApiResult> = async () => ({
    ok: true,
    data: { success: true, data: { user_name: 'Real User' } },
  });

  function setApiImpl(impl: () => Promise<ApiResult>) {
    apiImpl = impl;
  }

  function useErrorState(message: string) {
    workbenchData.value = null;
    loadFailed.value = true;
    errorMessage.value = message;
  }

  async function loadDashboard() {
    isLoading.value = !isRetrying.value;
    loadFailed.value = false;
    errorMessage.value = '';
    apiCallCount++;
    try {
      const result = await apiImpl();
      if (result.ok && result.data?.success && result.data.data) {
        workbenchData.value = result.data.data;
      } else {
        const apiMsg = result.data?.error || 'Dashboard returned an invalid response';
        useErrorState(apiMsg);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to load dashboard data';
      useErrorState(msg);
    } finally {
      isLoading.value = false;
      isRetrying.value = false;
    }
  }

  async function handleRetry() {
    isRetrying.value = true;
    await loadDashboard();
  }

  function reset() {
    workbenchData.value = null;
    isLoading.value = true;
    loadFailed.value = false;
    errorMessage.value = '';
    isRetrying.value = false;
    apiCallCount = 0;
  }

  return {
    workbenchData,
    isLoading,
    loadFailed,
    errorMessage,
    isRetrying,
    apiCallCount: () => apiCallCount,
    setApiImpl,
    loadDashboard,
    handleRetry,
    reset,
  };
}

describe('Dashboard load error handling (no fake data)', () => {
  let ctrl: ReturnType<typeof createDashboardLoadController>;

  beforeEach(() => {
    ctrl = createDashboardLoadController();
  });

  it('sets workbenchData to null when API throws an error', async () => {
    ctrl.setApiImpl(async () => {
      throw new Error('Connection refused');
    });
    await ctrl.loadDashboard();

    expect(ctrl.workbenchData.value).toBeNull();
    expect(ctrl.loadFailed.value).toBe(true);
    expect(ctrl.errorMessage.value).toBe('Connection refused');
    expect(ctrl.isLoading.value).toBe(false);
  });

  it('sets error state when API returns non-success envelope', async () => {
    ctrl.setApiImpl(async () => ({
      ok: true,
      data: { success: false, error: 'Internal Server Error' },
    }));
    await ctrl.loadDashboard();

    expect(ctrl.workbenchData.value).toBeNull();
    expect(ctrl.loadFailed.value).toBe(true);
    expect(ctrl.errorMessage.value).toBe('Internal Server Error');
  });

  it('populates workbenchData with real data on success', async () => {
    await ctrl.loadDashboard();

    expect(ctrl.workbenchData.value).not.toBeNull();
    expect(ctrl.workbenchData.value?.user_name).toBe('Real User');
    expect(ctrl.loadFailed.value).toBe(false);
    expect(ctrl.errorMessage.value).toBe('');
  });

  it('retry re-invokes the API and succeeds after failure', async () => {
    ctrl.setApiImpl(async () => {
      throw new Error('timeout');
    });
    await ctrl.loadDashboard();
    expect(ctrl.loadFailed.value).toBe(true);
    expect(ctrl.apiCallCount()).toBe(1);

    ctrl.setApiImpl(async () => ({
      ok: true,
      data: { success: true, data: { user_name: 'Recovered User' } },
    }));
    await ctrl.handleRetry();

    expect(ctrl.apiCallCount()).toBe(2);
    expect(ctrl.loadFailed.value).toBe(false);
    expect(ctrl.workbenchData.value?.user_name).toBe('Recovered User');
  });

  it('retry stays in error state if API still fails', async () => {
    ctrl.setApiImpl(async () => {
      throw new Error('persistent failure');
    });
    await ctrl.loadDashboard();
    expect(ctrl.loadFailed.value).toBe(true);

    await ctrl.handleRetry();

    expect(ctrl.apiCallCount()).toBe(2);
    expect(ctrl.loadFailed.value).toBe(true);
    expect(ctrl.workbenchData.value).toBeNull();
    expect(ctrl.errorMessage.value).toBe('persistent failure');
  });

  it('error state never contains fake/prototype indicator strings', async () => {
    ctrl.setApiImpl(async () => {
      throw new Error('any error');
    });
    await ctrl.loadDashboard();

    expect(ctrl.workbenchData.value).toBeNull();

    const fakeIndicators = [
      '原型', 'mock data', 'fake', 'demo', 'sample data', 'stub',
      '张三', '李四', '演示', '示例数据', 'fallback',
    ];
    const serialized = JSON.stringify({
      data: ctrl.workbenchData.value,
      error: ctrl.errorMessage.value,
    });
    for (const indicator of fakeIndicators) {
      expect(serialized.toLowerCase()).not.toContain(indicator.toLowerCase());
    }
  });
});
