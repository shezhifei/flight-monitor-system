// @vitest-environment jsdom
import { defineComponent, nextTick, ref } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import { useVirtualScroll } from './useVirtualScroll';

const items = Array.from({ length: 200 }, (_, index) => ({ id: index }));

const Harness = defineComponent({
  setup() {
    const containerRef = ref<HTMLElement | null>(null);
    const list = ref(items);
    const scroll = useVirtualScroll(list, containerRef, { itemHeight: 40, buffer: 2 });
    return { containerRef, ...scroll };
  },
  template: '<div ref="containerRef" class="scroller"></div>',
});

describe('useVirtualScroll', () => {
  let wrapper: VueWrapper<InstanceType<typeof Harness>>;

  beforeEach(() => {
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', {
      configurable: true,
      get() {
        return 200;
      },
    });
    Object.defineProperty(HTMLElement.prototype, 'scrollTop', {
      configurable: true,
      get() {
        return Number((this as HTMLElement).dataset.scrollTop ?? 0);
      },
      set(value: number) {
        (this as HTMLElement).dataset.scrollTop = String(value);
      },
    });
  });

  afterEach(() => {
    wrapper?.unmount();
    vi.unstubAllGlobals();
  });

  it('windows the first page plus buffer', async () => {
    wrapper = mount(Harness);
    await nextTick();

    expect(wrapper.vm.startIndex).toBe(0);
    expect(wrapper.vm.endIndex).toBe(9);
    expect(wrapper.vm.visibleItems).toHaveLength(9);
    expect(wrapper.vm.visibleItems[0]?.id).toBe(0);
    expect(wrapper.vm.topSpacerHeight).toBe(0);
    expect(wrapper.vm.bottomSpacerHeight).toBe(7640);
    expect(wrapper.vm.totalHeight).toBe(8000);
  });

  it('advances the window from scrollTop without keeping off-screen rows', async () => {
    wrapper = mount(Harness);
    await nextTick();

    const el = wrapper.get('.scroller').element as HTMLElement;
    el.scrollTop = 400;
    el.dispatchEvent(new Event('scroll'));
    await nextTick();

    expect(wrapper.vm.startIndex).toBe(8);
    expect(wrapper.vm.visibleItems[0]?.id).toBe(8);
    expect(wrapper.vm.visibleItems).toHaveLength(9);
    expect(wrapper.vm.topSpacerHeight).toBe(320);
    expect(wrapper.vm.topSpacerHeight + wrapper.vm.visibleItems.length * 40 + wrapper.vm.bottomSpacerHeight)
      .toBe(wrapper.vm.totalHeight);
  });

});
