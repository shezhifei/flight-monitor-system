<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { pageUrl } from '@/shared/page-routes';
import { useAuth, type AuthTokenData } from '@/composables/useAuth';
import { useApi } from '@/composables/useApi';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';

const username = ref('');
const password = ref('');
const isLoading = ref(false);
const errorMessage = ref('');
const successMessage = ref('');
const showPassword = ref(false);
const auth = useAuth();
const api = useApi();

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
  // Match legacy html/login.html: background + right-aligned flex live on body,
  // so backdrop-filter on .login-card samples the same fixed viewport background.
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
  <!-- Layout shell is applied on body.login-page (legacy parity). -->
  <div class="login-container">
      <div class="login-card">
        <div class="logo">
          <img
            src="/frontend/icons/plane.svg"
            alt=""
            aria-hidden="true"
            style="width:48px;height:48px;filter:invert(40%) sepia(98%) saturate(1500%) hue-rotate(190deg) brightness(100%) contrast(101%);"
          >
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
          <SvgIcon src="/frontend/icons/forbidden.svg" />
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
          <SvgIcon src="/frontend/icons/ok.svg" />
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
                  :size="18"
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
      </div>
      <ThemeToggle />
  </div>
</template>

<!-- Unscoped: body.login-page must win over global base body rules. -->
<style>
html.login-page,
html.login-page body.login-page {
  height: 100%;
  overflow: hidden;
}

/* Background on body (legacy); flex shell on #app so compositing matches. */
body.login-page {
  background: linear-gradient(rgba(0, 0, 0, 0.45), rgba(0, 0, 0, 0.45)),
    url('/frontend/images/index-pic-01.jpg') center / cover no-repeat fixed !important;
  background-color: transparent !important;
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
/*
 * Strict visual parity with legacy html/login.html AS RENDERED.
 * Global apple-theme/components selectors beat unscoped .form-input on legacy;
 * Vue scoped styles must not re-introduce higher-priority field chrome that
 * diverges from that cascade outcome.
 */
.login-container {
  width: 100%;
  max-width: 460px;
  height: 61.8vh;
  min-height: 500px;
  max-height: 90vh;
}

.login-card {
  background: var(--glass-bg);
  -webkit-backdrop-filter: saturate(180%) blur(20px);
  backdrop-filter: saturate(180%) blur(20px);
  border-radius: var(--radius-l);
  box-shadow: var(--shadow-lg);
  padding: 40px 48px;
  text-align: left;
  border: 1px solid rgba(255, 255, 255, 0.4);
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  overflow-y: auto;
  box-sizing: border-box;
}

.divider {
  display: flex;
  align-items: center;
  margin: 24px 0;
  color: var(--text-secondary);
  font-size: 13px;
}

.divider::before,
.divider::after {
  content: '';
  flex: 1;
  height: 1px;
  background: var(--border-light);
}

.divider span {
  padding: 0 16px;
}

.demo-credentials {
  background: var(--system-blue-subtle);
  border-radius: 12px;
  padding: 16px;
  text-align: left;
  font-size: 13px;
}

.demo-credentials-title {
  font-weight: 600;
  color: var(--system-blue);
  margin-bottom: 8px;
}

.demo-credentials p {
  color: var(--text-secondary);
  margin: 4px 0;
}

.demo-credentials code {
  background: rgba(0, 0, 0, 0.05);
  padding: 2px 6px;
  border-radius: 4px;
  font-family: "SF Mono", Monaco, monospace;
  font-size: 12px;
}

.logo {
  font-size: 48px;
  margin-bottom: 16px;
}

.login-title {
  font-size: 28px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0 0 8px;
  letter-spacing: -0.02em;
}

.login-subtitle {
  font-size: 15px;
  color: var(--text-secondary);
  margin: 0 0 32px;
}

.form-group {
  margin-bottom: 20px;
  text-align: left;
}

.form-label {
  display: block;
  /* Legacy cascade yields ~14px/21px label box (not the 13px page rule). */
  font-size: 14px;
  line-height: 21px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}

/*
 * Field chrome intentionally deferred to global apple-theme / components rules
 * so Vue matches legacy cascade winners (see style comparison diagnostic).
 * Legacy does not paint a distinct invalid field surface — only the banner.
 */
.form-input {
  width: 100%;
  outline: none;
  box-sizing: border-box;
  color: var(--text-primary);
}

.form-input::placeholder {
  color: var(--text-secondary);
  opacity: 1;
}

.password-wrapper {
  position: relative;
}

.password-toggle {
  position: absolute;
  right: 14px;
  top: 50%;
  transform: translateY(-50%);
  background: none;
  border: none;
  cursor: pointer;
  color: var(--text-secondary);
  font-size: 18px;
  padding: 4px;
}

.password-toggle:hover {
  color: var(--text-primary);
}

.password-toggle:focus-visible {
  outline: 2px solid rgba(0, 122, 255, 0.35);
  outline-offset: 2px;
  border-radius: 6px;
}

.login-options {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
  font-size: 13px;
}

.remember-me {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text-secondary);
  cursor: pointer;
}

