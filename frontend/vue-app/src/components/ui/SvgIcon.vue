<script lang="ts">
/**
 * SvgIcon — 加载外部 SVG 文件并内联渲染，使图标颜色跟随 CSS `color`（currentColor），
 * 从而自动适配浅色/深色主题。`<img src="*.svg">` 无法被 CSS 重新着色，本组件用于替代它。
 *
 * - 单色图标：defs/mask 之外带有硬编码 fill/stroke 的元素会被改写为 currentColor
 * - 无 label 时视为装饰图标（aria-hidden）
 * - 加载失败时回退为 <img>
 *
 * 以下为模块级状态与纯函数（非 setup 作用域）：缓存与 id 计数器必须跨实例共享。
 */
import DOMPurify from 'dompurify';

const svgCache = new Map<string, Promise<string>>();
let svgInstanceCounter = 0;

/**
 * 内联 SVG 前的最后一道净化：移除 script、事件处理器（onload 等）、
 * javascript: URL 与 foreignObject 等危险结构，只保留 SVG 图形元素。
 * 仅靠正则剔除 <script> 无法防御事件属性类 XSS。
 * 注意：DOMPurify 3.2+ 出于安全默认剥离 <use> 元素，但图标雪碧依赖它，
 * 故放行 <use>，再经 restrictUseRefs 只保留站内片段引用（#id）。
 */
function sanitizeInlineSvg(svgText: string): string {
  return DOMPurify.sanitize(svgText, {
    USE_PROFILES: { svg: true, svgFilters: true },
    ADD_ATTR: ['href', 'xlink:href'],
    ADD_TAGS: ['use'],
  });
}

/**
 * <use> 只能引用站内片段（#id），禁止引用外部资源或空引用，
 * 防止经 SVG 外链注入任意图形/脚本。
 */
function restrictUseRefs(svgText: string): string {
  const doc = new DOMParser().parseFromString(svgText, 'image/svg+xml');
  for (const use of Array.from(doc.querySelectorAll('use'))) {
    const ref = use.getAttribute('href') ?? use.getAttribute('xlink:href');
    if (!ref?.startsWith('#')) use.remove();
  }
  return new XMLSerializer().serializeToString(doc);
}

/**
 * 内联多个 SVG 到同一文档时，id（path-1、mask-2 等）会互相冲突，
 * <use xlink:href="#path-1"> 可能解析到其它图标的图形。给每个加载的
 * SVG 的 id 加唯一前缀，并同步改写内部引用（href / xlink:href / url(#id)）。
 */
function namespaceIds(root: SVGSVGElement, prefix: string): void {
  const idMap = new Map<string, string>();
  for (const el of root.querySelectorAll('[id]')) {
    const oldId = el.getAttribute('id')!;
    const newId = `${prefix}-${oldId}`;
    idMap.set(oldId, newId);
    el.setAttribute('id', newId);
  }
  if (idMap.size === 0) return;
  for (const el of root.querySelectorAll('*')) {
    for (const attr of ['href', 'xlink:href']) {
      const v = el.getAttribute(attr);
      if (v && v.startsWith('#') && idMap.has(v.slice(1))) {
        el.setAttribute(attr, `#${idMap.get(v.slice(1))}`);
      }
    }
    for (const attr of ['fill', 'stroke', 'filter', 'mask', 'clip-path', 'marker-start', 'marker-mid', 'marker-end']) {
      const v = el.getAttribute(attr);
      if (!v || !v.includes('url(#')) continue;
      let updated = v;
      for (const [oldId, newId] of idMap) {
        updated = updated.split(`url(#${oldId})`).join(`url(#${newId})`);
      }
      if (updated !== v) el.setAttribute(attr, updated);
    }
  }
}

function recolorToCurrentColor(svgText: string): string {
  const doc = new DOMParser().parseFromString(svgText, 'image/svg+xml');
  const root = doc.querySelector('svg');
  if (!root) return svgText;
  root.setAttribute('width', '100%');
  root.setAttribute('height', '100%');
  const all = root.querySelectorAll('*');
  for (const el of all) {
    // defs/mask 内的元素（渐变、遮罩等）保持原色，只重着色实际绘制的图形
    if (el.closest('defs') || el.closest('mask')) continue;
    const fill = el.getAttribute('fill');
    if (fill && fill !== 'none') el.setAttribute('fill', 'currentColor');
    const stroke = el.getAttribute('stroke');
    if (stroke && stroke !== 'none') el.setAttribute('stroke', 'currentColor');
  }
  return new XMLSerializer().serializeToString(root);
}

/** 每次注入前调用：同一图标在页面出现多次时，id 也不会重复 */
function applyNamespace(svgText: string): string {
  const doc = new DOMParser().parseFromString(svgText, 'image/svg+xml');
  const root = doc.querySelector('svg');
  if (!root) return svgText;
  namespaceIds(root, `svgi-${++svgInstanceCounter}`);
  return new XMLSerializer().serializeToString(root);
}

function loadSvg(src: string): Promise<string> {
  let pending = svgCache.get(src);
  if (!pending) {
    pending = fetch(src)
      .then((res) => {
        if (!res.ok) throw new Error(`SVG load failed (${res.status}): ${src}`);
        return res.text();
      })
      .then((text) =>
        sanitizeInlineSvg(
          recolorToCurrentColor(
            text
              .replace(/<\?xml[^?]*\?>/g, '')
              .replace(/<!DOCTYPE[^>]*>/gi, ''),
          ),
        ),
      )
      .then(restrictUseRefs);
    svgCache.set(src, pending);
  }
  return pending;
}
</script>

<script setup lang="ts">
import { ref, watch } from 'vue';

defineOptions({ inheritAttrs: false });

const props = withDefaults(
  defineProps<{
    src: string;
    /** 图标尺寸（px 数值或任意 CSS 尺寸），默认 1em 跟随字号 */
    size?: number | string;
    /** 无障碍标签；不传则 aria-hidden */
    label?: string;
  }>(),
  { size: undefined, label: undefined },
);

const svgContent = ref('');
const loadFailed = ref(false);

watch(
  () => props.src,
  (src) => {
    if (!src) return;
    loadFailed.value = false;
    loadSvg(src)
      .then((svg) => {
        svgContent.value = applyNamespace(svg);
      })
      .catch(() => {
        loadFailed.value = true;
      });
  },
  { immediate: true },
);
</script>

<template>
  <img
    v-if="loadFailed"
    v-bind="$attrs"
    :src="src"
    class="svg-icon svg-icon--fallback"
    :alt="label ?? ''"
    :aria-hidden="label ? undefined : 'true'"
  >
  <span
    v-else
    v-bind="$attrs"
    class="svg-icon svg-icon--inline"
    :style="size !== undefined ? { width: typeof size === 'number' ? `${size}px` : size, height: typeof size === 'number' ? `${size}px` : size } : undefined"
    :role="label ? 'img' : undefined"
    :aria-label="label"
    :aria-hidden="label ? undefined : 'true'"
    v-html="svgContent"
  />
</template>

<style scoped>
.svg-icon--inline {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1em;
  height: 1em;
  vertical-align: -0.125em;
  flex-shrink: 0;
}

.svg-icon--inline :deep(svg) {
  width: 100%;
  height: 100%;
  display: block;
}
</style>
