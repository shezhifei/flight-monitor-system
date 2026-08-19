<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import { useAuth, type AuthTokenData } from '@/composables/useAuth';
import { useApi } from '@/composables/useApi';
import { useTheme } from '@/composables/useTheme';
import SvgIcon from '@/components/ui/SvgIcon.vue';

const username = ref('');
const password = ref('');
const isLoading = ref(false);
const errorMessage = ref('');
const successMessage = ref('');
const showPassword = ref(false);
const auth = useAuth();
const api = useApi();
const { theme, setTheme } = useTheme();

// 默认账号提示仅开发模式展示，避免生产环境泄露默认凭据
const showDemoCredentials = import.meta.env.DEV;

const usernameInput = ref<HTMLInputElement | null>(null);
const passwordInput = ref<HTMLInputElement | null>(null);

const usernameError = ref(false);
const passwordError = ref(false);

function clearMessages() {
  errorMessage.value = '';
  successMessage.value = '';
  usernameError.value = false;
  passwordError.value = false;
}

function closeError() {
  errorMessage.value = '';
}

function closeSuccess() {
  successMessage.value = '';
}

function showError(msg: string) {
  errorMessage.value = '';
  successMessage.value = '';
  errorMessage.value = msg;
}

function togglePassword() {
  showPassword.value = !showPassword.value;
}

function handleLinkClick(msg: string) {
  showError(msg);
}

function checkExistingSession() {
  if (auth.isAuthenticated()) {
    auth.startAutoRenewal();
    window.location.href = pageUrl('workspace');
  }
}

onMounted(() => {
  document.documentElement.classList.add('login-page');
  document.body.classList.add('login-page');
  checkExistingSession();
  if (usernameInput.value) {
    usernameInput.value.focus();
  }
});

onUnmounted(() => {
  document.documentElement.classList.remove('login-page');
  document.body.classList.remove('login-page');
});

function extractLoginErrorMessage(payload: unknown, fallback: string): string {
  if (!payload || typeof payload !== 'object') {
    return fallback;
  }
  const record = payload as Record<string, unknown>;
  if (typeof record.detail === 'string' && record.detail.trim()) {
    return record.detail;
  }
  if (typeof record.message === 'string' && record.message.trim()) {
    return record.message;
  }
  const nested = record.error;
  if (typeof nested === 'string' && nested.trim()) {
    return nested;
  }
  if (nested && typeof nested === 'object') {
    const error = nested as Record<string, unknown>;
    if (typeof error.message === 'string' && error.message.trim()) {
      return error.message;
    }
    if (typeof error.detail === 'string' && error.detail.trim()) {
      return error.detail;
    }
  }
  return fallback;
}

function resolvePostLoginTarget(): string {
  if (typeof window === 'undefined') {
    return pageUrl('workspace');
  }
  const params = new URLSearchParams(window.location.search);
  const redirect = params.get('redirect') || params.get('next') || '';
  if (!redirect) {
    return pageUrl('workspace');
  }
  // Only same-origin relative frontend paths may be used as post-login targets.
  if (redirect.startsWith('/frontend/') && !redirect.startsWith('//') && !redirect.includes('://')) {
    return redirect;
  }
  return pageUrl('workspace');
}

async function handleLogin(event: Event): Promise<void> {
  event.preventDefault();
  clearMessages();

  if (!username.value.trim()) {
    usernameError.value = true;
    showError('请填写用户名');
    usernameInput.value?.focus();
    return;
  }
  if (!password.value) {
    passwordError.value = true;
    showError('请填写密码');
    passwordInput.value?.focus();
    return;
  }

  isLoading.value = true;

  try {
    interface LoginResponse {
      success?: boolean;
      detail?: string;
      message?: string;
      error?: string | { message?: string; detail?: string; code?: string };
      [key: string]: unknown;
    }
    const result = await api.post<LoginResponse>('/api/v2/auth/login', {
      username: username.value.trim(),
      password: password.value
    });

    if (!result.ok || result.data?.success === false) {
        throw new Error(extractLoginErrorMessage(result.data, '登录失败'));
    }

    auth.saveToken(result.data as unknown as AuthTokenData);

    successMessage.value = '登录成功，正在跳转...';

    setTimeout(() => {
        // 若登录页被错误地嵌在 iframe 内，跳转顶层，避免工作区里再套登录/工作区
        const target = resolvePostLoginTarget();
        auth.navigateAfterLogin(target);
    }, 1000);

  } catch (error: unknown) {
      console.error('Login error:', error);
      passwordError.value = true;
      showError((error as { message?: string }).message || '登录失败，请检查用户名和密码');
      passwordInput.value?.focus();
  } finally {
      isLoading.value = false;
  }
}
</script>

