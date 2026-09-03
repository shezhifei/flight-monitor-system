import { computed, reactive, readonly } from 'vue';
import type { ComputedRef } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import { assertSameOriginHttpUrl } from '@/lib/url-guard';

export interface AuthTokenData {
  access_token: string;
  refresh_token?: string;
  token_type?: string;
  expires_in?: number;
  sse_token?: string;
  sse_expires_in?: number;
  session_secret?: string;
}

export interface JwtUser {
  id?: string;
  sub?: string;
  user_id?: string;
  username?: string;
  email?: string;
  display_name?: string;
  name?: string;
  role?: string;
  roles?: string[];
  is_admin?: boolean;
  is_active?: boolean;
  is_verified?: boolean;
  department?: string | null;
  department_id?: string | null;
  permissions?: string[];
  [key: string]: unknown;
}

export interface AuthEventSourceOptions {
  clientScope?: string;
  clientInstanceId?: string;
}

const IMPLIED_PERMISSION_MAP: Record<string, string[]> = {
  'flight:read': ['flight.read', 'business_case.read', 'workflow_run.read'],
  'dispatch:view': [
    'dispatch_order.read',
    'dispatch_catalog.read',
    'shift_handover.read',
    'notification.read',
    'notification.receipt_read',
  ],
  'dispatch:manage': [
    'dispatch_order.read',
    'dispatch_order.create',
    'dispatch_order.update',
    'dispatch_order.publish',
    'dispatch_order.cancel',
    'dispatch_catalog.read',
    'dispatch_catalog.edit',
    'shift_handover.read',
    'shift_handover.create',
    'shift_handover.submit',
    'shift_handover.ack',
    'notification.read',
    'notification.send',
    'notification.receipt_read',
    'notification.receipt_manage',
  ],
  'flowable:read': ['workflow_definition.read', 'workflow_run.read'],
  'flowable:manage': [
    'workflow_definition.read',
    'workflow_definition.edit',
    'workflow_definition.publish',
    'workflow_definition.deprecate',
  ],
  'user:read': ['user_admin.read'],
  'user:create': ['user_admin.edit'],
  'user:update': ['user_admin.edit'],
  'user:delete': ['user_admin.edit'],
  'role:read': ['auth_role.read'],
  'role:create': ['auth_role.edit'],
  'role:update': ['auth_role.edit'],
  'role:delete': ['auth_role.edit'],
  'auth:view': ['user_admin.read', 'auth_role.read', 'auth_permission_template.read'],
  'auth:manage': [
    'user_admin.read',
    'user_admin.edit',
    'auth_role.read',
    'auth_role.edit',
    'auth_permission_template.read',
    'auth_permission_template.edit',
  ],
  'system:admin': ['system.config_read', 'system.config_write', 'system.ops_admin'],
};

export interface UseAuthReturn {
  state: Readonly<typeof authState>;
  apiBase: ComputedRef<string>;
  getToken: typeof getToken;
  getSSEToken: typeof getSSEToken;
  getRefreshToken: typeof getRefreshToken;
  getTokenExpiresAt: typeof getTokenExpiresAt;
  getAuthHeaders: typeof getAuthHeaders;
  saveToken: typeof saveToken;
  refreshToken: typeof refreshToken;
  refreshSSEToken: typeof refreshSSEToken;
  startAutoRenewal: typeof startAutoRenewal;
  stopAutoRenewal: typeof stopAutoRenewal;
  logout: typeof logout;
  isAuthenticated: typeof isAuthenticated;
  requireAuth: typeof requireAuth;
  requireAuthAsync: typeof requireAuthAsync;
  restoreSession: typeof restoreSession;
  fetch: typeof authenticatedFetch;
  getEventSource: typeof getEventSource;
  invalidateSSEToken: () => void;
  getClientInstanceId: typeof getClientInstanceId;
  getUser: typeof getUser;
  isAdmin: typeof isAdmin;
  initialize: typeof initialize;
  isEmbeddedFrame: typeof isEmbeddedFrame;
  navigateToLogin: typeof navigateToLogin;
  navigateAfterLogin: typeof navigateAfterLogin;
}

const STORAGE_KEYS = {
  sseToken: 'sse_token',
  sseTokenExpiresAt: 'sse_token_expires_at',
  sessionSecret: 'session_secret',
  authSession: 'auth_session',
} as const;

const CONFIG = {
  renewalBufferMs: 5 * 60 * 1000,
  checkIntervalMs: 60 * 1000,
  heartbeatIntervalMs: 60 * 1000,
  loginPath: pageUrl('login'),
} as const;

const CLIENT_INSTANCE_KEY_PREFIX = 'fm_client_instance_id::';
/** 跨 iframe 串行 refresh，避免并发 refresh 轮转 cookie 互相踩踏 */
const REFRESH_LOCK_CHANNEL = 'fms-auth-refresh-lock';
const REFRESH_LOCK_STORAGE_KEY = 'fms-auth-refresh-lock-ts';

const authState = reactive({
  initialized: false,
  isRefreshing: false,
  isRefreshingSSE: false,
  autoRenewalActive: false,
  heartbeatActive: false,
  heartbeatSuspendedReason: null as string | null,
});

let renewalTimer = 0;
let heartbeatTimer = 0;
let refreshPromise: Promise<boolean> | null = null;
let heartbeatPromise: Promise<boolean> | null = null;
let sseTokenPromise: Promise<boolean> | null = null;
let restoreSessionPromise: Promise<boolean> | null = null;
let heartbeatLifecycleListenersBound = false;
const clientInstanceCache: Record<string, string> = {};

