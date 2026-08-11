// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

import { PAGE_ROUTES } from './page-routes';
import { bootstrapProtectedPage, type ProtectedPageAuth } from './bootstrapProtectedPage';

function authResult(result: boolean): ProtectedPageAuth {
  return {
    restoreSession: vi.fn().mockResolvedValue(result),
  };
}

describe('bootstrapProtectedPage', () => {
  it('guards every application entry except the public login page', () => {
    const entriesDirectory = resolve(process.cwd(), 'src', 'entries');

    for (const pageId of Object.keys(PAGE_ROUTES)) {
      const source = readFileSync(`${entriesDirectory}/${pageId}.ts`, 'utf8');
      if (pageId === 'login') {
        expect(source).not.toContain('bootstrapProtectedPage');
      } else {
        expect(source).toContain('await bootstrapProtectedPage(');
      }
    }
  });

  it('restores the session before mounting protected content', async () => {
    const calls: string[] = [];
    const auth: ProtectedPageAuth = {
      restoreSession: vi.fn(async () => {
        calls.push('restore-session');
        return true;
      }),
    };

    await expect(bootstrapProtectedPage(
      () => calls.push('mount'),
      { auth },
    )).resolves.toBe(true);

    // markWorkspaceEmbed runs first; session restore then mount
    expect(calls).toEqual(['restore-session', 'mount']);
  });

  it('fails closed and redirects without mounting when session restore fails', async () => {
    const mount = vi.fn();
    const redirectToLogin = vi.fn();

    await expect(bootstrapProtectedPage(mount, {
      auth: authResult(false),
      redirectToLogin,
    })).resolves.toBe(false);

    expect(mount).not.toHaveBeenCalled();
    expect(redirectToLogin).toHaveBeenCalledOnce();
  });

  it('does not mask mounting failures after authentication succeeds', async () => {
    const error = new Error('mount failed');

    await expect(bootstrapProtectedPage(
      () => { throw error; },
      { auth: authResult(true) },
    )).rejects.toBe(error);
  });
});
