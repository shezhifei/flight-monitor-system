<script setup lang="ts">
import UiButton from './UiButton.vue';

const props = defineProps<{
  icon?: 'search' | 'plane' | 'alert' | 'filter' | 'data';
  title: string;
  description?: string;
  actionLabel?: string;
  actionDisabled?: boolean;
}>();

const emit = defineEmits<{
  (e: 'action'): void;
}>();

const iconPaths: Record<string, string> = {
  search: 'M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16zm10 2-4.35-4.35',
  plane: 'M2 14.5 22 4l-6.8 16-4.1-5.1L7 18l1.1-4.1L2 14.5Z',
  alert: 'M12 2L1 21h22M12 6l7.53 13H4.47M11 10v4h2v-4m-2 6v2h2v-4',
  filter: 'M2 3C2 2.72386 2.22386 2.5 2.5 2.5H11.5C11.7761 2.5 12 2.72386 12 3C12 3.12224 11.9553 3.24027 11.8745 3.33195L8.5 7.16667V10.7C8.5 10.8894 8.39299 11.0626 8.22361 11.1472L6.22361 12.1472C5.89112 12.3134 5.5 12.0716 5.5 11.7V7.16667L2.12553 3.33195C2.04473 3.24027 2 3.12224 2 3Z',
  data: 'M12 2v20M2 12h20M4.93 4.93l14.14 14.14M4.93 19.07L19.07 4.93',
};

const resolvedIcon = iconPaths[props.icon ?? 'search'];
</script>

<template>
  <div class="empty-state" role="status" aria-live="polite">
    <div class="empty-state-icon" aria-hidden="true">
      <svg
        width="40"
        height="40"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path :d="resolvedIcon" />
      </svg>
    </div>
    <div class="empty-state-title">
      {{ title }}
    </div>
    <div v-if="description" class="empty-state-desc">
      {{ description }}
    </div>
    <UiButton
      v-if="actionLabel"
      variant="tonal"
      class="empty-state-action"
      :disabled="actionDisabled"
      @click="emit('action')"
    >
      {{ actionLabel }}
    </UiButton>
  </div>
</template>

<style scoped>
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px 24px;
  text-align: center;
  color: var(--ink-subtle);
  contain: layout style paint;
}

.empty-state-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 64px;
  height: 64px;
  border-radius: var(--r-panel);
  background: color-mix(in srgb, var(--ink) 4%, transparent);
  color: var(--ink-muted);
  margin-bottom: 16px;
}

.empty-state-title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  margin-bottom: var(--s2);
  line-height: 1.4;
}

.empty-state-desc {
  font-size: var(--fs-body);
  color: var(--ink-muted);
  line-height: 1.5;
  max-width: 280px;
  margin-bottom: 16px;
}
</style>