function isEmbeddedFrame(): boolean {
  if (typeof window === 'undefined') return false;
  try {
    if (window.self !== window.top) return true;
  } catch {
    return true;
  }
  try {
    const params = new URLSearchParams(window.location.search);
    return params.get('embed') === '1' || params.get('shell') === '1';
  } catch {
    return false;
  }
}

/** 登录跳转：iframe 内禁止嵌登录页，改为顶层跳转回工作区/登录 */
function navigateToLogin(returnPath?: string): void {
  if (typeof window === 'undefined') return;
  const login = CONFIG.loginPath;
  const redirectTarget = returnPath
    || (isEmbeddedFrame() ? pageUrl('workspace') : `${window.location.pathname}${window.location.search}`);
  const url = `${login}?redirect=${encodeURIComponent(redirectTarget)}`;
  try {
    if (window.top && window.top !== window) {
      window.top.location.href = url;
      return;
    }
  } catch {
    // cross-origin top — fall through
  }
  window.location.replace(url);
}

function navigateAfterLogin(target: string): void {
  if (typeof window === 'undefined') return;
  try {
    if (window.top && window.top !== window) {
      window.top.location.href = target;
      return;
    }
  } catch {
    // ignore
  }
  window.location.href = target;
}

function asAuthToken(raw: unknown): AuthTokenData | null {
  if (!raw || typeof raw !== 'object') {
    return null;
  }
  return raw as AuthTokenData;
}

async function withCrossFrameRefreshLock<T>(fn: () => Promise<T>): Promise<T> {
  // 简单时间戳锁：多 iframe 同时 restore 时错开 refresh，减少 cookie 轮转竞态
  const waitStart = Date.now();
  while (Date.now() - waitStart < 8000) {
    try {
      const heldAt = Number(localStorage.getItem(REFRESH_LOCK_STORAGE_KEY) || '0');
      if (!heldAt || Date.now() - heldAt > 5000) {
        localStorage.setItem(REFRESH_LOCK_STORAGE_KEY, String(Date.now()));
        break;
      }
    } catch {
      break;
    }
    await new Promise((r) => setTimeout(r, 50 + Math.floor(Math.random() * 80)));
  }
  try {
    return await fn();
  } finally {
    try {
      localStorage.removeItem(REFRESH_LOCK_STORAGE_KEY);
    } catch {
      // ignore
    }
    try {
      // 通知其它帧锁已释放，可继续执行 refresh
      new BroadcastChannel(REFRESH_LOCK_CHANNEL).postMessage({ type: 'unlock' });
    } catch {
      // ignore
    }
  }
}

// Tokens are kept in memory only (not localStorage/sessionStorage) and
// transmitted automatically via HttpOnly cookies. The in-memory copy is used
// for decoding user claims and as a fallback Authorization header.
let memoryAccessToken: string | null = null;
let memoryRefreshToken: string | null = null;
let memoryTokenExpiresAt: number | null = null;
let memorySessionSecret: string | null = null;
let memoryCurrentUser: JwtUser | null = null;

const apiBase = computed(() => {
  if (typeof window === 'undefined') {
    return '/api/v2';
  }
  return `${window.location.origin}/api/v2`;
});

function getLocalStorageSafe(): Storage | null {
  try {
    return typeof window !== 'undefined' ? window.localStorage : null;
  } catch {
    return null;
  }
}

function getSessionStorageSafe(): Storage | null {
  try {
    return typeof window !== 'undefined' ? window.sessionStorage : null;
  } catch {
    return null;
  }
}

function getStorageValue(key: string): string | null {
  const localValue = getLocalStorageSafe()?.getItem(key);
  if (localValue) {
    return localValue;
  }
  return getSessionStorageSafe()?.getItem(key) ?? null;
}

function removeStorageValue(storage: Storage | null, key: string): void {
  try {
    storage?.removeItem(key);
  } catch {
    // ignore storage errors
  }
}

function setStorageValue(storage: Storage | null, key: string, value: string): void {
  try {
    storage?.setItem(key, value);
  } catch {
    // ignore storage errors
  }
}

function clearAuthStorage(storage: Storage | null): void {
  removeStorageValue(storage, STORAGE_KEYS.sseToken);
  removeStorageValue(storage, STORAGE_KEYS.sseTokenExpiresAt);
  removeStorageValue(storage, STORAGE_KEYS.sessionSecret);
  removeStorageValue(storage, STORAGE_KEYS.authSession);
}

function clearLocalSession(): void {
  memoryAccessToken = null;
  memoryRefreshToken = null;
  memoryTokenExpiresAt = null;
  memorySessionSecret = null;
  memoryCurrentUser = null;
  clearAuthStorage(getSessionStorageSafe());
}

function getSessionStorage(): Storage | null {
  return getSessionStorageSafe();
}

function readBrowserCookie(name: string): string | null {
  if (typeof document === 'undefined') {
    return null;
  }
  const prefix = `${encodeURIComponent(name)}=`;
  const parts = document.cookie.split(';');
  for (const part of parts) {
    const trimmed = part.trim();
    if (trimmed.startsWith(prefix)) {
      return decodeURIComponent(trimmed.slice(prefix.length));
    }
  }
  // Also try unencoded name
  const rawPrefix = `${name}=`;
  for (const part of parts) {
    const trimmed = part.trim();
    if (trimmed.startsWith(rawPrefix)) {
      return decodeURIComponent(trimmed.slice(rawPrefix.length));
    }
  }
  return null;
}

/**
 * Resolve the anti-replay session secret.
 *
 * Web responses omit `session_secret` from JSON and set a non-HttpOnly cookie instead.
 * Cookie is the source of truth after login/refresh so sibling iframes that rotate
 * tokens update this frame's signing secret without a full reload.
 */
