// scripts/apply-token-map-b-class.mjs
// 用法: node scripts/apply-token-map-b-class.mjs
// 作用: 对 B1+B2+B3 并集文件执行保守的 B 类颜色字面量 token 化。
// 输出: 控制台摘要 + docs/plans/b-class-tokenization-report.md
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const REPORT_PATH = join(ROOT, '../../docs/plans/b-class-tokenization-report.md');

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function literalRule(id, find, prop, replace) {
  return {
    id,
    find,
    prop,
    replace,
    findRe: new RegExp(`(?<![\\w#])${escapeRegExp(find)}(?![\\w])`, 'gi'),
  };
}

function rgbaRule(id, r, g, b, prop, replace) {
  return {
    id,
    find: `rgba(${r},${g},${b},x)`,
    prop,
    replace,
    findRe: new RegExp(`rgba\\(\\s*${r}\\s*,\\s*${g}\\s*,\\s*${b}\\s*,\\s*(?:0(?:\\.\\d+)?|1(?:\\.0+)?)\\s*\\)`, 'gi'),
  };
}

const RULES = [];
function addLiterals(id, values, prop, replace) {
  for (const value of values) RULES.push(literalRule(id, value, prop, replace));
}
function addRgbas(id, triples, prop, replace) {
  for (const [r, g, b] of triples) RULES.push(rgbaRule(id, r, g, b, prop, replace));
}

addLiterals('gray.text-tertiary', ['#64748b', '#86868b'], 'color', 'var(--text-tertiary)');
addLiterals('gray.system-gray2', ['#94a3b8'], 'color', 'var(--system-gray2)');
addLiterals('gray.system-gray', ['#6B7280'], 'color', 'var(--system-gray)');

const BLUE = ['#3b82f6', '#2563eb', '#1677ff', '#1d4ed8', '#0a7cff', '#2192ff', '#2b92ff', '#0a80ff', '#0057c2', '#0a5ec2', '#2196f3', '#0ea5e9', '#4169E1'];
addLiterals('blue.system-blue', BLUE, '*', 'var(--system-blue)');
addLiterals('blue.accent-soft', ['#93c5fd', '#bbdefb'], 'background', 'var(--dh-signal-accent-soft)');
const BLUE_RGBA = [[0, 122, 255], [10, 124, 255], [59, 130, 246], [37, 99, 235]];
addRgbas('blue.focus-ring', BLUE_RGBA, 'box-shadow', 'var(--focus-ring-blue)');
addRgbas('blue.background-subtle', BLUE_RGBA, 'background', 'var(--system-blue-subtle)');
addRgbas('blue.border-focus', BLUE_RGBA, 'border*', 'var(--border-focus)');
addRgbas('blue.color', BLUE_RGBA, 'color', 'var(--system-blue)');

addLiterals('red.system-red', ['#ef4444', '#dc2626', '#d64545', '#d70015', '#ff4d4f', '#ff6348'], '*', 'var(--system-red)');
addLiterals('red.ws-danger', ['#c0392b', '#b42318', '#b02a1f', '#b3362d', '#7a271a'], 'color', 'var(--ws-danger)');
const RED_RGBA = [[220, 38, 38], [239, 68, 68], [255, 59, 48]];
addRgbas('red.background-subtle', RED_RGBA, 'background', 'var(--error-bg-subtle)');
addRgbas('red.border-subtle', RED_RGBA, 'border*', 'var(--error-border-subtle)');
addRgbas('red.color', RED_RGBA, 'color', 'var(--system-red)');

addLiterals('green.system-green', ['#22c55e', '#2f9e44', '#0a7b2d'], '*', 'var(--system-green)');
addLiterals('green.status-progress', ['#0f9d8a', '#0f766e'], '*', 'var(--status-progress)');
const GREEN_RGBA = [[34, 197, 94], [52, 199, 89]];
addRgbas('green.background-subtle', GREEN_RGBA, 'background', 'var(--success-bg-subtle)');
addRgbas('green.border-subtle', GREEN_RGBA, 'border*', 'var(--success-border-subtle)');
addRgbas('green.color', GREEN_RGBA, 'color', 'var(--system-green)');