.remember-me input {
  accent-color: var(--system-blue);
  width: 16px;
  height: 16px;
}

.forgot-password {
  background: none;
  border: none;
  color: var(--system-blue);
  cursor: pointer;
  font: inherit;
  text-decoration: none;
  font-weight: 500;
  padding: 0;
}

.forgot-password:hover {
  text-decoration: underline;
}

.forgot-password:focus-visible,
.text-link:focus-visible {
  outline: 2px solid rgba(0, 122, 255, 0.35);
  outline-offset: 2px;
  border-radius: 4px;
}

.login-btn {
  width: 100%;
  /* Legacy measured content-box total is 53px with 16px vertical padding. */
  height: 53px;
  padding: 16px 24px;
  font-size: 16px;
  font-weight: 600;
  line-height: normal;
  color: var(--text-inverse);
  background: var(--system-blue);
  border: none;
  border-radius: var(--radius-m);
  cursor: pointer;
  transition: all 0.2s ease;
  position: relative;
  overflow: hidden;
  display: inline-block;
  box-sizing: border-box;
  vertical-align: middle;
}

.login-btn:hover {
  background: var(--system-blue-hover);
  transform: translateY(-1px);
  box-shadow: 0 8px 20px var(--focus-ring-blue);
}

.login-btn:active {
  transform: translateY(0);
}

.login-btn:disabled {
  background: var(--system-gray);
  cursor: not-allowed;
  transform: none;
  box-shadow: none;
}

.login-btn.loading::after {
  content: '';
  position: absolute;
  width: 20px;
  height: 20px;
  border: 2px solid transparent;
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  right: 20px;
  top: 50%;
  transform: translateY(-50%);
}

@keyframes spin {
  to {
    transform: translateY(-50%) rotate(360deg);
  }
}

.error-message {
  background: var(--error-bg-subtle);
  color: var(--system-red);
  padding: 12px 16px;
  border-radius: 12px;
  font-size: 14px;
  margin-bottom: 20px;
  display: none;
  align-items: center;
  gap: 8px;
  border: 1px solid var(--error-border-subtle);
}

.error-message.show {
  display: flex;
}

.success-message {
  background: var(--success-bg-subtle);
  color: var(--system-green);
  padding: 12px 16px;
  border-radius: 12px;
  font-size: 14px;
  margin-bottom: 20px;
  display: none;
  align-items: center;
  gap: 8px;
  border: 1px solid var(--success-border-subtle);
}

.success-message.show {
  display: flex;
}

.message-close {
  margin-left: auto;
  border: 0;
  background: transparent;
  color: currentColor;
  font-size: 16px;
  cursor: pointer;
  line-height: 1;
  padding: 2px;
}

.message-close:focus-visible {
  outline: 2px solid currentColor;
  outline-offset: 1px;
  border-radius: 4px;
}

.svg-icon {
  width: 18px;
  height: 18px;
  vertical-align: middle;
}

.svg-icon-md {
  width: 20px;
  height: 20px;
}

.footer-text {
  margin-top: 24px;
  font-size: 13px;
  color: var(--text-secondary);
}

.text-link {
  background: none;
  border: none;
  color: var(--system-blue);
  cursor: pointer;
  font: inherit;
  text-decoration: none;
  font-weight: 500;
  padding: 0;
}

.text-link:hover {
  text-decoration: underline;
}

@media (max-height: 700px) {
  .login-container {
    height: auto;
    min-height: unset;
    max-height: 95vh;
  }

  .login-card {
    padding: 28px 36px;
  }

  .logo {
    font-size: 36px;
    margin-bottom: 12px;
  }

  .login-title {
    font-size: 24px;
    margin-bottom: 4px;
  }

  .login-subtitle {
    margin-bottom: 20px;
  }

  .form-group {
    margin-bottom: 14px;
  }

  .login-btn {
    padding: 12px 20px;
    font-size: 15px;
  }

  .divider {
    margin: 16px 0;
  }

  .demo-credentials {
    padding: 12px;
  }

  .footer-text {
    margin-top: 16px;
  }
}

@media (max-width: 480px) {
  .login-container {
    height: auto;
    min-height: unset;
    max-width: 100%;
  }

  .login-card {
    padding: 32px 24px;
    height: auto;
  }

  .login-title {
    font-size: 24px;
  }
}
</style>