<template>
  <div class="login-container">
      <div class="login-card">
        <div class="logo">
          <SvgIcon src="/frontend/icons/plane.svg" :size="36" />
        </div>
        <h1 class="login-title">
          航班监控系统
        </h1>
        <p class="login-subtitle">
          登录以访问系统功能
        </p>

        <div
          id="errorMessage"
          class="error-message"
          :class="{ show: errorMessage }"
          role="alert"
          aria-live="assertive"
          aria-atomic="true"
        >
          <SvgIcon src="/frontend/icons/forbidden.svg" :size="16" />
          <span>{{ errorMessage }}</span>
          <button
            type="button"
            class="message-close"
            aria-label="关闭错误提示"
            @click="closeError"
          >
            &times;
          </button>
        </div>

        <div
          id="successMessage"
          class="success-message"
          :class="{ show: successMessage }"
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          <SvgIcon src="/frontend/icons/ok.svg" :size="16" />
          <span>{{ successMessage }}</span>
          <button
            type="button"
            class="message-close"
            aria-label="关闭成功提示"
            @click="closeSuccess"
          >
            &times;
          </button>
        </div>

        <form id="loginForm" novalidate @submit="handleLogin">
          <div class="form-group">
            <label class="form-label" for="username">用户名</label>
            <input
              id="username"
              ref="usernameInput"
              v-model="username"
              type="text"
              class="form-input"
              :aria-invalid="usernameError"
              placeholder="请输入用户名"
              required
              autocomplete="username"
            >
          </div>

          <div class="form-group">
            <label class="form-label" for="password">密码</label>
            <div class="password-wrapper">
              <input
                id="password"
                ref="passwordInput"
                v-model="password"
                :type="showPassword ? 'text' : 'password'"
                class="form-input"
                :aria-invalid="passwordError"
                placeholder="请输入密码"
                required
                autocomplete="current-password"
              >
              <button
                id="passwordToggleBtn"
                type="button"
                class="password-toggle"
                :aria-label="showPassword ? '隐藏密码' : '显示密码'"
                :aria-pressed="showPassword"
                @click="togglePassword"
              >
                <SvgIcon
                  :src="showPassword ? '/frontend/icons/password_unvisible.svg' : '/frontend/icons/password_visible.svg'"
                  :size="16"
                />
              </button>
            </div>
          </div>

          <div class="login-options">
            <button
              id="forgotPasswordLink"
              type="button"
              class="forgot-password"
              @click="handleLinkClick('请联系管理员重置密码')"
            >
              忘记密码？
            </button>
          </div>

          <button
            id="loginBtn"
            type="submit"
            class="login-btn"
            :class="{ loading: isLoading }"
            :disabled="isLoading"
          >
            {{ isLoading ? '登录中...' : '登录' }}
          </button>
        </form>

        <template v-if="showDemoCredentials">
          <div class="divider">
            <span>测试账号</span>
          </div>

          <div class="demo-credentials">
            <div class="demo-credentials-title">
              默认管理员账号
            </div>
            <p>用户名: <code>admin</code></p>
            <p>密码: <code>admin123</code></p>
          </div>
        </template>

        <div class="login-foot">
          <p class="footer-text">
            没有账号？<button
              id="contactAdminLink"
              type="button"
              class="text-link"
              @click="handleLinkClick('请联系管理员创建账号')"
            >
              联系管理员
            </button>
          </p>
          <!-- 主题是深/浅分段，是持守，不是一颗常亮主按钮 -->
          <div class="seg" role="radiogroup" aria-label="主题">
            <button
              type="button"
              role="radio"
              :aria-checked="theme === 'dark'"
              @click="setTheme('dark')"
            >深</button>
            <button
              type="button"
              role="radio"
              :aria-checked="theme === 'light'"
              @click="setTheme('light')"
            >浅</button>
          </div>
        </div>
      </div>
  </div>
