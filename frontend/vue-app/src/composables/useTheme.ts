import { ref } from 'vue';

type ThemeMode = 'light' | 'dark';

const STORAGE_KEY = 'fms-theme';
const THEME_ORDER: ThemeMode[] = ['light', 'dark'];
/** 跨 iframe / 标签页同步主题 */
const THEME_MSG_TYPE = 'fms-theme-change';

function getInitialTheme(): ThemeMode {
  const stored = localStorage.getItem(STORAGE_KEY) as ThemeMode | null;
  if (stored && THEME_ORDER.includes(stored)) return stored;
  if (window.matchMedia('(prefers-color-scheme: dark)').matches) return 'dark';
  return 'light';
}

const currentTheme = ref<ThemeMode>(getInitialTheme());

function isThemeMode(value: unknown): value is ThemeMode {
  return value === 'light' || value === 'dark';
}

function paintTheme(theme: ThemeMode): void {
  document.documentElement.setAttribute('data-theme', theme);
  document.documentElement.style.colorScheme = theme;
  currentTheme.value = theme;
}

/** 把主题变更广播给同源 parent / 子 iframe（工作区壳 ↔ 嵌入页） */
function broadcastTheme(theme: ThemeMode): void {
  const payload = { type: THEME_MSG_TYPE, theme };
  const origin = window.location.origin;
  try {
    if (window.parent && window.parent !== window) {
      window.parent.postMessage(payload, origin);
    }
  } catch {
    // ignore
  }
  try {
    document.querySelectorAll('iframe').forEach((frame) => {
      try {
        frame.contentWindow?.postMessage(payload, origin);
      } catch {
        // ignore cross-origin or unloaded frame
      }
    });
  } catch {
    // ignore
  }
}

function applyTheme(theme: ThemeMode, options: { broadcast?: boolean } = {}): void {
  const { broadcast = true } = options;
  paintTheme(theme);
  localStorage.setItem(STORAGE_KEY, theme);
  if (broadcast) {
    broadcastTheme(theme);
  }
}

function cycleTheme() {
  const idx = THEME_ORDER.indexOf(currentTheme.value);
  const next = THEME_ORDER[(idx + 1) % THEME_ORDER.length];
  applyTheme(next);
}

function setTheme(theme: ThemeMode) {
  if (THEME_ORDER.includes(theme)) applyTheme(theme);
}

function applyThemeFromPeer(theme: ThemeMode): void {
  if (theme === currentTheme.value) return;
  // 不二次广播，避免 parent↔iframe 死循环；其它兄弟 iframe 由发起方一并 postMessage
  paintTheme(theme);
  localStorage.setItem(STORAGE_KEY, theme);
}

applyTheme(currentTheme.value, { broadcast: false });

// 系统偏好（仅无用户显式选择时）
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
  if (!localStorage.getItem(STORAGE_KEY)) {
    applyTheme(e.matches ? 'dark' : 'light');
  }
});

// 同源其它文档改了 localStorage（含父页切换主题）→ 同步到本 iframe
window.addEventListener('storage', (e) => {
  if (e.key !== STORAGE_KEY || !isThemeMode(e.newValue)) return;
  applyThemeFromPeer(e.newValue);
});

// 工作区壳直接 postMessage 到 iframe（比 storage 更及时，且同页 iframe 更稳）
window.addEventListener('message', (e) => {
  if (e.origin !== window.location.origin) return;
  const data = e.data as { type?: string; theme?: string } | null;
  if (!data || data.type !== THEME_MSG_TYPE || !isThemeMode(data.theme)) return;
  applyThemeFromPeer(data.theme);
});

export function useTheme() {
  return {
    theme: currentTheme,
    cycleTheme,
    setTheme,
    isDark: () => currentTheme.value === 'dark',
  };
}