function resolveSessionSecret(tokenData?: AuthTokenData | null): string | null {
  const fromBody = tokenData?.session_secret?.trim();
  if (fromBody) {
    return fromBody;
  }
  const fromCookie = readBrowserCookie('session_secret');
  if (fromCookie) {
    return fromCookie;
  }
  return memorySessionSecret;
}

function getToken(): string | null {
  return memoryAccessToken;
}

function getSessionSecret(): string | null {
  // Prefer cookie so multi-tab/iframe refresh rotation is picked up before signing.
  const fromCookie = readBrowserCookie('session_secret');
  if (fromCookie) {
    if (memorySessionSecret !== fromCookie) {
      memorySessionSecret = fromCookie;
    }
    return fromCookie;
  }
  return memorySessionSecret;
}

function getRefreshToken(): string | null {
  return memoryRefreshToken;
}

function getTokenExpiresAt(): number | null {
  return memoryTokenExpiresAt;
}

function getSSEToken(): string | null {
  return getStorageValue(STORAGE_KEYS.sseToken);
}

function getSSETokenExpiresAt(): number | null {
  const value = getStorageValue(STORAGE_KEYS.sseTokenExpiresAt);
  if (!value) {
    return null;
  }
  const numeric = Number.parseInt(value, 10);
  return Number.isFinite(numeric) ? numeric : null;
}

    function invalidateSSEToken(): void {
      const storage = getSessionStorage();
      if (storage) {
        removeStorageValue(storage, STORAGE_KEYS.sseToken);
        removeStorageValue(storage, STORAGE_KEYS.sseTokenExpiresAt);
      }
    }

    function hasUsableSSEToken(): boolean {
  const token = getSSEToken();
  if (!token) {
    return false;
  }
  const expiresAt = getSSETokenExpiresAt();
  if (!expiresAt) {
    return true;
  }
  return expiresAt - Date.now() > 30_000;
}

function persistSSEToken(
  storage: Storage | null,
  tokenData: Partial<AuthTokenData>,
  options: { keepExistingWhenMissing?: boolean } = {},
): boolean {
  if (storage && tokenData.sse_token) {
    setStorageValue(storage, STORAGE_KEYS.sseToken, tokenData.sse_token);
    const expiresAt = Date.now() + ((tokenData.sse_expires_in ?? tokenData.expires_in ?? 3600) * 1000);
    setStorageValue(storage, STORAGE_KEYS.sseTokenExpiresAt, String(expiresAt));
    return true;
  }

  if (!options.keepExistingWhenMissing) {
    removeStorageValue(storage, STORAGE_KEYS.sseToken);
    removeStorageValue(storage, STORAGE_KEYS.sseTokenExpiresAt);
  }

  return false;
}

function decodeToken(token: string | null): JwtUser | null {
  if (!token || typeof window === 'undefined' || typeof window.atob !== 'function') {
    return null;
  }

  try {
    const base64Url = token.split('.')[1];
    if (!base64Url) {
      return null;
    }
    const base64 = base64Url.replace(/-/g, '+').replace(/_/g, '/');
    const jsonPayload = decodeURIComponent(
      window
        .atob(base64)
        .split('')
        .map((character) => `%${`00${character.charCodeAt(0).toString(16)}`.slice(-2)}`)
        .join(''),
    );
    return normalizeJwtUser(JSON.parse(jsonPayload) as JwtUser);
  } catch {
    return null;
  }
}

function readJwtStringClaim(
  payload: Record<string, unknown>,
  keys: string[],
): string | null {
  for (const key of keys) {
    const value = payload[key];
    if (typeof value === 'string') {
      const normalized = value.trim();
      if (normalized) {
        return normalized;
      }
      continue;
    }
    if (typeof value === 'number' && Number.isFinite(value)) {
      return String(value);
    }
  }
  return null;
}

function normalizeJwtUser(user: JwtUser | null): JwtUser | null {
  if (!user || typeof user !== 'object') {
    return null;
  }

  const normalizedUser: JwtUser = { ...user };
  const payload = normalizedUser as Record<string, unknown>;

  const departmentId = readJwtStringClaim(payload, [
    'department_id',
    'departmentId',
    'dept_id',
    'deptId',
  ]);
  if (departmentId) {
    normalizedUser.department_id = departmentId;
  }

  const departmentName = readJwtStringClaim(payload, [
    'department',
    'department_name',
    'departmentName',
    'dept_name',
    'deptName',
  ]);
  if (departmentName) {
    normalizedUser.department = departmentName;
  }

  return normalizedUser;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function normalizeStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .filter((item): item is string => typeof item === 'string')
    .map((item) => item.trim())
    .filter(Boolean);
}

function normalizeCurrentUserResponse(payload: unknown): JwtUser | null {
  if (!isRecord(payload)) {
    return null;
  }
  if (payload.success === false) {
    return null;
  }

  const candidate = isRecord(payload.data) ? payload.data : payload;
  const id = readJwtStringClaim(candidate, ['id', 'sub', 'user_id', 'userId']);
  const username = readJwtStringClaim(candidate, ['username', 'user_name', 'name']);
  if (!id && !username) {
    return null;
  }

  const roles = normalizeStringArray(candidate.roles);
  const permissions = normalizeStringArray(candidate.permissions);
  const normalized = normalizeJwtUser({
    ...candidate,
    id: id ?? undefined,
    sub: readJwtStringClaim(candidate, ['sub', 'id', 'user_id', 'userId']) ?? undefined,
    user_id: readJwtStringClaim(candidate, ['user_id', 'userId', 'id', 'sub']) ?? undefined,
    username: username ?? undefined,
    role: readJwtStringClaim(candidate, ['role']) ?? roles[0],
    roles,
    permissions,
  } as JwtUser);

  return normalized;
}

