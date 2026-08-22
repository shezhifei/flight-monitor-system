<script setup lang="ts">
import UiButton from '../../../components/ui/UiButton.vue';
import UiPill from '../../../components/ui/UiPill.vue';
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

type AudioStatus = 'idle' | 'connecting' | 'connected' | 'closed' | 'error';
type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';

/* 会话状态 → 四声：已连=ok，连接中=act，异常=danger，其余=mute */
function statusTone(status: AudioStatus): PillTone {
  switch (status) {
    case 'connected': return 'ok';
    case 'connecting': return 'act';
    case 'error': return 'danger';
    default: return 'mute';
  }
}
</script>

<template>
  <section
    class="audio-panel"
    aria-label="实时音频测试"
  >
    <div class="audio-panel-header">
      <h4>实时音频测试</h4>
      <UiPill :tone="statusTone(audioStatus)">
        {{ audioStatus }}
      </UiPill>
    </div>
    <div class="audio-panel-actions">
      <input
        class="audio-file-input"
        type="file"
        accept="audio/*"
        :disabled="audioStatus === 'connecting'"
        @change="emit('handleFile', $event)"
      >
      <UiButton
        variant="ghost"
        :disabled="audioStatus === 'connecting' || audioStatus === 'connected'"
        @click="emit('connect')"
      >
        连接
      </UiButton>
      <UiButton
        variant="ghost"
        :disabled="audioStatus !== 'connected' && audioStatus !== 'connecting'"
        @click="emit('disconnect')"
      >
        断开
      </UiButton>
      <UiButton
        variant="ghost"
        :disabled="audioStatus !== 'connected' || !audioSelectedFile"
        @click="emit('sendSelectedChunk')"
      >
        发送音频
      </UiButton>
      <UiButton
        variant="ghost"
        :disabled="audioStatus !== 'connected'"
        @click="emit('sendEnd')"
      >
        结束音频
      </UiButton>
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
