import { StrictMode, useLayoutEffect, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';

import { FrontendEntryShell } from '@/components/shell/FrontendEntryShell';
import { getCurrentUser, requireAuth } from '@/lib/auth/authBridge';
import { getFrontendEntryDefinition } from '@/lib/shell/entryRegistry';

interface BootstrapEntryOptions {
  entryName: string;
  render: () => ReactNode;
  gate?: (user: Record<string, unknown> | null) => boolean;
}

function renderMissingHostError(entryName: string, hostId: string): void {
  if (typeof document === 'undefined' || !document.body) {
    return;
  }
  const fallbackId = `ai-entry-host-error-${entryName}`;
  if (document.getElementById(fallbackId)) {
    return;
  }
  const fallback = document.createElement('div');
  fallback.id = fallbackId;
  fallback.setAttribute('role', 'alert');
  fallback.style.border = '1px solid #fecaca';
  fallback.style.background = '#fef2f2';
  fallback.style.color = '#991b1b';
  fallback.style.borderRadius = '8px';
  fallback.style.margin = '24px';
  fallback.style.padding = '16px';
  fallback.style.fontSize = '14px';
  fallback.style.lineHeight = '1.6';
  const title = document.createElement('strong');
  title.textContent = 'AI 前端加载失败';
  const detail = document.createElement('div');
  detail.textContent = `${entryName} 缺少宿主节点 #${hostId}`;
  fallback.append(title, detail);
  document.body.prepend(fallback);
}

function resolveHost(entryName: string): HTMLElement | null {
  const definition = getFrontendEntryDefinition(entryName);
  const host = document.getElementById(definition.hostId);
  if (!host) {
    console.error(`[frontend-shell] missing host for ${entryName}: #${definition.hostId}`);
    renderMissingHostError(entryName, definition.hostId);
    return null;
  }
  host.setAttribute('data-ai-entry', definition.entryName);
  host.setAttribute('data-shell-surface', definition.surface);
  host.setAttribute('data-ai-bootstrap', 'bootstrapping');
  host.removeAttribute('data-ai-mounted');
  host.classList.add('fm-entry-host', `fm-entry-host--${definition.surface}`);
  return host;
}

function EntryCommitBoundary({
  host,
  onCommit,
  children,
}: {
  host: HTMLElement;
  onCommit: () => void;
  children: ReactNode;
}) {
  useLayoutEffect(() => {
    host.setAttribute('data-ai-mounted', 'true');
    host.setAttribute('data-ai-bootstrap', 'ready');
    onCommit();
  }, [host, onCommit]);

  return children;
}

export async function bootstrapEntry(options: BootstrapEntryOptions): Promise<void> {
  const ok = await requireAuth();
  if (!ok) {
    throw new Error('认证失败或会话已过期，请重新登录后再打开该 AI 页面');
  }

  const user = getCurrentUser();
  if (!user) {
    throw new Error('认证用户信息不可用，已阻止挂载 AI 页面');
  }
  if (options.gate && !options.gate(user)) {
    throw new Error('当前用户无权访问此 AI 功能');
  }

  const definition = getFrontendEntryDefinition(options.entryName);
  const host = resolveHost(options.entryName);
  if (!host) {
    throw new Error(`AI 页面缺少宿主节点 #${definition.hostId}`);
  }

  await new Promise<void>((resolve) => {
    createRoot(host).render(
      <StrictMode>
        <EntryCommitBoundary host={host} onCommit={resolve}>
          <FrontendEntryShell entryName={definition.entryName} surface={definition.surface}>
            {options.render()}
          </FrontendEntryShell>
        </EntryCommitBoundary>
      </StrictMode>,
    );
  });
}

export function renderBootstrapError(entryName: string, error: unknown): void {
  console.error(`[frontend-shell] failed to bootstrap ${entryName}`, error);
  const definition = getFrontendEntryDefinition(entryName);
  const host = resolveHost(entryName);
  if (!host) {
    return;
  }
  host.removeAttribute('data-ai-mounted');
  host.setAttribute('data-ai-bootstrap', 'error');
  const message = error instanceof Error ? error.message : String(error || '未知错误');
  createRoot(host).render(
    <StrictMode>
      <FrontendEntryShell entryName={definition.entryName} surface={definition.surface}>
        <div
          role="alert"
          style={{
            border: '1px solid #fecaca',
            background: '#fef2f2',
            color: '#991b1b',
            borderRadius: 8,
            padding: 16,
            fontSize: 14,
            lineHeight: 1.6,
          }}
        >
          <strong>AI 前端加载失败</strong>
          <div>{message}</div>
        </div>
      </FrontendEntryShell>
    </StrictMode>,
  );
}