addLiterals('orange.system-orange', ['#f59e0b', '#E08600', '#ffa502', '#ff8f1f'], '*', 'var(--system-orange)');
addLiterals('orange.ws-warn', ['#d97706', '#b76e00', '#b35d00', '#8a6200', '#915400', '#c2410c'], 'color', 'var(--ws-warn)');
const ORANGE_RGBA = [[245, 158, 11], [255, 149, 0]];
addRgbas('orange.background-soft', ORANGE_RGBA, 'background', 'var(--dh-signal-warn-soft)');
addRgbas('orange.color', ORANGE_RGBA, 'color', 'var(--system-orange)');

addLiterals('purple.secondary', ['#667eea', '#764ba2', '#4F46E5', '#6366F1'], '*', 'var(--secondary-color)');
addLiterals('border.light', ['#e5e5e5', '#d2d2d7', '#efefef', '#e5e5ea'], 'border*', 'var(--border-light)');

const EXCLUDE_PARTS = [
  'node_modules', 'dist', 'src/legacy',
  'theme-tokens.css', 'variables.css', 'apple-theme.css',
  'matte-black-overrides.css',
  'src/pages/system_status/SystemStatus.vue',
  'src/styles/command_center_dashboard.css',
  'src/styles/kpi_dashboard_enhanced.css',
];

const BATCHES = {
  B1: [
    'src/styles/components.css', 'src/styles/tables.css',
    'src/styles/command_center_dashboard.css', 'src/styles/kpi_dashboard_enhanced.css',
    'src/styles/dashboard_frontline_workbench.css', 'src/styles/dashboard_handover.css',
    'src/styles/dispatch-board.css', 'src/pages/dashboard/', 'src/pages/flight_monitor/',
    'src/pages/command_center/', 'src/pages/dispatch_board/', 'src/pages/kpi_dashboard/',
    'src/components/flight-monitor/', 'src/components/dispatch-board/', 'src/components/dispatch/',
  ],
  B2: [
    'src/styles/flight-imports.css', 'src/styles/dispatch-rule-center.css',
    'src/styles/workspace_unified_theme.css', 'src/pages/flight_imports/',
    'src/pages/resource_manager/', 'src/pages/resource_utilization/',
    'src/pages/anomaly_monitor/', 'src/pages/system_status/', 'src/pages/system_flags/',
    'src/pages/dispatch_rule_center/',
  ],
  B3: [
    'src/styles/flowable-modeler.css', 'src/styles/admin-layout.css',
    'src/styles/admin-page.css', 'src/styles/page-tabs.css', 'src/styles/layout.css',
    'src/styles/base.css', 'src/styles/legacy_sync.css', 'src/styles/main.css',
    'src/components/', 'src/pages/ai_config_center/', 'src/pages/ai_monitor/',
    'src/pages/llm_eval_lab/', 'src/pages/nl_query/', 'src/pages/label_manager/',
    'src/pages/user_manager/', 'src/pages/login/', 'src/pages/operations_review_report/',
    'src/pages/flowable_modeler/',
  ],
};

const LOCKED_SEL_RE = /\.(nl-query-page|data-hub-page)/;
const COLOR_RE = /#[0-9a-fA-F]{3,8}\b|rgba?\([^)]*\)/g;
const EXPLICIT_KEEP_RE = /rgba\(\s*(?:118\s*,\s*118\s*,\s*128|142\s*,\s*142\s*,\s*147|60\s*,\s*60\s*,\s*67|255\s*,\s*255\s*,\s*0\s*,\s*0\.4|255\s*,\s*255\s*,\s*255\s*,\s*(?:0(?:\.\d+)?))\s*\)|#000(?:000)?\b/i;

function collectFiles(entry, out = []) {
  const normalized = entry.replaceAll('\\', '/');
  if (EXCLUDE_PARTS.some((part) => normalized.includes(part))) return out;
  const abs = join(ROOT, normalized);
  const stat = statSync(abs, { throwIfNoEntry: false });
  if (!stat) return out;
  if (stat.isDirectory()) {
    for (const name of readdirSync(abs)) collectFiles(`${normalized}/${name}`, out);
  } else if (/\.(css|vue)$/.test(normalized)) {
    out.push(abs);
  }
  return out;
}

function propMatches(ruleProp, prop) {
  if (ruleProp === '*') return true;
  if (ruleProp === 'border*') return prop.startsWith('border') || prop === 'outline-color';
  if (ruleProp === 'background') return prop === 'background' || prop === 'background-color';
  return ruleProp === prop;
}

