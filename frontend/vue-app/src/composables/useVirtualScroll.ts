import { computed, onUnmounted, ref, watch, type ComputedRef, type Ref } from 'vue';

export interface VirtualScrollOptions {
  itemHeight: Ref<number> | number;
  buffer?: number;
}

export function useVirtualScroll<T>(
  items: Ref<readonly T[]> | ComputedRef<readonly T[]>,
  containerRef: Ref<HTMLElement | null>,
  options: VirtualScrollOptions,
) {
  const itemHeightRef = computed(() =>
    typeof options.itemHeight === 'number' ? options.itemHeight : options.itemHeight.value,
  );
  const buffer = options.buffer ?? 5;

  const scrollTop = ref(0);
  const containerHeight = ref(0);

  let scrollRaf: number | null = null;
  let resizeRaf: number | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let listeningEl: HTMLElement | null = null;

  const updateScrollTop = () => {
    const el = containerRef.value;
    if (!el) return;
    const next = el.scrollTop;
    if (next !== scrollTop.value) {
      scrollTop.value = next;
    }
  };

  const onScroll = () => {
    if (scrollRaf !== null) return;
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = null;
      updateScrollTop();
    });
  };

  const updateContainerHeight = () => {
    const el = containerRef.value;
    if (!el) return;
    const next = el.clientHeight;
    if (next !== containerHeight.value) {
      containerHeight.value = next;
    }
  };

  const onResize = () => {
    if (resizeRaf !== null) return;
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = null;
      updateContainerHeight();
    });
  };

  const cleanupListeners = (el: HTMLElement) => {
    el.removeEventListener('scroll', onScroll);
    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }
    if (listeningEl === el) {
      listeningEl = null;
    }
  };

  const setupListeners = (el: HTMLElement) => {
    if (listeningEl === el) return;
    if (listeningEl) cleanupListeners(listeningEl);
    listeningEl = el;
    el.addEventListener('scroll', onScroll, { passive: true });
    updateContainerHeight();
    updateScrollTop();
    if (typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(onResize);
      resizeObserver.observe(el);
    }
  };

  watch(
    containerRef,
    (newEl, oldEl) => {
      if (oldEl) cleanupListeners(oldEl);
      if (newEl) setupListeners(newEl);
    },
    { flush: 'post' },
  );

  onUnmounted(() => {
    if (listeningEl) cleanupListeners(listeningEl);
    if (scrollRaf !== null) cancelAnimationFrame(scrollRaf);
    if (resizeRaf !== null) cancelAnimationFrame(resizeRaf);
    scrollRaf = null;
    resizeRaf = null;
  });

  const startIndex = computed(() => {
    const height = itemHeightRef.value;
    if (height <= 0) return 0;
    return Math.max(0, Math.floor(scrollTop.value / height) - buffer);
  });

  const visibleCount = computed(() => {
    const height = itemHeightRef.value;
    if (height <= 0) return 0;
    return Math.ceil(containerHeight.value / height) + 2 * buffer;
  });

  const endIndex = computed(() => Math.min(items.value.length, startIndex.value + visibleCount.value));

  const visibleItems = computed(() => items.value.slice(startIndex.value, endIndex.value));

  const totalHeight = computed(() => items.value.length * Math.max(0, itemHeightRef.value));
  const topSpacerHeight = computed(() => startIndex.value * Math.max(0, itemHeightRef.value));
  const bottomSpacerHeight = computed(() =>
    Math.max(
      0,
      totalHeight.value - topSpacerHeight.value - visibleItems.value.length * Math.max(0, itemHeightRef.value),
    ),
  );

  const scrollToItem = (index: number) => {
    if (containerRef.value) {
      containerRef.value.scrollTop = index * Math.max(0, itemHeightRef.value);
    }
  };

  return {
    visibleItems,
    startIndex,
    endIndex,
    totalHeight,
    topSpacerHeight,
    bottomSpacerHeight,
    scrollToItem,
  };
}