function normalizePermissionName(permission: unknown): string | null {
  const normalized = String(permission || '').trim();
  return normalized ? normalized : null;
}

export function getUserPermissions(user: JwtUser | null | undefined): string[] {
  if (!user || typeof user !== 'object') {
    return [];
  }

  const expandedPermissions = new Set<string>();
  const directPermissions = Array.isArray(user.permissions) ? user.permissions : [];

  for (const directPermission of directPermissions) {
    const normalized = normalizePermissionName(directPermission);
    if (!normalized) {
      continue;
    }
    expandedPermissions.add(normalized);

    const impliedPermissions = IMPLIED_PERMISSION_MAP[normalized] ?? [];
    for (const impliedPermission of impliedPermissions) {
      expandedPermissions.add(impliedPermission);
    }
  }

  return Array.from(expandedPermissions);
}

export function hasUserPermission(
  user: JwtUser | null | undefined,
  required: string | string[],
): boolean {
  if (!user || typeof user !== 'object') {
    return false;
  }

  if (user.is_admin === true || user.role === 'admin') {
    return true;
  }

  const grantedPermissions = new Set(getUserPermissions(user));
  if (grantedPermissions.has('*')) {
    return true;
  }

  const requiredPermissions = Array.isArray(required) ? required : [required];
  for (const requiredPermission of requiredPermissions) {
    const normalized = normalizePermissionName(requiredPermission);
    const separator = normalized?.includes(':') ? ':' : normalized?.includes('.') ? '.' : null;
    const wildcard = normalized && separator
      ? `${normalized.split(separator, 1)[0]}${separator}*`
      : null;
    if (
      normalized
      && (grantedPermissions.has(normalized) || (wildcard ? grantedPermissions.has(wildcard) : false))
    ) {
      return true;
    }
  }

  return false;
}

function getUser(): JwtUser | null {
  return memoryCurrentUser ?? decodeToken(getToken());
}

function isAdmin(): boolean {
  const user = getUser();
  return Boolean(user && (user.is_admin === true || user.role === 'admin'));
}

function getAuthHeaders(): Record<string, string> {
  return {
    'Content-Type': 'application/json',
  };
}

function isPageVisible(): boolean {
  if (typeof document === 'undefined') {
    return true;
  }
  return document.visibilityState !== 'hidden';
}

function isBrowserOnline(): boolean {
  if (typeof navigator === 'undefined' || typeof navigator.onLine !== 'boolean') {
    return true;
  }
  return navigator.onLine;
}

function getHeartbeatSuspensionReason(): string | null {
  if (!isPageVisible()) {
    return 'page-hidden';
  }
  if (!isBrowserOnline()) {
    return 'browser-offline';
  }
  return null;
}

function canRunHeartbeat(): boolean {
  return Boolean(getToken()) && !getHeartbeatSuspensionReason();
}

function syncHeartbeatReason(): void {
  authState.heartbeatSuspendedReason = getHeartbeatSuspensionReason();
}

function bindHeartbeatLifecycleListeners(): void {
  if (heartbeatLifecycleListenersBound || typeof window === 'undefined') {
    return;
  }

  let previousSuspendedReason: string | null = null;

  const handleLifecycleChange = () => {
    const wasSuspended = Boolean(previousSuspendedReason);
    syncHeartbeatReason();
    previousSuspendedReason = authState.heartbeatSuspendedReason;
    syncHeartbeatWithPageState();
    if (!authState.heartbeatSuspendedReason && getToken() && wasSuspended) {
      void checkAndRenew();
    }
  };

  if (typeof document !== 'undefined' && typeof document.addEventListener === 'function') {
    document.addEventListener('visibilitychange', handleLifecycleChange);
  }

  window.addEventListener('online', handleLifecycleChange);
  window.addEventListener('offline', handleLifecycleChange);
  heartbeatLifecycleListenersBound = true;
}

function createClientInstanceId(): string {
  const nowPart = Date.now().toString(36);

  if (typeof window !== 'undefined' && window.crypto && typeof window.crypto.getRandomValues === 'function') {
    const bytes = new Uint8Array(8);
    window.crypto.getRandomValues(bytes);
    return `${nowPart}-${Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')}`;
  }

  return `${nowPart}-${Math.random().toString(36).slice(2, 14)}`;
}

function getClientInstanceId(scope = 'default'): string {
  const normalizedScope = String(scope || 'default').trim() || 'default';
  if (clientInstanceCache[normalizedScope]) {
    return clientInstanceCache[normalizedScope];
  }

  const key = `${CLIENT_INSTANCE_KEY_PREFIX}${normalizedScope}`;
  const sessionStorageRef = getSessionStorageSafe();
  let instanceId = sessionStorageRef?.getItem(key) ?? null;

  if (!instanceId) {
    instanceId = createClientInstanceId();
    setStorageValue(sessionStorageRef, key, instanceId);
  }

  clientInstanceCache[normalizedScope] = instanceId;
  return instanceId;
}

function saveToken(tokenData: AuthTokenData): void {
  const sessionStorageRef = getSessionStorageSafe();

  // Access/refresh tokens stay in memory; the backend delivers them as HttpOnly cookies.
  memoryAccessToken = tokenData.access_token || null;
  memoryRefreshToken = tokenData.refresh_token || null;
  memoryTokenExpiresAt = Date.now() + ((tokenData.expires_in ?? 3600) * 1000);
  memoryCurrentUser = decodeToken(memoryAccessToken);

  memorySessionSecret = resolveSessionSecret(tokenData);
  if (sessionStorageRef) {
    setStorageValue(sessionStorageRef, STORAGE_KEYS.authSession, '1');
  }

  const hasSSEToken = persistSSEToken(sessionStorageRef, tokenData);

  startAutoRenewal();
  if (!hasSSEToken) {
    void refreshSSEToken();
  }
}

