import { describe, expect, it } from 'vitest';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { handleDispatchReplanMessage } from '../dispatchReplanWorker';

describe('no dead worker copy', () => {
  it('does not keep the legacy dispatch_replan_worker.js copy', () => {
    const deadPath = resolve(__dirname, '../dispatch_replan_worker.js');

    expect(existsSync(deadPath)).toBe(false);
  });

  it('keeps the active TypeScript dispatch replan worker', () => {
    const activePath = resolve(__dirname, '../dispatchReplanWorker.ts');

    expect(existsSync(activePath)).toBe(true);
  });
});

describe('dispatch replan worker message handling', () => {
  it('returns an empty successful payload when there are no optimizable clusters', async () => {
    const messages: unknown[] = [];
    let solverLoaded = false;

    await handleDispatchReplanMessage(
      {
        solver_version: 'ortools-test',
        optimizable_orders: [],
      },
      (message) => messages.push(message),
      async () => {
        solverLoaded = true;
        return {
          solve_cluster: () => '{}',
          runtime_manifest: {},
          active_manifest: {},
        };
      },
    );

    expect(messages).toEqual([{ ok: true, payload: {} }]);
    expect(solverLoaded).toBe(false);
  });

  it('posts a structured error when required snapshot fields are missing', async () => {
    const messages: unknown[] = [];

    await handleDispatchReplanMessage({}, (message) => messages.push(message));

    expect(messages).toEqual([
      {
        ok: false,
        error: 'solver_version is required',
      },
    ]);
  });
});
