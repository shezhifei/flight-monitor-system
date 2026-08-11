import { ref, computed, onMounted, onUnmounted, watch, type Ref, type ComputedRef } from 'vue';

export interface VirtualScrollOptions {
  itemHeight: Ref<number> | number;
  buffer?: number;
}

export function useVirtualScroll<T>(
  items: Ref<readonly T[]> | ComputedRef<readonly T[]>,
  containerRef: Ref<HTMLElement | null>,
  options: VirtualScrollOptions
) {
  const itemHeightRef = computed(() => 
    typeof options.itemHeight === 'number' ? options.itemHeight : options.itemHeight.value
  );
  const buffer = options.buffer || 5;

  const scrollTop = ref(0);
  const containerHeight = ref(0);
  const rafId = ref<number | null>(null);

  const updateScrollTop = () => {
    if (containerRef.value) {
      scrollTop.value = containerRef.value.scrollTop;
    }
  };

  const onScroll = () => {
    if (rafId.value !== null) return;
    rafId.value = requestAnimationFrame(() => {
      updateScrollTop();
      rafId.value = null;
    });
  };

  const updateContainerHeight = () => {
    if (containerRef.value) {
      containerHeight.value = containerRef.value.clientHeight;
    }
  };

  let resizeObserver: ResizeObserver | null = null;

  const setupListeners = (el: HTMLElement) => {
    el.addEventListener('scroll', onScroll, { passive: true });
    updateContainerHeight();
    updateScrollTop();

    if (window.ResizeObserver) {
      resizeObserver = new ResizeObserver(() => {
        updateContainerHeight();
      });
      resizeObserver.observe(el);
    }
  };

  const cleanupListeners = (el: HTMLElement) => {
    el.removeEventListener('scroll', onScroll);
    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }
  };

  watch(containerRef, (newEl, oldEl) => {
    if (oldEl) cleanupListeners(oldEl);
    if (newEl) setupListeners(newEl);
  });

  onMounted(() => {
    if (containerRef.value) {
      setupListeners(containerRef.value);
    }
  });

  onUnmounted(() => {
    if (containerRef.value) {
      cleanupListeners(containerRef.value);
    }
    if (rafId.value !== null) {
      cancelAnimationFrame(rafId.value);
    }
  });

  const startIndex = computed(() => {
    return Math.max(0, Math.floor(scrollTop.value / itemHeightRef.value) - buffer);
  });

  const visibleCount = computed(() => {
    if (itemHeightRef.value <= 0) return 0;
    return Math.ceil(containerHeight.value / itemHeightRef.value) + 2 * buffer;
  });

  const endIndex = computed(() => {
    return Math.min(items.value.length, startIndex.value + visibleCount.value);
  });

  const visibleItems = computed(() => {
    return items.value.slice(startIndex.value, endIndex.value);
  });

  const totalHeight = computed(() => items.value.length * itemHeightRef.value);
  const topSpacerHeight = computed(() => startIndex.value * itemHeightRef.value);
  const bottomSpacerHeight = computed(() => 
    Math.max(0, totalHeight.value - topSpacerHeight.value - (visibleItems.value.length * itemHeightRef.value))
  );

  const scrollToItem = (index: number) => {
    if (containerRef.value) {
      containerRef.value.scrollTop = index * itemHeightRef.value;
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