async function refreshToken(options: { refreshSSE?: boolean } = {}): Promise<boolean> {
  if (refreshPromise) {
    return refreshPromise;
  }

  if (typeof window === 'undefined') {
    return false;
  }

  authState.isRefreshing = true;

  refreshPromise = (async () => {
    try {
      const response = await globalThis.fetch(`${apiBase.value}/auth/refresh`, {
        method: 'POST',
        credentials: 'include',
        headers: {
          'Content-Type': 'application/json',
        },
        // The refresh token is sent automatically as an HttpOnly cookie.
      });

      if (!response.ok) {
        if (response.status === 401) {
          clearLocalSession();
        }
        return false;
      }

      const tokenData = asAuthToken(await response.json()) ?? ({} as AuthTokenData);
      const sessionStorageRef = getSessionStorageSafe();

      // Web 面 access_token 在 JSON；同时 HttpOnly cookie 已由 Set-Cookie 更新
      memoryAccessToken = tokenData.access_token || memoryAccessToken || null;
      if (tokenData.refresh_token) {
        memoryRefreshToken = tokenData.refresh_token;
      }
      if (typeof tokenData.expires_in === 'number' && Number.isFinite(tokenData.expires_in)) {
        memoryTokenExpiresAt = Date.now() + (tokenData.expires_in * 1000);
      } else if (!memoryTokenExpiresAt) {
        memoryTokenExpiresAt = Date.now() + 3600 * 1000;
      }
      memoryCurrentUser = decodeToken(memoryAccessToken) ?? memoryCurrentUser;

      // Drop stale in-memory secret first so cookie rotation from this (or a sibling)
      // frame is always re-read after refresh.
      memorySessionSecret = null;
      memorySessionSecret = resolveSessionSecret(tokenData);
      if (sessionStorageRef) {
        setStorageValue(sessionStorageRef, STORAGE_KEYS.authSession, '1');
      }

      const hasSSEToken = persistSSEToken(sessionStorageRef, tokenData, { keepExistingWhenMissing: true });

      if (!hasSSEToken && options.refreshSSE !== false) {
        void refreshSSEToken();
      }

      // 即使 body 未带 access_token，cookie 刷新成功也视为可用（后续 /auth/me 靠 cookie）
      return true;
    } catch {
      return false;
    } finally {
      authState.isRefreshing = false;
      refreshPromise = null;
    }
  })();

  return refreshPromise;
}

async function checkAndRenew(): Promise<void> {
  const expiresAt = getTokenExpiresAt();
  if (!expiresAt) {
    return;
  }

  if (expiresAt - Date.now() <= CONFIG.renewalBufferMs) {
    await refreshToken();
  }
}

async function sendHeartbeat(): Promise<boolean> {
  if (heartbeatPromise) {
    return heartbeatPromise;
  }

  const token = getToken();
  if (!token || typeof window === 'undefined') {
    return false;
  }

  heartbeatPromise = (async () => {
    try {
      const response = await authenticatedFetch(`${apiBase.value}/auth/heartbeat`, {
        method: 'POST',
        headers: getAuthHeaders(),
      });

      return response.ok;
    } catch {
      return false;
    } finally {
      heartbeatPromise = null;
    }
  })();

  return heartbeatPromise;
}

function stopHeartbeat(): void {
  if (heartbeatTimer) {
    window.clearInterval(heartbeatTimer);
    heartbeatTimer = 0;
  }
  authState.heartbeatActive = false;
}

function startHeartbeat(options: { immediate?: boolean } = {}): void {
  if (typeof window === 'undefined') {
    return;
  }

  if (heartbeatTimer) {
    window.clearInterval(heartbeatTimer);
    heartbeatTimer = 0;
  }

  syncHeartbeatReason();
  if (!canRunHeartbeat()) {
    authState.heartbeatActive = false;
    return;
  }

  authState.heartbeatActive = true;
  if (options.immediate !== false) {
    void sendHeartbeat();
  }

  heartbeatTimer = window.setInterval(() => {
    if (!canRunHeartbeat()) {
      syncHeartbeatWithPageState({ immediate: false });
      return;
    }
    void sendHeartbeat();
  }, CONFIG.heartbeatIntervalMs);
}

function syncHeartbeatWithPageState(options: { immediate?: boolean } = {}): boolean {
  syncHeartbeatReason();

  if (!getToken()) {
    stopHeartbeat();
    return false;
  }

  if (authState.heartbeatSuspendedReason) {
    stopHeartbeat();
    return false;
  }

  if (!heartbeatTimer) {
    startHeartbeat({ immediate: options.immediate !== false });
  }

  return true;
}

function startAutoRenewal(): void {
  if (typeof window === 'undefined') {
    return;
  }

  if (renewalTimer) {
    window.clearInterval(renewalTimer);
    renewalTimer = 0;
  }

  if (!getToken()) {
    authState.autoRenewalActive = false;
    stopHeartbeat();
    return;
  }

  bindHeartbeatLifecycleListeners();
  authState.autoRenewalActive = true;
  void checkAndRenew();
  syncHeartbeatWithPageState();

  renewalTimer = window.setInterval(() => {
    void checkAndRenew();
  }, CONFIG.checkIntervalMs);
}