function findVarRanges(value) {
  const ranges = [];
  for (let index = 0; index < value.length; index += 1) {
    if (!/^var\s*\(/i.test(value.slice(index))) continue;
    const open = value.indexOf('(', index);
    let depth = 1;
    let cursor = open + 1;
    for (; cursor < value.length && depth > 0; cursor += 1) {
      if (value[cursor] === '(') depth += 1;
      else if (value[cursor] === ')') depth -= 1;
    }
    ranges.push([index, depth === 0 ? cursor : value.length]);
    index = Math.max(index, cursor - 1);
  }
  return ranges;
}

function isInsideRanges(index, ranges) {
  return ranges.some(([start, end]) => index >= start && index < end);
}

function hasRuleCandidate(value, prop) {
  const ranges = findVarRanges(value);
  return RULES.some((rule) => {
    if (!propMatches(rule.prop, prop)) return false;
    rule.findRe.lastIndex = 0;
    return [...value.matchAll(rule.findRe)].some((match) => !isInsideRanges(match.index, ranges));
  });
}

function hasRawColorOutsideVar(value) {
  const ranges = findVarRanges(value);
  COLOR_RE.lastIndex = 0;
  return [...value.matchAll(COLOR_RE)].some((match) => !isInsideRanges(match.index, ranges));
}

function findBadgePairProtection(text, file, startLine, stats) {
  const protectedLines = new Set();
  const blockRe = /([^{}]+)\{([^{}]*)\}/gs;
  let match;
  while ((match = blockRe.exec(text))) {
    const selector = match[1].trim();
    if (!/(badge|chip|pill|tag)/i.test(selector)) continue;
    const declarations = new Map();
    for (const declaration of match[2].split(';')) {
      const parsed = declaration.match(/^\s*([-\w]+)\s*:\s*(.*?)\s*$/s);
      if (parsed) declarations.set(parsed[1].toLowerCase(), parsed[2]);
    }
    const background = declarations.get('background') ?? declarations.get('background-color');
    const color = declarations.get('color');
    if (!background || !color) continue;
    const backgroundCandidate = hasRuleCandidate(background, 'background');
    const colorCandidate = hasRuleCandidate(color, 'color');
    const splitPair = (backgroundCandidate && !colorCandidate && hasRawColorOutsideVar(color))
      || (colorCandidate && !backgroundCandidate && hasRawColorOutsideVar(background));
    if (!splitPair) continue;
    const blockStart = startLine + text.slice(0, match.index).split('\n').length - 1;
    const blockEnd = blockStart + match[0].split('\n').length - 1;
    for (let line = blockStart; line <= blockEnd; line += 1) protectedLines.add(line);
    stats.badgePairSkips.push({ file, line: blockStart, selector: selector.replace(/\s+/g, ' ') });
  }
  return protectedLines;
}

function gradientCollapse(value, prop) {
  if (!/linear-gradient\s*\(/i.test(value)) return null;
  const targets = new Map();
  for (const rule of RULES) {
    if (!propMatches(rule.prop, prop)) continue;
    rule.findRe.lastIndex = 0;
    const matches = [...value.matchAll(rule.findRe)];
    const literals = targets.get(rule.replace) ?? [];
    for (const match of matches) literals.push(match[0]);
    targets.set(rule.replace, literals);
  }
  for (const [target, literals] of targets) {
    if (literals.length === 0) continue;
    const existingTargetCount = value.split(target).length - 1;
    if (existingTargetCount + literals.length >= 2) {
      return {
        target,
        literals: [...Array(existingTargetCount).fill(target), ...literals],
      };
    }
  }
  return null;
}

function replaceValue(value, prop, context, stats) {
  const collapse = gradientCollapse(value, prop);
  if (collapse) {
    stats.gradientSkips.push({ ...context, prop, value, ...collapse });
    return value;
  }

  let current = value;
  for (const rule of RULES) {
    if (!propMatches(rule.prop, prop)) continue;
    const ranges = findVarRanges(current);
    rule.findRe.lastIndex = 0;
    current = current.replace(rule.findRe, (match, offset) => {
      if (isInsideRanges(offset, ranges)) {
        stats.varSkips.push({ ...context, prop, literal: match });
        return match;
      }
      stats.total += 1;
      stats.byFile.set(context.file, (stats.byFile.get(context.file) ?? 0) + 1);
      stats.byRule.set(rule.id, (stats.byRule.get(rule.id) ?? 0) + 1);
      return rule.replace;
    });
  }
  return current;
}

function processDeclarationPiece(piece, context, stats) {
  const parts = piece.split(';');
  return parts.map((fragment) => {
    const match = fragment.match(/^(\s*)([-\w]+)(\s*:\s*)(.*?)(\s*)$/s);
    if (!match || match[4] === '') return fragment;
    const [, before, propRaw, colon, value, after] = match;
    const prop = propRaw.toLowerCase();
    return `${before}${propRaw}${colon}${replaceValue(value, prop, context, stats)}${after}`;
  }).join(';');
}

function processCssText(text, file, stats, startLine = 1) {
  const lines = text.split('\n');
  const badgePairProtectedLines = findBadgePairProtection(text, file, startLine, stats);
  const selectorStack = [];
  const atRuleStack = [];
  let pendingSelector = '';
  let lockedLines = 0;
  const output = lines.map((line, index) => {
    const lineNo = startLine + index;
    const trimmed = line.trim();
    if (trimmed.startsWith('//') || trimmed.startsWith('/*') || trimmed.startsWith('*') || trimmed.startsWith('*/')) return line;
    const segments = line.split(/([{}])/);
    let lineOutput = '';
    for (const segment of segments) {
      if (segment === '{') {
        const selector = pendingSelector.trim();
        selectorStack.push(selector);
        atRuleStack.push(/^@(keyframes|-\w+-keyframes|media)\b/i.test(selector) ? selector : '');
        pendingSelector = '';
        lineOutput += segment;
        continue;
      }
      if (segment === '}') {
        selectorStack.pop();
        atRuleStack.pop();
        pendingSelector = '';
        lineOutput += segment;
        continue;
      }
      if (segment.trim()) pendingSelector += ` ${segment.trim()}`;
      if (selectorStack.length === 0 || !segment.includes(':')) {
        lineOutput += segment;
        continue;
      }
      const locked = selectorStack.some((selector) => LOCKED_SEL_RE.test(selector));
      const keyframes = atRuleStack.some((atRule) => /^@(keyframes|-\w+-keyframes)\b/i.test(atRule));
      const contrast = selectorStack.some((selector) => /@media[^\{]*prefers-contrast/i.test(selector));
      const svgData = /data:image\/svg\+xml/i.test(segment);
      if (locked) lockedLines += 1;
      const badgePair = badgePairProtectedLines.has(lineNo);
      if (locked || keyframes || contrast || svgData || badgePair) {
        lineOutput += segment;
      } else {
        lineOutput += processDeclarationPiece(segment, { file, line: lineNo }, stats);
      }
    }
    return lineOutput;
  });
  stats.lockedLines += lockedLines;
  return output.join('\n');
}

function processVueText(text, file, stats) {
  return text.replace(/(<style[^>]*>)([\s\S]*?)(<\/style>)/gi, (whole, open, body, close, offset) => {
    const startLine = text.slice(0, offset + open.length).split('\n').length;
    return open + processCssText(body, file, stats, startLine) + close;
  });
}

function declarationContext(line) {
  const match = line.match(/([-\w]+)\s*:\s*([^;}]*)/);
  return match ? { prop: match[1], value: match[2].trim() } : { prop: '(unknown)', value: line.trim() };
}

function scanLeftovers(text, file, leftovers) {
  text.split('\n').forEach((line, index) => {
    COLOR_RE.lastIndex = 0;
    let match;
    while ((match = COLOR_RE.exec(line))) {
      const context = declarationContext(line);
      const literal = match[0];
      const mappedCandidate = RULES.some((rule) => {
        rule.findRe.lastIndex = 0;
        return propMatches(rule.prop, context.prop.toLowerCase()) && rule.findRe.test(literal);
      });
      const recommendation = mappedCandidate && !EXPLICIT_KEEP_RE.test(literal)
        ? '建议人工映射'
        : '建议保留原样';
      leftovers.push({ file, line: index + 1, literal, prop: context.prop, recommendation });
    }
  });
}

const files = [...new Set(Object.values(BATCHES).flat().flatMap((entry) => collectFiles(entry)))]
  .sort((a, b) => a.localeCompare(b));
const stats = {
  total: 0,
  byFile: new Map(),
  byRule: new Map(),
  gradientSkips: [],
  varSkips: [],
  badgePairSkips: [],
  lockedLines: 0,
};

for (const abs of files) {
  const file = relative(ROOT, abs).replaceAll('\\', '/');
  const source = readFileSync(abs, 'utf8');
  const output = file.endsWith('.vue')
    ? processVueText(source, file, stats)
    : processCssText(source, file, stats);
  if (output !== source) writeFileSync(abs, output);
}

const leftovers = [];
for (const abs of files) {
  const file = relative(ROOT, abs).replaceAll('\\', '/');
  scanLeftovers(readFileSync(abs, 'utf8'), file, leftovers);
}

const report = [
  '# B 类残留颜色字面量 token 化报告', '',
  '## 执行摘要', '',
  `- 扫描文件数: ${files.length}`,
  `- 替换总数: ${stats.total}`,
  `- 发生替换的文件数: ${stats.byFile.size}`,
  `- 渐变坍缩跳过: ${stats.gradientSkips.length}`,
  `- var() 内部跳过: ${stats.varSkips.length}`,
  `- 徽章自洽色对跳过: ${stats.badgePairSkips.length}`,
  `- 深色锁定块跳过声明片段: ${stats.lockedLines}`,
  '', '## 按文件分布', '',
];
if (stats.byFile.size === 0) report.push('- 无');
for (const [file, count] of [...stats.byFile].sort()) report.push(`- \`${file}\`: ${count}`);

report.push('', '## 映射表命中', '');
for (const id of [...new Set(RULES.map((rule) => rule.id))]) {
  const count = stats.byRule.get(id) ?? 0;
  const rate = stats.total === 0 ? '0.00' : ((count / stats.total) * 100).toFixed(2);
  report.push(`- \`${id}\`: ${count}（占全部替换 ${rate}%）`);
}

report.push('', '## 渐变坍缩跳过记录', '');
if (stats.gradientSkips.length === 0) report.push('- 无');
for (const item of stats.gradientSkips) {
  report.push(`- \`${item.file}:${item.line}\` \`${item.prop}\`: ${item.literals.map((literal) => `\`${literal}\``).join(' + ')} -> \`${item.target}\``);
}