</template>

<!-- Unscoped: body.login-page must win over global base body rules. -->
<style>
html.login-page,
html.login-page body.login-page {
  height: 100%;
  overflow: hidden;
}

/* 页底保留现场照片（压暗衬底），卡片是抬起面实色坐在其上 */
body.login-page {
  background: linear-gradient(rgba(0, 0, 0, 0.45), rgba(0, 0, 0, 0.45)),
    url('/frontend/images/index-pic-01.jpg') center / cover no-repeat fixed !important;
  background-color: var(--face-page) !important;
  min-height: 100vh !important;
  height: 100% !important;
  margin: 0 !important;
  padding: 0 !important;
  display: block !important;
  box-sizing: border-box;
}

body.login-page #app {
  min-height: 100vh;
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding-right: 10%;
  box-sizing: border-box;
}

@media (max-width: 480px) {
  body.login-page #app {
    padding: 20px;
    justify-content: center;
    align-items: center;
  }
}
</style>

<style scoped>
/* 信号面变位（标本 frontend/signal-surface-preview.html）：
   卡片是抬起面；输入是表单高 36 的器；登录是页级 40 主按钮；
   败=危衬横幅，成=安衬横幅；主题=深/浅分段持守。 */
.login-container {
  width: 100%;
  max-width: 400px;
}

.login-card {
  background: var(--face-raised);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-md);
  padding: var(--s5);
  text-align: left;
  display: flex;
  flex-direction: column;
  justify-content: center;
  max-height: 92vh;
  overflow-y: auto;
  box-sizing: border-box;
}

.logo {
  color: var(--ink);
  margin-bottom: var(--s3);
}

.login-title {
  font-size: var(--fs-page);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  margin: 0 0 var(--s1);
}

.login-subtitle {
  font-size: var(--fs-body);
  color: var(--ink-subtle);
  margin: 0 0 var(--s4);
}

.form-group {
  margin-bottom: var(--s3);
  text-align: left;
}

.form-label {
  display: block;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
  margin-bottom: var(--s1);
}

.form-input {
  width: 100%;
  height: var(--h-md);
  padding: 0 10px;
  border-radius: var(--r-control);
  border: 1px solid var(--line-strong);
  background: var(--face-page);
  color: var(--ink);
  font-size: var(--fs-body);
  outline: none;
  box-sizing: border-box;
  transition: border-color var(--t-fast) var(--ease);
}

.form-input::placeholder {
  color: var(--ink-muted);
  opacity: 1;
}

.form-input:hover {
  border-color: var(--act);
}

.form-input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

/* 败：危描边 + 危衬，不是换一套皮肤 */
.form-input[aria-invalid="true"] {
  border-color: var(--danger);
  background: var(--danger-soft);
}

.password-wrapper {
  position: relative;
}

.password-wrapper .form-input {
  padding-right: 36px;
}

.password-toggle {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  width: 24px;
  height: 24px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: none;
  border: none;
  border-radius: var(--r-cell);
  cursor: pointer;
  color: var(--ink-muted);
  padding: 0;
}

.password-toggle:hover {
  color: var(--ink);
}

.login-options {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  margin-bottom: var(--s4);
}

.forgot-password,
.text-link {
  background: none;
  border: none;
  color: var(--act);
  cursor: pointer;
  font: inherit;
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  text-decoration: none;
  padding: 0;
}

.forgot-password:hover,
.text-link:hover {
  text-decoration: underline;
}

.forgot-password:focus-visible,
.text-link:focus-visible,
.password-toggle:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
  border-radius: var(--r-cell);
}

/* 页级主按钮：本声实底 + 其上（深色实底坐近黑字） */
.login-btn {
  width: 100%;
  height: var(--h-lg);
  padding: 0 16px;
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--act-on);
  background: var(--act);
  border: none;
  border-radius: var(--r-control);
  cursor: pointer;
  position: relative;
  transition: filter var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
}

