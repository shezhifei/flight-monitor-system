<script setup lang="ts">
import type { ConfigFieldItem } from './configFieldTypes';

const props = defineProps<{
  item: ConfigFieldItem;
}>();

const emit = defineEmits<{
  (e: 'change', id: string, value: unknown): void;
}>();

function onBool(event: Event): void {
  emit('change', props.item.id, (event.target as HTMLSelectElement).value === 'true');
}

function onNumber(event: Event): void {
  emit('change', props.item.id, Number((event.target as HTMLInputElement).value));
}

function onText(event: Event): void {
  emit('change', props.item.id, (event.target as HTMLInputElement).value);
}

function displayList(value: unknown): string {
  if (Array.isArray(value)) return value.length ? value.join(', ') : '（空列表）';
  return value == null ? '—' : String(value);
}
</script>

<template>
  <div class="cf-row">
    <div class="cf-meta">
      <div class="cf-title">{{ item.title }}</div>
      <div v-if="item.path" class="cf-path" :title="item.path">{{ item.path }}</div>
      <div v-if="item.description" class="cf-desc">{{ item.description }}</div>
    </div>

    <div class="cf-control">
      <template v-if="item.masked || item.type === 'password'">
        <input
          type="password"
          class="cf-input"
          value=""
          placeholder="••••••••"
          :disabled="item.disabled || item.masked"
          :aria-label="`配置 ${item.path || item.id}`"
          @change="onText"
        >
      </template>
      <template v-else-if="item.type === 'boolean'">
        <select
          class="cf-input cf-select"
          :disabled="item.disabled"
          :aria-label="`切换 ${item.path || item.id}`"
          :value="item.value === true ? 'true' : 'false'"
          @change="onBool"
        >
          <option value="true">已启用</option>
          <option value="false">已禁用</option>
        </select>
      </template>
      <template v-else-if="item.type === 'integer' || item.type === 'float'">
        <input
          type="number"
          class="cf-input"
          :step="item.type === 'float' ? 0.1 : 1"
          :disabled="item.disabled"
          :value="item.value as number | string"
          :aria-label="`配置 ${item.path || item.id}`"
          @change="onNumber"
        >
      </template>
      <template v-else-if="item.type === 'list'">
        <span class="cf-plain">{{ displayList(item.value) }}</span>
      </template>
      <template v-else-if="item.type === 'readonly' || item.disabled">
        <span class="cf-plain" :title="String(item.value ?? '')">{{ item.value ?? '—' }}</span>
      </template>
      <template v-else>
        <input
          type="text"
          class="cf-input"
          :disabled="item.disabled"
          :value="item.value as string | number | undefined"
          placeholder="请输入…"
          :aria-label="`配置 ${item.path || item.id}`"
          @change="onText"
        >
      </template>
    </div>
  </div>
</template>
