export const REQUIRED_AUTH_BRIDGE_VERSION = 1 as const;

export interface AuthBridge {
  readonly owner: 'vue-app';
  readonly version: typeof REQUIRED_AUTH_BRIDGE_VERSION;
  requireAuthAsync: () => Promise<boolean>;
  fetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  getEventSource: (
    url: string,
    options?: { clientScope?: string; clientInstanceId?: string },
  ) => EventSource;
  getUser: () => Record<string, unknown> | null;
  getPermissions: () => readonly string[];
  hasPermission: (permission: string) => boolean;
  logout: () => void | Promise<void>;
  isAdmin: () => boolean;
}

declare global {
  interface Window {
    Auth?: AuthBridge;
    FM_AI_BRIDGE?: Record<string, unknown>;
    DISPATCH_AI_BRIDGE?: Record<string, unknown>;
    FLOWABLE_AI_BRIDGE?: Record<string, unknown>;
  }
}

export class AuthBridgeUnavailableError extends Error {
  constructor() {
    super('Vue authentication bridge is unavailable or incomplete.');
    this.name = 'AuthBridgeUnavailableError';
  }
}

function isCompleteBridge(candidate: AuthBridge | undefined): candidate is AuthBridge {
  return Boolean(
    candidate
      && candidate.owner === 'vue-app'
      && candidate.version === REQUIRED_AUTH_BRIDGE_VERSION
      && typeof candidate.requireAuthAsync === 'function'
      && typeof candidate.fetch === 'function'
      && typeof candidate.getEventSource === 'function'
      && typeof candidate.getUser === 'function'
      && typeof candidate.getPermissions === 'function'
      && typeof candidate.hasPermission === 'function'
      && typeof candidate.logout === 'function'
      && typeof candidate.isAdmin === 'function',
  );
}

function getBridge(): AuthBridge | null {
  if (typeof window === 'undefined' || !isCompleteBridge(window.Auth)) {
    return null;
  }
  return window.Auth;
}

function requireBridge(): AuthBridge {
  const bridge = getBridge();
  if (!bridge) {
    throw new AuthBridgeUnavailableError();
  }
  return bridge;
}

export async function requireAuth(): Promise<boolean> {
  const bridge = getBridge();
  if (!bridge) {
    return false;
  }
  try {
    return (await bridge.requireAuthAsync()) === true;
  } catch {
    console.error('[auth-bridge] authentication failed');
    return false;
  }
}

export async function authFetch(
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<Response> {
  return requireBridge().fetch(input, init);
}

export function createEventSource(
  url: string,
  options: { clientScope?: string; clientInstanceId?: string } = {},
): EventSource {
  return requireBridge().getEventSource(url, options);
}

export function getCurrentUser(): Record<string, unknown> | null {
  const bridge = getBridge();
  if (!bridge) {
    return null;
  }
  try {
    return bridge.getUser();
  } catch {
    return null;
  }
}

export async function logout(): Promise<void> {
  const bridge = getBridge();
  if (!bridge) {
    if (typeof window !== 'undefined') {
      window.location.href = '/frontend/login.html';
    }
    return;
  }
  await bridge.logout();
}

export function getPermissions(): readonly string[] {
  const bridge = getBridge();
  if (!bridge) {
    return [];
  }
  try {
    return bridge.getPermissions();
  } catch {
    return [];
  }
}

export function hasPermission(permission: string): boolean {
  const normalized = String(permission || '').trim();
  if (!normalized) {
    return false;
  }
  const bridge = getBridge();
  if (!bridge) {
    return false;
  }
  try {
    return bridge.hasPermission(normalized);
  } catch {
    return false;
  }
}

export function hasRole(role: string): boolean {
  const normalized = String(role || '').trim();
  if (!normalized) {
    return false;
  }
  const user = getCurrentUser();
  if (!user) {
    return false;
  }
  const roles = Array.isArray(user.roles)
    ? user.roles.filter((value): value is string => typeof value === 'string')
    : [];
  if (typeof user.role === 'string') {
    roles.push(user.role);
  }
  return roles.includes(normalized);
}

export function isAdmin(): boolean {
  const bridge = getBridge();
  if (!bridge) {
    return false;
  }
  try {
    return bridge.isAdmin();
  } catch {
    return false;
  }
}
