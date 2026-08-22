import { computed, ref, watch } from 'vue';
import { useTheme } from './useTheme';

/**
 * 画布取声（信号面 §2 / §3.4）。
 *
 * CSS 里的东西吃 token，画布（ECharts / canvas）吃不到 var()。以前的做法是
 * 在 JS 里另抄一份明暗两套色值 —— 那就是第二套设计语言，主题一改就分叉。
 * 这里改成运行时从 :root 读回 token 的真值，主题一换重读一次：
 * 画布和 DOM 用的是同一批 token，不存在「图表色板」这种东西。
 *
 * 派生洗色由 `alpha()` / `mix()` 现算，对应 CSS 侧的 color-mix，
 * 同样不引入新色值。
 */

const CANVAS_TOKENS = [
  'face-page',
  'face-work',
  'face-raised',
  'line',
  'line-strong',
  'ink',
  'ink-subtle',
  'ink-muted',
  'ink-inverse',
  'act',
  'act-on',
  'act-soft',
  'ok',
  'warn',
  'danger',
  'scrim',
  'sans',
  'mono',
] as const;

export type SignalTokenName = (typeof CANVAS_TOKENS)[number];
export type SignalTokens = Record<SignalTokenName, string>;

/** CSS 尚未落地时的兜底（亮色一档），只为避免画布画出空字符串 */
const FALLBACK: SignalTokens = {
  'face-page': '#edf2f8',
  'face-work': '#ffffff',
  'face-raised': '#ffffff',
  line: 'rgba(16, 35, 63, 0.13)',
  'line-strong': '#7a8494',
  ink: '#11233f',
  'ink-subtle': '#4e5d73',
  'ink-muted': '#5b6b83',
  'ink-inverse': '#ffffff',
  act: '#0066d6',
  'act-on': '#ffffff',
  'act-soft': 'rgba(10, 124, 255, 0.1)',
  ok: '#16794a',
  warn: '#9a5b00',
  danger: '#b52e3e',
  scrim: 'rgba(17, 35, 63, 0.36)',
  sans: '"MiSans", "PingFang SC", system-ui, sans-serif',
  mono: 'ui-monospace, Consolas, monospace',
};

function readTokens(): SignalTokens {
  if (typeof window === 'undefined' || typeof document === 'undefined') return { ...FALLBACK };
  const style = window.getComputedStyle(document.documentElement);
  const out = { ...FALLBACK };
  for (const name of CANVAS_TOKENS) {
    const value = style.getPropertyValue(`--${name}`).trim();
    if (value) out[name] = value;
  }
  return out;
}

const resolved = ref<SignalTokens>(readTokens());
let bound = false;

function bindThemeWatch(): void {
  if (bound) return;
  bound = true;
  const { theme } = useTheme();
  // paintTheme 先写 data-theme 再改 ref，所以 watcher 里读到的已是新主题的真值
  watch(theme, () => {
    resolved.value = readTokens();
  });
}

type Rgba = { r: number; g: number; b: number; a: number };

function parseColor(input: string): Rgba | null {
  const value = input.trim();
  const hex = /^#([0-9a-f]{3,8})$/i.exec(value);
  if (hex) {
    const d = hex[1];
    const expand = (s: string): number => parseInt(s.length === 1 ? s + s : s, 16);
    if (d.length === 3 || d.length === 4) {
      return {
        r: expand(d[0]),
        g: expand(d[1]),
        b: expand(d[2]),
        a: d.length === 4 ? expand(d[3]) / 255 : 1,
      };
    }
    if (d.length === 6 || d.length === 8) {
      return {
        r: expand(d.slice(0, 2)),
        g: expand(d.slice(2, 4)),
        b: expand(d.slice(4, 6)),
        a: d.length === 8 ? expand(d.slice(6, 8)) / 255 : 1,
      };
    }
    return null;
  }
  const fn = /^rgba?\(([^)]+)\)$/i.exec(value);
  if (fn) {
    const parts = fn[1].split(/[,/\s]+/).filter(Boolean).map(Number);
    if (parts.length >= 3 && parts.slice(0, 3).every((n) => Number.isFinite(n))) {
      const a = parts.length > 3 && Number.isFinite(parts[3]) ? parts[3] : 1;
      return { r: parts[0], g: parts[1], b: parts[2], a };
    }
  }
  return null;
}

function toCss({ r, g, b, a }: Rgba): string {
  const round = (n: number) => Math.max(0, Math.min(255, Math.round(n)));
  const alphaOut = Math.max(0, Math.min(1, a));
  return `rgba(${round(r)}, ${round(g)}, ${round(b)}, ${Number(alphaOut.toFixed(3))})`;
}

/** 把已解析的颜色降到给定不透明度 —— 对应 CSS 侧 color-mix(… X%, transparent) */
export function alpha(color: string, value: number): string {
  const parsed = parseColor(color);
  if (!parsed) return color;
  return toCss({ ...parsed, a: parsed.a * value });
}

/** 两色相调，weight 为第一色占比 —— 对应 CSS 侧 color-mix(in srgb, A p%, B) */
export function mix(colorA: string, colorB: string, weight: number): string {
  const a = parseColor(colorA);
  const b = parseColor(colorB);
  if (!a || !b) return colorA;
  const w = Math.max(0, Math.min(1, weight));
  return toCss({
    r: a.r * w + b.r * (1 - w),
    g: a.g * w + b.g * (1 - w),
    b: a.b * w + b.b * (1 - w),
    a: a.a * w + b.a * (1 - w),
  });
}

export function useSignalTokens() {
  bindThemeWatch();
  const { theme } = useTheme();

  return {
    /** 当前主题下 token 的真值；主题切换后自动重读 */
    tokens: computed<SignalTokens>(() => resolved.value),
    isDark: computed(() => theme.value === 'dark'),
    /** 手动重读（字体/样式表迟到加载时用） */
    refresh(): void {
      resolved.value = readTokens();
    },
    alpha,
    mix,
  };
}
