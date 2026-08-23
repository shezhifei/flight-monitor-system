import { createApp } from 'vue';
import { useAuth } from '@/composables/useAuth';
import { pageUrl } from '@/shared/page-routes';
import ToastRegion from '@/components/ui/ToastRegion.vue';
import { startGlobalReceiptToasts } from '@/shared/globalReceiptToasts';

export interface ProtectedPageAuth {
  restoreSession(): Promise<boolean>;
}

export interface BootstrapProtectedPageOptions {
  auth?: ProtectedPageAuth;
  redirectToLogin?: () => void;
}

function redirectToLogin(): void {
  // iframe 内禁止嵌 login 页：顶层跳到登录并带回工作区
  try {
    const { navigateToLogin } = useAuth();
    navigateToLogin(pageUrl('workspace'));
    return;
  } catch {
    // fallback
  }
  if (typeof window !== 'undefined') {
    const url = `${pageUrl('login')}?redirect=${encodeURIComponent(pageUrl('workspace'))}`;
    try {
      if (window.top && window.top !== window) {
        window.top.location.href = url;
        return;
      }
    } catch {
      // ignore
    }
    window.location.replace(url);
  }
}

/**
 * 工作区 iframe 嵌入态：隐藏页内 unified header / breadcrumb，避免双层顶栏。
 * 触发条件：被 iframe 嵌套，或 URL 带 embed=1。
 */
export function markWorkspaceEmbed(): void {
  if (typeof window === 'undefined' || typeof document === 'undefined') return;
  try {
    const params = new URLSearchParams(window.location.search);
    const flagged = params.get('embed') === '1' || params.get('shell') === '1';
    const inFrame = window.self !== window.top;
    if (flagged || inFrame) {
      document.documentElement.classList.add('workspace-embed');
      document.body?.classList.add('workspace-embed');
    }
  } catch {
    // ignore
  }
}

// 入口 import 时尽早打标，减少 iframe 内顶栏预留空白的闪烁
markWorkspaceEmbed();

/**
 * 把全局 toast 渲染器挂到 body 末尾（独立于页面 #app，避免被页面根组件的
 * 样式/卸载影响）。每个受保护页面只挂一次。
 */
function mountGlobalToastRegion(): void {
  if (typeof document === 'undefined') return;
  if (document.getElementById('fms-toast-region-host')) return;
  const host = document.createElement('div');
  host.id = 'fms-toast-region-host';
  document.body.appendChild(host);
  createApp(ToastRegion).mount(host);
}

/**
 * Restores the cookie-backed session before any protected Vue tree is created.
 * Returning false means the page failed closed and no protected component was mounted.
 */
export async function bootstrapProtectedPage(
  mount: () => void,
  options: BootstrapProtectedPageOptions = {},
): Promise<boolean> {
  markWorkspaceEmbed();

  const auth = options.auth ?? useAuth();
  const authenticated = await auth.restoreSession();

  if (!authenticated) {
    (options.redirectToLogin ?? redirectToLogin)();
    return false;
  }

  mount();
  mountGlobalToastRegion();
  startGlobalReceiptToasts();
  return true;
}
