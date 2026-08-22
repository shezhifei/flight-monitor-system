<script setup lang="ts">
import { computed } from 'vue';

/**
 * 头像（信号面 §5.2）：没有照片时的人形占位 —— 一个字 + 一圈线。
 *
 * 不用渐变、不用彩色实底：那是「品牌感」，不是事态。默认是页底 + 一根线 + 次墨，
 * 只有当这个人此刻的状态真的要说话（在线、已拒、超时）时才给 tone，
 * 且仍只动底与字（`*-soft` + 本声），不动大小、不加光。
 */
const props = withDefaults(defineProps<{
  /** 显示用的字；给整个名字也行，只取第一个字符 */
  text?: string;
  size?: 'sm' | 'md';
  tone?: 'mute' | 'act' | 'ok' | 'warn' | 'danger';
  /** 读屏用的全名；不给就退到 text */
  label?: string;
}>(), {
  text: '',
  size: 'md',
  tone: 'mute',
  label: undefined,
});

const initial = computed(() => {
  const raw = String(props.text ?? '').trim();
  return raw ? Array.from(raw)[0].toUpperCase() : '·';
});
</script>

<template>
  <span
    class="ui-avatar"
    :data-size="size"
    :data-tone="tone"
    :title="label ?? (text || undefined)"
    :aria-label="label ?? (text || undefined)"
  >{{ initial }}</span>
</template>

<style scoped>
.ui-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  border: 1px solid var(--line);
  border-radius: var(--r-pill);
  background: var(--face-page);
  color: var(--ink-subtle);
  font-weight: var(--fw-semibold);
  line-height: 1;
  user-select: none;
}

.ui-avatar[data-size='md'] {
  width: 30px;
  height: 30px;
  font-size: var(--fs-body);
}

.ui-avatar[data-size='sm'] {
  width: 24px;
  height: 24px;
  font-size: 11px;
}

.ui-avatar[data-tone='act'] {
  border-color: var(--act);
  background: var(--act-soft);
  color: var(--act);
}

.ui-avatar[data-tone='ok'] {
  border-color: var(--ok);
  background: var(--ok-soft);
  color: var(--ok);
}

.ui-avatar[data-tone='warn'] {
  border-color: var(--warn);
  background: var(--warn-soft);
  color: var(--warn);
}

.ui-avatar[data-tone='danger'] {
  border-color: var(--danger);
  background: var(--danger-soft);
  color: var(--danger);
}
</style>