function stopAutoRenewal(): void {
  stopHeartbeat();

  if (renewalTimer) {
    window.clearInterval(renewalTimer);
    renewalTimer = 0;
  }

  authState.autoRenewalActive = false;
}

async function logout(): Promise<void> {
  stopAutoRenewal();

  if (typeof window !== 'undefined') {
    try {
      await performAuthenticatedFetch(`${apiBase.value}/auth/logout`, {
        method: 'POST',
        headers: getAuthHeaders(),
      }, false);
    } catch {
      // Ignore network errors; local cleanup still proceeds.
    }
  }

  clearLocalSession();

  if (typeof window !== 'undefined') {
    // 禁止在工作区 iframe 内嵌登录页
    navigateToLogin(pageUrl('workspace'));
  }
}

async function refreshSSEToken(force = false): Promise<boolean> {
  if (!force && hasUsableSSEToken()) {
    return true;
  }

  if (sseTokenPromise) {
    return sseTokenPromise;
  }

  if (!isAuthenticated() || typeof window === 'undefined') {
    return false;
  }

  authState.isRefreshingSSE = true;

  sseTokenPromise = (async () => {
    try {
      const response = await authenticatedFetch(`${apiBase.value}/auth/sse-token`, {
        method: 'POST',
        headers: getAuthHeaders(),
      });

      if (response.ok) {
        const data = (await response.json()) as Partial<AuthTokenData>;
        persistSSEToken(getSessionStorage(), data);
        return true;
      }

      return false;
    } catch {
      return false;
    } finally {
      authState.isRefreshingSSE = false;
      sseTokenPromise = null;
    }
  })();

  return sseTokenPromise;
}

function isAuthenticated(): boolean {
  const token = getToken();
  if (!token) {
    return false;
  }
  const expiresAt = getTokenExpiresAt();
  if (expiresAt && Date.now() > expiresAt) {
    return false;
  }
  return true;
}

function requireAuth(): boolean {
  if (!isAuthenticated()) {
    logout();
    return false;
  }
  return true;
}

async function requireAuthAsync(): Promise<boolean> {
  const token = getToken();
  if (!token) {
    return restoreSession();
  }

  const expiresAt = getTokenExpiresAt();
  if (expiresAt && Date.now() < expiresAt) {
    return true;
  }

  const refreshed = await refreshToken();
  if (refreshed) {
    return true;
  }

  logout();
  return false;
}

async function restoreSession(): Promise<boolean> {
  if (restoreSessionPromise) {
    return restoreSessionPromise;
  }

  if (typeof window === 'undefined') {
    return false;
  }

  restoreSessionPromise = (async () => {
    return withCrossFrameRefreshLock(async () => {
      const expiresAt = getTokenExpiresAt();
      const hasUsableAccessToken = Boolean(getToken()) && (!expiresAt || expiresAt > Date.now());
      if (!hasUsableAccessToken && !(await refreshToken({ refreshSSE: false }))) {
        // iframe 冷启动仅有 cookie、无内存 token 时 refresh 失败才判未登录
        clearLocalSession();
        return false;
      }

      try {
        const currentUserRequest = () => performAuthenticatedFetch(`${apiBase.value}/auth/me`, {
          method: 'GET',
          headers: getAuthHeaders(),
        }, false);
        let response = await currentUserRequest();
        if (response.status === 401) {
          const refreshed = await refreshToken({ refreshSSE: false });
          if (!refreshed) {
            clearLocalSession();
            return false;
          }
          response = await currentUserRequest();
        }
        if (!response.ok) {
          clearLocalSession();
          return false;
        }

        const currentUser = normalizeCurrentUserResponse(await response.json());
        if (!currentUser) {
          clearLocalSession();
          return false;
        }

        memoryCurrentUser = currentUser;
        authState.initialized = true;
        startAutoRenewal();
        void refreshSSEToken();
        return true;
      } catch (err) {
        console.warn('[auth] restoreSession failed', err);
        clearLocalSession();
        return false;
      }
    });
  })().finally(() => {
    restoreSessionPromise = null;
  });

  return restoreSessionPromise;
}

function buildAuthenticatedHeaders(
  providedHeaders: HeadersInit | undefined,
  body: BodyInit | null | undefined,
): Headers {
  const headers = new Headers(providedHeaders);

  const isFormData = typeof FormData !== 'undefined' && body instanceof FormData;
  if (!headers.has('Content-Type') && !isFormData) {
    headers.set('Content-Type', 'application/json');
  }

  return headers;
}

function mergeHeaders(...sources: Array<HeadersInit | undefined>): Headers {
  const merged = new Headers();

  for (const source of sources) {
    if (!source) {
      continue;
    }

    new Headers(source).forEach((value, key) => {
      merged.set(key, value);
    });
  }

  return merged;
}