.login-btn:hover {
  filter: brightness(1.06);
}

.login-btn:active {
  transform: translateY(0.5px);
}

.login-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.login-btn:disabled {
  background: color-mix(in srgb, var(--ink) 7%, transparent);
  color: var(--ink-muted);
  cursor: not-allowed;
  filter: none;
  transform: none;
}

.login-btn.loading::after {
  content: '';
  position: absolute;
  width: 14px;
  height: 14px;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  animation: spin 700ms linear infinite;
  right: 16px;
  top: 50%;
  margin-top: -7px;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* 横幅：败=危衬+危字，成=安衬+安字；入出用 escalate */
.error-message,
.success-message {
  padding: 8px 12px;
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  margin-bottom: var(--s3);
  display: none;
  align-items: center;
  gap: 8px;
}

.error-message {
  background: var(--danger-soft);
  color: var(--danger);
  border: 1px solid color-mix(in srgb, var(--danger) 42%, transparent);
}

.success-message {
  background: var(--ok-soft);
  color: var(--ok);
  border: 1px solid color-mix(in srgb, var(--ok) 42%, transparent);
}

.error-message.show,
.success-message.show {
  display: flex;
  animation: escalate var(--t-slow) var(--ease);
}

@keyframes escalate {
  0% { transform: translateY(-4px); opacity: 0; }
  100% { transform: translateY(0); opacity: 1; }
}

.message-close {
  margin-left: auto;
  border: 0;
  background: transparent;
  color: currentColor;
  font-size: 14px;
  cursor: pointer;
  line-height: 1;
  padding: 2px;
  border-radius: var(--r-cell);
}

.message-close:focus-visible {
  outline: 2px solid currentColor;
  outline-offset: 1px;
}

.divider {
  display: flex;
  align-items: center;
  margin: var(--s4) 0;
  color: var(--ink-muted);
  font-size: var(--fs-label);
}

.divider::before,
.divider::after {
  content: '';
  flex: 1;
  height: 1px;
  background: var(--line);
}

.divider span {
  padding: 0 var(--s3);
}

/* 测试账号块：中性面 + 等宽标识，不用行动衬当背景 */
.demo-credentials {
  background: var(--face-page);
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  padding: var(--s3);
  text-align: left;
  font-size: var(--fs-label);
}

.demo-credentials-title {
  font-weight: var(--fw-medium);
  color: var(--ink);
  margin-bottom: var(--s1);
}

.demo-credentials p {
  color: var(--ink-subtle);
  margin: 4px 0;
}

.demo-credentials code {
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  padding: 1px 5px;
  border-radius: var(--r-cell);
  font-family: var(--mono);
  font-size: 11px;
}

.login-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  margin-top: var(--s4);
}

.footer-text {
  margin: 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.footer-text .text-link {
  font-size: var(--fs-label);
}

/* 深/浅分段：轨道低一档，选中块与所在面同面并抬起 */
.seg {
  display: inline-flex;
  padding: 2px;
  border-radius: var(--r-control);
  background: var(--face-page);
  border: 1px solid var(--line);
  flex-shrink: 0;
}

.seg button {
  height: 28px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--r-cell);
  background: transparent;
  color: var(--ink-subtle);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  cursor: pointer;
}

.seg button:hover {
  color: var(--ink);
}

.seg button[aria-checked="true"],
.seg button[aria-checked="true"]:hover {
  background: var(--face-raised);
  color: var(--ink);
  box-shadow: var(--shadow-sm);
}

.seg button:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

@media (max-height: 700px) {
  .login-card {
    padding: var(--s4);
  }

  .login-subtitle {
    margin-bottom: var(--s3);
  }

  .divider {
    margin: var(--s3) 0;
  }

  .login-foot {
    margin-top: var(--s3);
  }
}

@media (max-width: 480px) {
  .login-container {
    max-width: 100%;
  }

  .login-card {
    padding: var(--s4);
  }
}

@media (prefers-reduced-motion: reduce) {
  .error-message.show,
  .success-message.show {
    animation: none;
  }

  .login-btn,
  .form-input {
    transition: none;
  }
}
</style>
