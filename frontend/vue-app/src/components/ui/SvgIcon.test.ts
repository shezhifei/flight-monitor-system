import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import SvgIcon from './SvgIcon.vue';

const BLACK_HOME_SVG = `<?xml version="1.0" encoding="UTF-8"?>
<svg width="24px" height="24px" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
  <defs><path id="path-1" d="M1 1h10v10H1z"/></defs>
  <g stroke="none" fill="none">
    <mask id="mask-2" fill="white"><use xlink:href="#path-1"/></mask>
    <use id="p" fill="#000000" xlink:href="#path-1"/>
  </g>
</svg>`;

function mockFetchOnce(body: string, ok = true) {
  const fetchMock = vi.fn().mockResolvedValue({
    ok,
    status: ok ? 200 : 404,
    text: () => Promise.resolve(body),
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

describe('SvgIcon', () => {
  beforeEach(() => {
    mockFetchOnce(BLACK_HOME_SVG);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('内联渲染 SVG 并将硬编码 fill 改写为 currentColor', async () => {
    const wrapper = mount(SvgIcon, { props: { src: '/frontend/icons/home.svg' } });
    await flushPromises();

    const span = wrapper.find('span.svg-icon--inline');
    expect(span.exists()).toBe(true);
    const use = wrapper.find('g use[fill]');
    expect(use.exists()).toBe(true);
    expect(use.attributes('fill')).toBe('currentColor');
  });

  it('保留 defs/mask 内元素的原始颜色', async () => {
    const wrapper = mount(SvgIcon, { props: { src: '/frontend/icons/home.svg' } });
    await flushPromises();

    const mask = wrapper.find('mask');
    expect(mask.exists()).toBe(true);
    expect(mask.attributes('fill')).toBe('white');
  });

  it('无 label 时 aria-hidden，传入 label 时 role=img', async () => {
    const deco = mount(SvgIcon, { props: { src: '/frontend/icons/home.svg' } });
    await flushPromises();
    expect(deco.find('span').attributes('aria-hidden')).toBe('true');

    const labeled = mount(SvgIcon, { props: { src: '/frontend/icons/home.svg', label: '返回工作台' } });
    await flushPromises();
    expect(labeled.find('span').attributes('role')).toBe('img');
    expect(labeled.find('span').attributes('aria-label')).toBe('返回工作台');
  });

  it('支持数值尺寸', async () => {
    const wrapper = mount(SvgIcon, { props: { src: '/frontend/icons/home.svg', size: 18 } });
    await flushPromises();
    expect(wrapper.find('span').attributes('style')).toContain('width: 18px');
    expect(wrapper.find('span').attributes('style')).toContain('height: 18px');
  });

  it('加载失败时回退为 img', async () => {
    mockFetchOnce('', false);
    const wrapper = mount(SvgIcon, { props: { src: '/frontend/icons/missing.svg' } });
    await flushPromises();
    const img = wrapper.find('img.svg-icon--fallback');
    expect(img.exists()).toBe(true);
    expect(img.attributes('src')).toBe('/frontend/icons/missing.svg');
  });

  it('多个图标内联时 id 加唯一前缀，use 引用不串图标', async () => {
    const eyeSvg = BLACK_HOME_SVG; // 含 id="path-1" 与 <use xlink:href="#path-1">
    const forbiddenSvg = BLACK_HOME_SVG.replace('M1 1h10v10H1z', 'M2 2h8v8H2z');
    mockFetchOnce(eyeSvg);
    const first = mount(SvgIcon, { props: { src: '/frontend/icons/a.svg' } });
    await flushPromises();
    mockFetchOnce(forbiddenSvg);
    const second = mount(SvgIcon, { props: { src: '/frontend/icons/b.svg' } });
    await flushPromises();

    const firstUse = first.find('g use');
    const secondUse = second.find('g use');
    const firstRef = firstUse.attributes('xlink:href') ?? firstUse.attributes('href');
    const secondRef = secondUse.attributes('xlink:href') ?? secondUse.attributes('href');
    expect(firstRef).toBeTruthy();
    expect(secondRef).toBeTruthy();
    // 引用都被重写为带前缀的 id，且两个图标互不相同
    expect(firstRef).not.toBe('#path-1');
    expect(secondRef).not.toBe('#path-1');
    expect(firstRef).not.toBe(secondRef);
    // 引用目标仍指向自己内部的 path
    expect(first.find(`path[id="${firstRef!.slice(1)}"]`).exists()).toBe(true);
    expect(second.find(`path[id="${secondRef!.slice(1)}"]`).exists()).toBe(true);
  });
});