report.push('', '## var() 内部跳过记录', '');
if (stats.varSkips.length === 0) report.push('- 无');
for (const item of stats.varSkips) report.push(`- \`${item.file}:${item.line}\` \`${item.prop}\`: \`${item.literal}\``);

report.push('', '## 徽章自洽色对跳过记录', '');
if (stats.badgePairSkips.length === 0) report.push('- 无');
for (const item of stats.badgePairSkips) report.push(`- \`${item.file}:${item.line}\`: \`${item.selector}\``);

for (const category of ['建议人工映射', '建议保留原样']) {
  report.push('', `## 未替换清单：${category}`, '');
  const items = leftovers.filter((item) => item.recommendation === category);
  if (items.length === 0) report.push('- 无');
  for (const item of items) {
    report.push(`- \`${item.file}:${item.line}\`: \`${item.literal}\`；属性 \`${item.prop}\``);
  }
}

report.push('', '## 深色锁定块确认', '',
  `- \`.nl-query-page\` / \`.data-hub-page\` 规则块内替换数: 0`,
  `- 跳过的锁定块声明片段: ${stats.lockedLines}`,
  '', '## 验证结果', '',
  '- 待执行：`npm run typecheck`',
  '- 待执行：`npx vitest run`',
  '- 待执行：`npm run build`',
  '- 待执行：`npm run parity:capture-vue`',
  '- parity 差异摘要：待验证后补充。',
  '- 最终 commit/status：待验证后补充。', '');

writeFileSync(REPORT_PATH, report.join('\n'));
console.log(`[b-class] 扫描 ${files.length} 个文件，替换 ${stats.total} 处，涉及 ${stats.byFile.size} 个文件。`);
console.log(`[b-class] 渐变跳过 ${stats.gradientSkips.length} 处，var() 内跳过 ${stats.varSkips.length} 处，徽章色对跳过 ${stats.badgePairSkips.length} 处，锁定块跳过 ${stats.lockedLines} 个声明片段。`);
console.log('[b-class] 报告已写入 docs/plans/b-class-tokenization-report.md');
