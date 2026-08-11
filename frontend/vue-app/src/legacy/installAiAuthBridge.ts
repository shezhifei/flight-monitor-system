import {
  getUserPermissions,
  hasUserPermission,
  useAuth,
  type AuthEventSourceOptions,
  type JwtUser,
} from '@/composables/useAuth';

export const AI_AUTH_BRIDGE_VERSION = 1 as const;

export interface AiAuthProvider {
  requireAuthAsync: () => Promise<boolean>;
  fetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  getEventSource: (url: string, options?: AuthEventSourceOptions) => EventSource;
  getUser: () => JwtUser | null;
  logout: () => void | Promise<void>;
  isAdmin: () => boolean;
}

export interface VueOwnedAiAuthBridge {
  readonly owner: 'vue-app';
  readonly version: typeof AI_AUTH_BRIDGE_VERSION;
  requireAuthAsync: () => Promise<boolean>;
  fetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  getEventSource: (url: string, options?: AuthEventSourceOptions) => EventSource;
  getUser: () => JwtUser | null;
  getPermissions: () => readonly string[];
  hasPermission: (permission: string) => boolean;
  logout: () => Promise<void>;
  isAdmin: () => boolean;
}

declare global {
  interface Window {
    Auth?: VueOwnedAiAuthBridge;
  }
}

let installedBridge: VueOwnedAiAuthBridge | null = null;

function assertBrowserWindow(): Window {
  if (typeof window === 'undefined') {
    throw new Error('The Vue-owned AI auth bridge can only be installed in a browser.');
  }
  return window;
}

function createBridge(provider: AiAuthProvider): VueOwnedAiAuthBridge {
  let authenticated = false;

  const requireAuthAsync = async (): Promise<boolean> => {
    try {
      authenticated = (await provider.requireAuthAsync()) === true;
    } catch {
      authenticated = false;
    }
    return authenticated;
  };

  const bridge: VueOwnedAiAuthBridge = {
    owner: 'vue-app',
    version: AI_AUTH_BRIDGE_VERSION,
    requireAuthAsync,
    fetch: async (input, init) => {
      if (!await requireAuthAsync()) {
        throw new Error('AI request blocked because the authenticated session is unavailable.');
      }
      return provider.fetch(input, init);
    },
    getEventSource(url, options) {
      if (!authenticated) {
        throw new Error('AI event stream blocked until authentication succeeds.');
      }
      return provider.getEventSource(url, options);
    },
    getUser() {
      try {
        return provider.getUser();
      } catch {
        return null;
      }
    },
    getPermissions() {
      try {
        return Object.freeze([...getUserPermissions(provider.getUser())]);
      } catch {
        return Object.freeze([]);
      }
    },
    hasPermission(permission) {
      try {
        return hasUserPermission(provider.getUser(), permission);
      } catch {
        return false;
      }
    },
    async logout() {
      authenticated = false;
      await provider.logout();
    },
    isAdmin() {
      try {
        return provider.isAdmin();
      } catch {
        return false;
      }
    },
  };

  return Object.freeze(bridge);
}

/**
 * Installs the sole production auth boundary consumed by retained React entries.
 * React bundles never receive tokens and cannot bypass the Vue auth transport.
 */
export function installAiAuthBridge(
  provider: AiAuthProvider = useAuth(),
): VueOwnedAiAuthBridge {
  const browserWindow = assertBrowserWindow();
  if (installedBridge && browserWindow.Auth === installedBridge) {
    return installedBridge;
  }

  installedBridge = createBridge(provider);
  Object.defineProperty(browserWindow, 'Auth', {
    configurable: true,
    enumerable: false,
    writable: false,
    value: installedBridge,
  });
  return installedBridge;
}

/** Test-only reset for the module singleton and configurable Window property. */
export function __resetAiAuthBridgeForTests(): void {
  if (typeof window !== 'undefined') {
    delete window.Auth;
  }
  installedBridge = null;
}