async function sha256Hex(input: string | ArrayBuffer | Uint8Array): Promise<string> {
  const source = typeof input === 'string'
    ? new TextEncoder().encode(input)
    : input instanceof Uint8Array
      ? input
      : new Uint8Array(input);
  const digestInput = source.slice().buffer;
  const subtle = globalThis.crypto?.subtle ?? window.crypto?.subtle;
  if (!subtle) {
    throw new Error('当前浏览器不支持 SHA-256 请求体摘要');
  }
  const digest = await subtle.digest('SHA-256', digestInput);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

async function serializeFormData(formData: FormData): Promise<string> {
  const parts: string[] = [];

  for (const [key, value] of formData.entries()) {
    if (typeof value === 'string') {
      parts.push(`${key}=${value}`);
      continue;
    }

    parts.push(
      `${key}=file:${value.name}:${value.type}:${value.size}:${value.lastModified}`,
    );
  }

  return parts.join('&');
}

async function buildRequestBodyHash(body: BodyInit | null | undefined): Promise<string> {
  if (body == null) {
    return sha256Hex('');
  }

  if (typeof body === 'string') {
    return sha256Hex(body);
  }

  if (typeof URLSearchParams !== 'undefined' && body instanceof URLSearchParams) {
    return sha256Hex(body.toString());
  }

  if (typeof FormData !== 'undefined' && body instanceof FormData) {
    return sha256Hex(await serializeFormData(body));
  }

  if (body instanceof Blob) {
    return sha256Hex(await body.arrayBuffer());
  }

  if (body instanceof ArrayBuffer) {
    return sha256Hex(body);
  }

  if (ArrayBuffer.isView(body)) {
    return sha256Hex(new Uint8Array(body.buffer, body.byteOffset, body.byteLength));
  }

  return sha256Hex(String(body));
}

// 防重放签名生成核心
async function generateAntiReplayHeaders(
  method: string,
  requestUri: string,
  bodyHash: string,
): Promise<Record<string, string>> {
  const sessionSecret = getSessionSecret();
  if (!sessionSecret) return {};

  const timestamp = Math.floor(Date.now() / 1000).toString();
  const nonce = createClientInstanceId();
  
  const payload = `${method.toUpperCase()}:${requestUri}:${timestamp}:${nonce}:${bodyHash}`;
  
  try {
    const subtle = globalThis.crypto?.subtle ?? window.crypto?.subtle;
    if (subtle) {
      const encoder = new TextEncoder();
      const cryptoKey = await subtle.importKey(
        'raw',
        encoder.encode(sessionSecret),
        { name: 'HMAC', hash: 'SHA-256' },
        false,
        ['sign'],
      );
      
      const signatureBuffer = await subtle.sign(
        'HMAC',
        cryptoKey,
        encoder.encode(payload),
      );
      
      const signatureHex = Array.from(new Uint8Array(signatureBuffer), (byte) => byte.toString(16).padStart(2, '0')).join('');
      
      return {
        'X-Request-Timestamp': timestamp,
        'X-Request-Nonce': nonce,
        'X-Request-Body-SHA256': bodyHash,
        'X-Request-Signature': signatureHex,
      };
    }
    throw new Error('当前浏览器不支持请求签名，已阻止发送未签名请求');
  } catch (e) {
    console.warn('Failed to generate anti-replay signature', e);
    throw new Error(`请求签名失败，已阻止发送未签名请求: ${e instanceof Error ? e.message : String(e)}`);
  }
}

async function performAuthenticatedFetch(
  input: RequestInfo | URL,
  options: RequestInit,
  recoverUnauthorized: boolean,
): Promise<Response> {
  if (typeof window === 'undefined') {
    throw new Error('authenticated fetch is only available in the browser');
  }

  const fetchUrl = typeof input === 'string' ? input : (input instanceof Request ? input.url : input.href);
  const urlObj = assertSameOriginHttpUrl(fetchUrl, 'authenticated fetch');
  const requestUri = `${urlObj.pathname}${urlObj.search}`;
  const resolvedFetchUrl = urlObj.toString();

  const method = options.method || (input instanceof Request ? input.method : 'GET');
  const baseHeaders = mergeHeaders(input instanceof Request ? input.headers : undefined, options.headers);
  const createUnsignedRequest = (): Request => (
    input instanceof Request
      ? new Request(input, {
          ...options,
          method,
          credentials: 'include',
          headers: buildAuthenticatedHeaders(baseHeaders, options.body),
        })
      : new Request(resolvedFetchUrl, {
          ...options,
          method,
          credentials: 'include',
          headers: buildAuthenticatedHeaders(baseHeaders, options.body),
        })
  );
  const unsignedRequest = createUnsignedRequest();
  const bodyHash = await buildRequestBodyHash(await unsignedRequest.clone().arrayBuffer());
  const antiReplayHeaders = await generateAntiReplayHeaders(method, requestUri, bodyHash);
  const signedRequest = new Request(unsignedRequest, {
    headers: mergeHeaders(unsignedRequest.headers, antiReplayHeaders),
  });

  const response = await globalThis.fetch(signedRequest);
  // Login/register intentionally return 401 for bad credentials; do not treat as
  // session expiry or force a full logout redirect mid-form submission.
  const isCredentialChallenge = /\/api\/v2\/auth\/(login|register)(?:\?|$)/.test(urlObj.pathname);
  if (response.status !== 401 || !recoverUnauthorized || isCredentialChallenge) {
    return response;
  }

  const refreshed = await refreshToken();
  if (!refreshed) {
    await logout();
    return response;
  }

  const retryAntiReplayHeaders = await generateAntiReplayHeaders(method, requestUri, bodyHash);
  const retryUnsignedRequest = createUnsignedRequest();
  const retryRequest = new Request(retryUnsignedRequest, {
    headers: mergeHeaders(retryUnsignedRequest.headers, retryAntiReplayHeaders),
  });
  const retryResponse = await globalThis.fetch(retryRequest);

  if (retryResponse.status === 401) {
    await logout();
  }

  return retryResponse;
}

async function authenticatedFetch(input: RequestInfo | URL, options: RequestInit = {}): Promise<Response> {
  return performAuthenticatedFetch(input, options, true);
}

function appendQueryParam(url: URL, key: string, value: string | null | undefined): void {
  if (!value) {
    return;
  }
  url.searchParams.set(key, value);
}

interface ParsedSSEEvent {
  type: string;
  data: string;
  id: string;
}

function parseSSEEvent(block: string, lastEventId: string): ParsedSSEEvent | null {
  let eventType = 'message';
  let eventId = lastEventId;
  const dataLines: string[] = [];

  for (const rawLine of block.split(/\r?\n/)) {
    if (!rawLine || rawLine.startsWith(':')) {
      continue;
    }

    const separatorIndex = rawLine.indexOf(':');
    const field = separatorIndex === -1 ? rawLine : rawLine.slice(0, separatorIndex);
    const value = separatorIndex === -1
      ? ''
      : rawLine.slice(separatorIndex + 1).replace(/^ /, '');

    if (field === 'event') {
      eventType = value || 'message';
    } else if (field === 'data') {
      dataLines.push(value);
    } else if (field === 'id') {
      eventId = value;
    }
  }

  if (dataLines.length === 0) {
    return null;
  }

  return {
    type: eventType,
    data: dataLines.join('\n'),
    id: eventId,
  };
}

class AuthenticatedFetchEventSource extends EventTarget {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;

  readonly url: string;
  readonly withCredentials = true;
  readyState = AuthenticatedFetchEventSource.CONNECTING;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  private readonly abortController = new AbortController();
  private lastEventId = '';

  constructor(url: string) {
    super();
    this.url = url;
    void this.connect();
  }

  close(): void {
    if (this.readyState === AuthenticatedFetchEventSource.CLOSED) {
      return;
    }
    this.readyState = AuthenticatedFetchEventSource.CLOSED;
    this.abortController.abort();
  }

  private emitOpen(): void {
    if (this.readyState === AuthenticatedFetchEventSource.CLOSED) {
      return;
    }
    this.readyState = AuthenticatedFetchEventSource.OPEN;
    const event = new Event('open');
    this.onopen?.(event);
    this.dispatchEvent(event);
  }

  private emitMessage(parsed: ParsedSSEEvent): void {
    if (this.readyState === AuthenticatedFetchEventSource.CLOSED) {
      return;
    }
    this.lastEventId = parsed.id;
    const event = new MessageEvent<string>(parsed.type, {
      data: parsed.data,
      lastEventId: parsed.id,
    });
    if (parsed.type === 'message') {
      this.onmessage?.(event);
    }
    this.dispatchEvent(event);
  }

  private emitError(): void {
    if (this.readyState === AuthenticatedFetchEventSource.CLOSED) {
      return;
    }
    this.readyState = AuthenticatedFetchEventSource.CLOSED;
    const event = new Event('error');
    this.onerror?.(event);
    this.dispatchEvent(event);
  }

  private processBufferedEvents(buffer: string, flush = false): string {
    const normalized = buffer.replace(/\r\n/g, '\n');
    const parts = normalized.split('\n\n');
    const completeParts = flush ? parts : parts.slice(0, -1);

    for (const part of completeParts) {
      const parsed = parseSSEEvent(part, this.lastEventId);
      if (parsed) {
        this.emitMessage(parsed);
      }
    }

    return flush ? '' : (parts.at(-1) ?? '');
  }

  private async connect(): Promise<void> {
    try {
      const response = await authenticatedFetch(this.url, {
        method: 'GET',
        headers: {
          ...getAuthHeaders(),
          Accept: 'text/event-stream',
          'Cache-Control': 'no-cache',
        },
        signal: this.abortController.signal,
      });

      if (!response.ok || !response.body) {
        this.emitError();
        return;
      }

      this.emitOpen();
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (this.readyState !== AuthenticatedFetchEventSource.CLOSED) {
        const { done, value } = await reader.read();
        if (done) {
          break;
        }
        buffer += decoder.decode(value, { stream: true });
        buffer = this.processBufferedEvents(buffer);
      }

      buffer += decoder.decode();
      this.processBufferedEvents(buffer, true);
      this.emitError();
    } catch {
      if (!this.abortController.signal.aborted) {
        this.emitError();
      }
    }
  }
}

function getEventSource(url: string, options: AuthEventSourceOptions = {}): EventSource {
  if (typeof window === 'undefined') {
    throw new Error('EventSource is only available in the browser');
  }

  // 认证统一走 fetch-based EventSource（Authorization 头 + 同源 cookie），
  // 不再支持把 token 放进 URL query——query 中的 token 会落入访问日志、
  // 浏览器历史与 Referrer。
  const clientScope = options.clientScope ?? 'default';
  const clientInstanceId = options.clientInstanceId ?? getClientInstanceId(clientScope);
  const authenticatedUrl = assertSameOriginHttpUrl(url, 'authenticated EventSource');

  appendQueryParam(authenticatedUrl, 'client_instance_id', clientInstanceId);
  return new AuthenticatedFetchEventSource(authenticatedUrl.toString()) as unknown as EventSource;
}

function initialize(): void {
  if (authState.initialized) {
    return;
  }

  authState.initialized = true;
  if (isAuthenticated()) {
    startAutoRenewal();
    void refreshSSEToken();
  }
}

const authApi: UseAuthReturn = {
  state: readonly(authState),
  apiBase,
  getToken,
  getSSEToken,
  getRefreshToken,
  getTokenExpiresAt,
  getAuthHeaders,
  saveToken,
  refreshToken,
  refreshSSEToken,
  startAutoRenewal,
  stopAutoRenewal,
  logout,
  isAuthenticated,
  requireAuth,
  requireAuthAsync,
  restoreSession,
  fetch: authenticatedFetch,
  getEventSource,
  invalidateSSEToken,
  getClientInstanceId,
  getUser,
  isAdmin,
  initialize,
  isEmbeddedFrame,
  navigateToLogin,
  navigateAfterLogin,
};

export function useAuth(): UseAuthReturn {
  initialize();
  return authApi;
}
