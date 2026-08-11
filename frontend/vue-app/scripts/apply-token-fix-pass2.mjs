// scripts/apply-token-fix-pass2.mjs
// 用法: node scripts/apply-token-fix-pass2.mjs
// 作用: 对 B1+B2+B3 并集文件做第二轮微调修复：
//   Pass A: var(--x, var(--y)) 回退修复为 var(--x, <字面量>)（逆映射来自 apply-token-map.mjs 的 RULES）
//   Pass B: 广谱字面量替换（原 RULES + 追加规则），支持多声明行、深色锁定块跳过
// 执行顺序: 先 Pass B 后 Pass A（若先 A 后 B，Pass B 会把 A 写入的回退字面量再次 token 化，形成循环）。
// 输出: 控制台摘要 + docs/plans/token-fix-pass2-report.md
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url)); // frontend/vue-app/
const SRC = join(ROOT, 'src');

// ---- 原替换规则表（与 scripts/apply-token-map.mjs 的 RULES 完全一致）----
const RULES = [
  // 品牌/系统色（任意属性）
  { find: '#007AFF', prop: '*', replace: 'var(--system-blue)' },
  { find: '#0062cc', prop: '*', replace: 'var(--system-blue-hover)' },
  { find: '#FF3B30', prop: '*', replace: 'var(--system-red)' },
  { find: '#34C759', prop: '*', replace: 'var(--system-green)' },
  { find: '#FF9500', prop: '*', replace: 'var(--system-orange)' },
  { find: '#AF52DE', prop: '*', replace: 'var(--system-purple)' },
  { find: '#5AC8FA', prop: '*', replace: 'var(--info-color)' },
  { find: '#5856D6', prop: '*', replace: 'var(--secondary-color)' },
  { find: '#218838', prop: '*', replace: 'var(--system-green)' },
  { find: '#1e5799', prop: '*', replace: 'var(--system-blue)' },
  // 中性文本
  { find: '#1D1D1F', prop: '*', replace: 'var(--text-primary)' },
  { find: '#5f6368', prop: '*', replace: 'var(--text-secondary)' },
  { find: '#8b9097', prop: '*', replace: 'var(--text-tertiary)' },
  { find: '#8E8E93', prop: '*', replace: 'var(--system-gray)' },
  { find: '#AEAEB2', prop: '*', replace: 'var(--system-gray2)' },
  { find: '#666', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#6c757d', prop: 'color', replace: 'var(--text-secondary)' },
  // 白色：文本用反色 token，背景用卡片底色
  { find: '#fff', prop: 'color', replace: 'var(--text-inverse)' },
  { find: '#FFFFFF', prop: 'color', replace: 'var(--text-inverse)' },
  { find: '#fff', prop: 'background', replace: 'var(--bg-card)' },
  { find: '#FFFFFF', prop: 'background', replace: 'var(--bg-card)' },
  { find: '#fff', prop: 'background-color', replace: 'var(--bg-card)' },
  { find: '#FFFFFF', prop: 'background-color', replace: 'var(--bg-card)' },
  { find: '#F5F5F7', prop: '*', replace: 'var(--bg-sidebar)' },
  { find: '#F8FAFC', prop: '*', replace: 'var(--bg-page)' },
  { find: '#F2F2F7', prop: '*', replace: 'var(--system-gray6)' },
  // 边框
  { find: 'rgba(0, 0, 0, 0.08)', prop: 'border-color', replace: 'var(--border-light)' },
  { find: 'rgba(0, 0, 0, 0.06)', prop: 'border-color', replace: 'var(--border-light)' },
  { find: 'rgba(0,0,0,0.08)', prop: 'border-color', replace: 'var(--border-light)' },
  { find: '#ddd', prop: 'border-color', replace: 'var(--border-light)' },
  // 状态胶囊底色/文字
  { find: '#E3F2FD', prop: '*', replace: 'var(--status-bg-scheduled)' },
  { find: '#FFF4E5', prop: '*', replace: 'var(--status-bg-boarding)' },
  { find: '#FFF3E0', prop: '*', replace: 'var(--status-bg-boarding)' },
  { find: '#FBE9E7', prop: '*', replace: 'var(--status-bg-boarding-ended)' },
  { find: '#E8F5E9', prop: '*', replace: 'var(--status-bg-departed)' },
  { find: '#FFEBEE', prop: '*', replace: 'var(--status-bg-delayed)' },
  { find: '#F5F5F5', prop: '*', replace: 'var(--status-bg-arrived)' },
  { find: '#ECEFF1', prop: '*', replace: 'var(--status-bg-cancelled)' },
  { find: '#FFF8E1', prop: '*', replace: 'var(--status-bg-checkin-end)' },
  { find: '#E0F2F1', prop: '*', replace: 'var(--status-bg-next-arrived)' },
  { find: '#1565C0', prop: 'color', replace: 'var(--status-text-scheduled)' },
  { find: '#EF6C00', prop: 'color', replace: 'var(--status-text-boarding)' },
  { find: '#D84315', prop: 'color', replace: 'var(--status-text-boarding-ended)' },
  { find: '#2E7D32', prop: 'color', replace: 'var(--status-text-departed)' },
  { find: '#C62828', prop: 'color', replace: 'var(--status-text-delayed)' },
  { find: '#616161', prop: 'color', replace: 'var(--status-text-arrived)' },
  { find: '#546E7A', prop: 'color', replace: 'var(--status-text-cancelled)' },
  { find: '#1976D2', prop: 'color', replace: 'var(--status-text-prev-departed)' },
  { find: '#F57C00', prop: 'color', replace: 'var(--status-text-checkin-end)' },
  { find: '#D32F2F', prop: 'color', replace: 'var(--status-text-boarding-urge)' },
  { find: '#00796B', prop: 'color', replace: 'var(--status-text-next-arrived)' },
  { find: '#0369A1', prop: 'color', replace: 'var(--status-text-prev-departed)' },
  { find: '#166534', prop: 'color', replace: 'var(--status-text-departed)' },
  { find: '#92400E', prop: 'color', replace: 'var(--status-text-checkin-end)' },
  { find: '#DC2626', prop: 'color', replace: 'var(--status-text-boarding-urge)' },
  { find: '#047857', prop: 'color', replace: 'var(--status-text-next-arrived)' },
  { find: '#6B7280', prop: 'color', replace: 'var(--status-text-cancelled)' },
  { find: '#c3e6cb', prop: '*', replace: 'var(--success-border-subtle)' },
];

// ---- Pass B 追加规则 ----
// prop: 'color' | 'background'(= background|background-color) | 'border*'(= border 开头或 outline-color) | '*'
// findRe: 正则规则，捕获组 1 为 alpha，可配 alphaMin/alphaMax/alphaEq 约束
function rgbaRule(r, g, b, prop, replace, opts = {}) {
  return {
    findRe: new RegExp(`rgba\\(\\s*${r}\\s*,\\s*${g}\\s*,\\s*${b}\\s*,\\s*([\\d.]+)\\s*\\)`, 'gi'),
    prop,
    replace,
    ...opts,
  };
}

const NEW_RULES = [
  // ---- 文字色（prop: color）----
  { find: '#0f172a', prop: 'color', replace: 'var(--text-primary)' },
  { find: '#1e293b', prop: 'color', replace: 'var(--text-primary)' },
  { find: '#1f2937', prop: 'color', replace: 'var(--text-primary)' },
  { find: '#0e2138', prop: 'color', replace: 'var(--text-primary)' },
  { find: '#1a1a2e', prop: 'color', replace: 'var(--text-primary)' },
  { find: '#333', prop: 'color', replace: 'var(--text-primary)' },
  { find: '#334155', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#475569', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#444', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#2f3d4d', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#27466b', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#1d3c5a', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#5a6d82', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#42576d', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#31557f', prop: 'color', replace: 'var(--text-secondary)' },
  { find: '#555', prop: 'color', replace: 'var(--text-secondary)' },
  rgbaRule(15, 23, 42, 'color', 'var(--text-primary)', { alphaEq: 0.94 }),
  rgbaRule(37, 50, 67, 'color', 'var(--text-secondary)', { alphaEq: 0.88 }),
  { find: '#8B2252', prop: 'color', replace: 'var(--system-red)' },
  { find: '#991b1b', prop: 'color', replace: 'var(--ws-danger)' },
  { find: '#b91c1c', prop: 'color', replace: 'var(--ws-danger)' },
  { find: '#9a3412', prop: 'color', replace: 'var(--ws-warn)' },
  { find: '#b45309', prop: 'color', replace: 'var(--ws-warn)' },
  { find: '#166534', prop: 'color', replace: 'var(--ws-success)' },
  { find: '#1d9a6c', prop: 'color', replace: 'var(--ws-success)' },
  { find: '#125f9f', prop: 'color', replace: 'var(--ws-primary)' },
  // ---- 浅色表面（prop: background | background-color）----
  { find: '#f8fafc', prop: 'background', replace: 'var(--bg-page)' },
  { find: '#f0f2f5', prop: 'background', replace: 'var(--bg-page)' },
  { find: '#f1f6ff', prop: 'background', replace: 'var(--bg-page)' },
  { find: '#f5f7fb', prop: 'background', replace: 'var(--bg-page)' },
  { find: '#eef2f8', prop: 'background', replace: 'var(--bg-page)' },
  { find: '#edf4fc', prop: 'background', replace: 'var(--bg-page)' },
  { find: '#eef4fc', prop: 'background', replace: 'var(--bg-page)' },
  { find: '#f1f5f9', prop: 'background', replace: 'var(--ws-surface-muted)' },
  { find: '#fafafa', prop: 'background', replace: 'var(--ws-surface-muted)' },
  { find: '#f8f9fa', prop: 'background', replace: 'var(--ws-surface-muted)' },
  { find: '#f1f3f4', prop: 'background', replace: 'var(--ws-surface-muted)' },
  { find: '#F3F4F6', prop: 'background', replace: 'var(--ws-surface-muted)' },
  { find: '#fcfdfe', prop: 'background', replace: 'var(--bg-card)' },
  { find: '#e2e8f0', prop: 'background', replace: 'var(--bg-input)' },
  { find: '#e9ecef', prop: 'background', replace: 'var(--bg-input)' },
  { find: '#E3E3E8', prop: 'background', replace: 'var(--bg-input)' },
  rgbaRule(255, 255, 255, 'background', 'var(--glass-bg)', { alphaMin: 0.5 }),
  rgbaRule(248, 250, 252, 'background', 'var(--glass-bg)'),
  rgbaRule(246, 250, 255, 'background', 'var(--glass-bg)'),
  rgbaRule(247, 250, 255, 'background', 'var(--glass-bg)'),
  rgbaRule(240, 247, 255, 'background', 'var(--glass-bg)'),
  rgbaRule(245, 247, 251, 'background', 'var(--glass-bg)'),
  // ---- 徽章 pastel 底（prop: background | background-color）----
  { find: '#fee2e2', prop: 'background', replace: 'var(--dh-signal-critical-soft)' },
  { find: '#FEF2F2', prop: 'background', replace: 'var(--dh-signal-critical-soft)' },
  { find: '#fff0f0', prop: 'background', replace: 'var(--dh-signal-critical-soft)' },
  { find: '#fff1f0', prop: 'background', replace: 'var(--dh-signal-critical-soft)' },
  { find: '#fef3c7', prop: 'background', replace: 'var(--dh-signal-warn-soft)' },
  { find: '#FFF7ED', prop: 'background', replace: 'var(--dh-signal-warn-soft)' },
  { find: '#FEFCE8', prop: 'background', replace: 'var(--dh-signal-warn-soft)' },
  rgbaRule(255, 247, 237, 'background', 'var(--dh-signal-warn-soft)'),
  { find: '#dcfce7', prop: 'background', replace: 'var(--dh-signal-ok-soft)' },
  { find: '#D1FAE5', prop: 'background', replace: 'var(--dh-signal-ok-soft)' },
  { find: '#e0f2fe', prop: 'background', replace: 'var(--dh-signal-accent-soft)' },
  { find: '#F0F7FF', prop: 'background', replace: 'var(--dh-signal-accent-soft)' },
  { find: '#eff6ff', prop: 'background', replace: 'var(--dh-signal-accent-soft)' },
  { find: '#f5faff', prop: 'background', replace: 'var(--dh-signal-accent-soft)' },
  { find: '#e9f3ff', prop: 'background', replace: 'var(--dh-signal-accent-soft)' },
  // ---- 边框（prop: border*）----
  { find: '#ddd', prop: 'border*', replace: 'var(--border-light)' },
  { find: '#d2d2d7', prop: 'border*', replace: 'var(--border-light)' },
  { find: '#e5e5e5', prop: 'border*', replace: 'var(--border-light)' },
  { find: '#e5e5ea', prop: 'border*', replace: 'var(--border-light)' },
  { find: '#e5e7eb', prop: 'border*', replace: 'var(--border-light)' },
  { find: '#d1d5db', prop: 'border*', replace: 'var(--border-light)' },
  { find: '#cbd5e1', prop: 'border*', replace: 'var(--border-light)' },
  { find: '#e2e8f0', prop: 'border*', replace: 'var(--border-light)' },
  { find: '#efefef', prop: 'border*', replace: 'var(--border-light)' },
  rgbaRule(0, 0, 0, 'border*', 'var(--border-light)', { alphaMin: 0.04, alphaMax: 0.25 }),
  rgbaRule(15, 23, 42, 'border*', 'var(--border-light)'),
  rgbaRule(16, 35, 63, 'border*', 'var(--border-light)'),
  rgbaRule(17, 35, 63, 'border*', 'var(--border-light)'),
  rgbaRule(17, 50, 91, 'border*', 'var(--border-light)'),
];

const EXCLUDE_PARTS = [
  'node_modules', 'dist', 'src/legacy',
  'theme-tokens.css', 'variables.css', 'apple-theme.css',
  'matte-black-overrides.css', 'index.css',
  // pass2 追加排除
  'src/pages/system_status/SystemStatus.vue',
  'src/styles/command_center_dashboard.css',
  'src/styles/kpi_dashboard_enhanced.css',
];

// 与原脚本 BATCHES 一致；pass2 处理 B1+B2+B3 并集
const BATCHES = {
  B1: [
    'src/styles/components.css',
    'src/styles/tables.css',
    'src/styles/command_center_dashboard.css',
    'src/styles/kpi_dashboard_enhanced.css',
    'src/styles/dashboard_frontline_workbench.css',
    'src/styles/dashboard_handover.css',
    'src/styles/dispatch-board.css',
    'src/pages/dashboard/',
    'src/pages/flight_monitor/',
    'src/pages/command_center/',
    'src/pages/dispatch_board/',
    'src/pages/kpi_dashboard/',
    'src/components/flight-monitor/',
    'src/components/dispatch-board/',
    'src/components/dispatch/',
  ],
  B2: [
    'src/styles/flight-imports.css',
    'src/styles/dispatch-rule-center.css',
    'src/styles/workspace_unified_theme.css',
    'src/pages/flight_imports/',
    'src/pages/resource_manager/',
    'src/pages/resource_utilization/',
    'src/pages/anomaly_monitor/',
    'src/pages/system_status/',
    'src/pages/system_flags/',
    'src/pages/dispatch_rule_center/',
  ],
  B3: [
    'src/styles/flowable-modeler.css',
    'src/styles/admin-layout.css',
    'src/styles/admin-page.css',
    'src/styles/page-tabs.css',
    'src/styles/layout.css',
    'src/styles/base.css',
    'src/styles/legacy_sync.css',
    'src/styles/main.css',
    'src/components/',          // B1 已处理过的子目录会因幂等而零改动
    'src/pages/ai_config_center/',
    'src/pages/ai_monitor/',
    'src/pages/llm_eval_lab/',
    'src/pages/nl_query/',
    'src/pages/label_manager/',
    'src/pages/user_manager/',
    'src/pages/login/',
    'src/pages/operations_review_report/',
    'src/pages/flowable_modeler/',
  ],
};

const LOCKED_SEL_RE = /\.(nl-query-page|data-hub-page)/;

function escapeRegExp(s) { return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }

// 预编译全部 Pass B 规则
const ALL_RULES = [...RULES, ...NEW_RULES].map((rule) => {
  if (rule.findRe) return rule;
  return { ...rule, findRe: new RegExp(`(?<![\\w#])${escapeRegExp(rule.find)}(?![\\w])`, 'gi') };
});

// ---- Pass A 逆映射：token 名 -> 字面量（同一 token 以 RULES 中先出现的为准）----
const INVERSE_MAP = new Map();
for (const rule of RULES) {
  const m = rule.replace.match(/^var\(\s*(--[\w-]+)\s*\)$/);
  if (!m) continue;
  if (!INVERSE_MAP.has(m[1])) INVERSE_MAP.set(m[1], rule.find);
}

function collectFiles(entry, out = []) {
  if (EXCLUDE_PARTS.some((p) => entry.includes(p))) return out;
  const abs = join(ROOT, entry);
  if (!statSync(abs, { throwIfNoEntry: false })) return out;
  const st = statSync(abs);
  if (st.isDirectory()) {
    for (const name of readdirSync(abs)) {
      collectFiles(`${entry}/${name}`, out); // 保持正斜杠，保证 EXCLUDE_PARTS 子串匹配跨平台生效
    }
  } else if (/\.(css|vue)$/.test(entry)) {
    out.push(abs);
  }
  return out;
}

// ---- Pass A: var(--x, var(--y)) 回退修复（不受深色锁定块限制）----
const FALLBACK_RE = /var\(\s*(--[\w-]+)\s*,\s*var\(\s*(--[\w-]+)\s*\)\s*\)/g;
function passAFix(text) {
  let hits = 0;
  const skipped = new Map(); // token -> count
  const out = text.replace(FALLBACK_RE, (all, outer, inner) => {
    if (INVERSE_MAP.has(inner)) {
      hits += 1;
      return `var(${outer}, ${INVERSE_MAP.get(inner)})`;
    }
    skipped.set(inner, (skipped.get(inner) || 0) + 1);
    return all;
  });
  return { text: out, hits, skipped };
}

function propMatches(ruleProp, prop) {
  if (ruleProp === '*') return true;
  if (ruleProp === 'border*') return prop.startsWith('border') || prop === 'outline-color';
  if (ruleProp === 'background') return prop === 'background' || prop === 'background-color';
  return ruleProp === prop;
}

function applyRulesToValue(value, prop) {
  let v = value;
  let hits = 0;
  for (const rule of ALL_RULES) {
    if (!propMatches(rule.prop, prop)) continue;
    if (rule.alphaMin !== undefined || rule.alphaMax !== undefined || rule.alphaEq !== undefined) {
      v = v.replace(rule.findRe, (all, alpha) => {
        const a = parseFloat(alpha);
        if (rule.alphaMin !== undefined && a < rule.alphaMin) return all;
        if (rule.alphaMax !== undefined && a > rule.alphaMax) return all;
        if (rule.alphaEq !== undefined && Math.abs(a - rule.alphaEq) > 1e-9) return all;
        hits += 1;
        return rule.replace;
      });
    } else {
      v = v.replace(rule.findRe, () => { hits += 1; return rule.replace; });
    }
  }
  return { value: v, hits };
}

// 增强 1：多声明行 —— 按 ';' 切分，逐段独立应用规则后拼回（保持分隔与行尾结构）
function processDeclText(piece) {
  let hits = 0;
  const parts = piece.split(';');
  const out = parts.map((frag) => {
    const m = frag.match(/^(\s*)([\w-]+)(\s*:\s*)(.*?)(\s*)$/);
    if (!m || m[4] === '') return frag;
    const [, pre, prop, colon, value, post] = m;
    const r = applyRulesToValue(value, prop);
    if (r.hits === 0) return frag;
    hits += r.hits;
    return `${pre}${prop}${colon}${r.value}${post}`;
  });
  return { text: out.join(';'), hits };
}

// ---- Pass B: 行处理 + 花括号/选择器栈跟踪（增强 2：深色锁定块跳过）----
function passBCssText(text) {
  const lines = text.split('\n');
  const selStack = [];
  let pendingSel = '';
  let hits = 0;
  const next = lines.map((line) => {
    const t = line.trim();
    if (t.startsWith('//') || t.startsWith('/*') || t.startsWith('*') || t.startsWith('*/')) return line;
    const segs = line.split(/([{}])/);
    let lineOut = '';
    for (const seg of segs) {
      if (seg === '{') {
        selStack.push(pendingSel);
        pendingSel = '';
        lineOut += seg;
      } else if (seg === '}') {
        selStack.pop();
        pendingSel = '';
        lineOut += seg;
      } else {
        if (seg.trim()) pendingSel += ` ${seg.trim()}`;
        if (selStack.length >= 1 && seg.includes(':')) {
          const locked = selStack.some((s) => LOCKED_SEL_RE.test(s));
          if (!locked) {
            const r = processDeclText(seg);
            hits += r.hits;
            lineOut += r.text;
          } else {
            lineOut += seg;
          }
        } else {
          lineOut += seg;
        }
      }
    }
    return lineOut;
  });
  return { text: next.join('\n'), hits };
}

function processCssText(text) {
  // 先 Pass B 后 Pass A：避免 Pass B 把 Pass A 写入的回退字面量再次 token 化
  const b = passBCssText(text);
  const a = passAFix(b.text);
  return { text: a.text, hitsB: b.hits, hitsA: a.hits, skipped: a.skipped };
}

function processVueText(text) {
  let hitsB = 0;
  let hitsA = 0;
  const skipped = new Map();
  const next = text.replace(/(<style[^>]*>)([\s\S]*?)(<\/style>)/g, (all, open, body, close) => {
    const r = processCssText(body);
    hitsB += r.hitsB;
    hitsA += r.hitsA;
    for (const [tok, n] of r.skipped) skipped.set(tok, (skipped.get(tok) || 0) + n);
    return open + r.text + close;
  });
  return { text: next, hitsB, hitsA, skipped };
}

function scanLeftovers(text) {
  const found = [];
  const re = /#[0-9a-fA-F]{3,8}\b|rgba?\([^)]*\)/g;
  text.split('\n').forEach((line, i) => {
    if (line.includes('var(--')) return;
    let m;
    while ((m = re.exec(line))) found.push({ line: i + 1, literal: m[0] });
  });
  return found;
}

const files = [...new Set(Object.values(BATCHES).flat().flatMap((e) => collectFiles(e)))];
const report = ['# token-fix-pass2 报告', ''];
let totalA = 0;
let totalB = 0;
const globalSkipped = new Map();
for (const abs of files) {
  const rel = relative(ROOT, abs);
  const src = readFileSync(abs, 'utf8');
  const r = rel.endsWith('.vue') ? processVueText(src) : processCssText(src);
  if (r.hitsA > 0 || r.hitsB > 0) writeFileSync(abs, r.text);
  totalA += r.hitsA;
  totalB += r.hitsB;
  for (const [tok, n] of r.skipped) globalSkipped.set(tok, (globalSkipped.get(tok) || 0) + n);
  const leftovers = scanLeftovers(r.text);
  report.push(`## ${rel}`, '', `- Pass A 修复: ${r.hitsA} 处`, `- Pass B 替换: ${r.hitsB} 处`, `- 残留字面量: ${leftovers.length} 处`);
  for (const l of leftovers.slice(0, 50)) report.push(`  - L${l.line}: \`${l.literal}\``);
  if (leftovers.length > 50) report.push(`  - …另有 ${leftovers.length - 50} 处`);
  report.push('');
}
report.push('## 汇总', '', `- 文件数: ${files.length}`, `- Pass A 修复合计: ${totalA} 处`, `- Pass B 替换合计: ${totalB} 处`);
if (globalSkipped.size > 0) {
  report.push('- Pass A 跳过（内层 token 不在逆映射中）:');
  for (const [tok, n] of [...globalSkipped.entries()].sort()) report.push(`  - \`${tok}\`: ${n} 次`);
} else {
  report.push('- Pass A 跳过: 无');
}
report.push('');
writeFileSync(join(ROOT, '../../docs/plans/', 'token-fix-pass2-report.md'), report.join('\n'));
console.log(`[pass2] 处理 ${files.length} 个文件，Pass A 修复 ${totalA} 处，Pass B 替换 ${totalB} 处。报告已写入 docs/plans/token-fix-pass2-report.md`);
if (globalSkipped.size > 0) {
  console.log(`[pass2] Pass A 跳过 token: ${[...globalSkipped.entries()].map(([t, n]) => `${t}(${n})`).join(', ')}`);
}
