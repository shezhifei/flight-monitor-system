// scripts/apply-token-map.mjs
// 用法: node scripts/apply-token-map.mjs B1|B2|B3
// 作用: 将指定批次文件中的硬编码颜色字面量替换为 CSS 变量 token。
// 输出: 控制台摘要 + docs/plans/token-codemod-<batch>-report.md
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url)); // frontend/vue-app/
const SRC = join(ROOT, 'src');

// ---- 替换规则表（prop: '*' 表示任意属性；否则仅在该 CSS 属性上生效）----
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

const EXCLUDE_PARTS = [
  'node_modules', 'dist', 'src/legacy',
  'theme-tokens.css', 'variables.css', 'apple-theme.css',
  'matte-black-overrides.css', 'index.css',
];

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

function escapeRegExp(s) { return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }

function collectFiles(entry, out = []) {
  const abs = join(ROOT, entry);
  if (EXCLUDE_PARTS.some((p) => entry.includes(p))) return out;
  if (!statSync(abs, { throwIfNoEntry: false })) return out;
  const st = statSync(abs);
  if (st.isDirectory()) {
    for (const name of readdirSync(abs)) {
      collectFiles(join(entry, name), out);
    }
  } else if (/\.(css|vue)$/.test(entry)) {
    out.push(abs);
  }
  return out;
}

function processCssText(text) {
  const lines = text.split('\n');
  let hits = 0;
  const next = lines.map((line) => {
    const t = line.trim();
    if (t.startsWith('//') || t.startsWith('/*') || t.startsWith('*')) return line;
    const m = line.match(/^(\s*)([\w-]+)\s*:\s*([^;]+)(;.*)$/);
    if (!m) return line;
    const [, indent, prop, value, tail] = m;
    let v = value;
    for (const rule of RULES) {
      if (rule.prop !== '*' && rule.prop !== prop) continue;
      const re = new RegExp(`(?<![\\w#])${escapeRegExp(rule.find)}(?![\\w])`, 'gi');
      v = v.replace(re, () => { hits += 1; return rule.replace; });
    }
    return v === value ? line : `${indent}${prop}: ${v}${tail}`;
  });
  return { text: next.join('\n'), hits };
}

function processVueText(text) {
  let hits = 0;
  const next = text.replace(/(<style[^>]*>)([\s\S]*?)(<\/style>)/g, (all, open, body, close) => {
    const r = processCssText(body);
    hits += r.hits;
    return open + r.text + close;
  });
  return { text: next, hits };
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

const batch = process.argv[2];
if (!BATCHES[batch]) { console.error('用法: node scripts/apply-token-map.mjs B1|B2|B3'); process.exit(1); }

const files = [...new Set(BATCHES[batch].flatMap((e) => collectFiles(e)))];
const report = [`# token-codemod ${batch} 报告`, ''];
let totalHits = 0;
for (const abs of files) {
  const rel = relative(ROOT, abs);
  const src = readFileSync(abs, 'utf8');
  const r = rel.endsWith('.vue') ? processVueText(src) : processCssText(src);
  if (r.hits > 0) writeFileSync(abs, r.text);
  totalHits += r.hits;
  const leftovers = scanLeftovers(r.text);
  report.push(`## ${rel}`, '', `- 替换: ${r.hits} 处`, `- 残留字面量: ${leftovers.length} 处（留待最终微调，勿手动处理）`);
  for (const l of leftovers.slice(0, 50)) report.push(`  - L${l.line}: \`${l.literal}\``);
  if (leftovers.length > 50) report.push(`  - …另有 ${leftovers.length - 50} 处`);
  report.push('');
}
writeFileSync(join(ROOT, '../../docs/plans/', `token-codemod-${batch}-report.md`), report.join('\n'));
console.log(`[${batch}] 处理 ${files.length} 个文件，共替换 ${totalHits} 处。报告已写入 docs/plans/token-codemod-${batch}-report.md`);
