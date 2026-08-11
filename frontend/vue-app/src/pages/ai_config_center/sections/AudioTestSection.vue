<script setup lang="ts">
import type { AudioLogEntry } from '../composables/useAiConfigCenter';

defineProps<{
  audioStatus: 'idle' | 'connecting' | 'connected' | 'closed' | 'error';
  audioError: string;
  audioLogs: AudioLogEntry[];
  audioAsrText: string;
  audioSelectedFile: File | null;
}>();
const emit = defineEmits<{
  connect: [];
  disconnect: [];
  handleFile: [event: Event];
  sendSelectedChunk: [];
  sendEnd: [];
}>();
</script>

<template>
  <section
    class="audio-panel"
    aria-label="实时音频测试"
  >
    <div class="audio-panel-header">
      <h4>实时音频测试</h4>
      <span class="audio-status-badge" :data-status="audioStatus">{{ audioStatus }}</span>
    </div>
    <div class="audio-panel-actions">
      <input
        class="audio-file-input"
        type="file"
        accept="audio/*"
        :disabled="audioStatus === 'connecting'"
        @change="emit('handleFile', $event)"
      >
      <button
        type="button"
        class="btn btn-sm btn-secondary"
        :disabled="audioStatus === 'connecting' || audioStatus === 'connected'"
        @click="emit('connect')"
      >
        连接
      </button>
      <button
        type="button"
        class="btn btn-sm btn-secondary"
        :disabled="audioStatus !== 'connected' && audioStatus !== 'connecting'"
        @click="emit('disconnect')"
      >
        断开
      </button>
      <button
        type="button"
        class="btn btn-sm btn-secondary"
        :disabled="audioStatus !== 'connected' || !audioSelectedFile"
        @click="emit('sendSelectedChunk')"
      >
        发送音频
      </button>
      <button
        type="button"
        class="btn btn-sm btn-secondary"
        :disabled="audioStatus !== 'connected'"
        @click="emit('sendEnd')"
      >
        结束音频
      </button>
    </div>
    <div v-if="audioError" class="audio-panel-error">
      {{ audioError }}
    </div>
    <div v-if="audioAsrText" class="audio-panel-asr">
      <span class="audio-panel-asr-label">ASR:</span>
      <span>{{ audioAsrText }}</span>
    </div>
    <ul class="audio-panel-log">
      <li
        v-for="entry in audioLogs"
        :key="entry.id"
        :class="['audio-log-item', `audio-log-item--${entry.direction}`]"
      >
        <span class="audio-log-type">
          {{ entry.direction === 'in' ? '←' : '→' }} {{ entry.type }}
        </span>
        <span class="audio-log-detail">{{ entry.detail }}</span>
      </li>
      <li v-if="audioLogs.length === 0" class="audio-log-empty">
        尚无音频会话事件
      </li>
    </ul>
  </section>
</template>
