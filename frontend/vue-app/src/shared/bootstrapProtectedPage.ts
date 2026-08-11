import { useAuth } from '@/composables/useAuth';
import { pageUrl } from '@/shared/page-routes';

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
  return true;
}
