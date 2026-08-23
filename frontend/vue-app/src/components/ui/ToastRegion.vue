<script setup lang="ts">
import { useToast } from '@/composables/useToast';

/**
 * 全局 toast 渲染器：useToast 只维护状态，这里负责上屏。
 * 由 bootstrapProtectedPage 在每个受保护页面挂载到 body 末尾，
 * 样式复用 components.css 里已有的 .toast-region / .toast 一组。
 */
const { toasts, dismissToast, pauseToast, resumeToast } = useToast();
</script>

<template>
  <div class="toast-region" aria-live="polite">
    <div
      v-for="toast in toasts"
      :key="toast.id"
      class="toast"
      :class="[`toast--${toast.type}`, { 'toast--closing': toast.isClosing }]"
      :role="toast.role"
      @mouseenter="pauseToast(toast.id)"
      @mouseleave="resumeToast(toast.id)"
    >
      <div class="toast__content">
        <div class="toast__header">
          <p class="toast__title">{{ toast.title }}</p>
          <button
            type="button"
            class="toast__close"
            aria-label="关闭通知"
            @click="dismissToast(toast.id)"
          >
            ×
          </button>
        </div>
        <p class="toast__message">{{ toast.message }}</p>
      </div>
    </div>
  </div>
</template>
