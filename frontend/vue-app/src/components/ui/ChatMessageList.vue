<script lang="ts">
/**
 * 一条话的最小形：组件只认这四样，各页可以往上加字段（type / data / …）。
 * 组件是泛型的，加出来的字段会原样出现在 #body 槽里，类型不丢。
 */
export interface ChatMessage {
  id?: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  time?: string;
  mentioned?: boolean;
}
</script>

<script setup lang="ts" generic="T extends ChatMessage">
import { nextTick, onMounted, ref, watch } from 'vue';

/**
 * 会话流（信号面 §3.3）：一列话。气泡的形只在这里定，各页不要再自己写一套。
 *
 * - 我说的靠右、行动衬；它说的靠左、工作面 + 一根线；系统旁白居中、淡墨无底。
 * - 富内容（Markdown、图、建议卡、按钮）用 #body 具名槽自己渲染，
 *   气泡的形、时刻、滚动到底仍由本组件持有。
 * - 槽给回 msg（本页自己的类型）与 index，取值不必再按下标回捞。
 * - 翻旧话：滚到顶发 reach-start，页面把更早的消息接在头上；
 *   组件认得出「接在头上」和「来了新话」，前者钉住视线不跳，后者落到底。
 *   要认得出，消息就必须带稳定的 id。
 */
const props = withDefaults(defineProps<{
  messages: T[];
  streaming?: boolean;
  emptyText?: string;
  /** 距顶多少像素算触顶（翻旧话）；给 0 关掉 */
  reachStartThreshold?: number;
}>(), {
  streaming: false,
  emptyText: undefined,
  reachStartThreshold: 80,
});

const emit = defineEmits<{
  (e: 'reach-start'): void;
}>();

const container = ref<HTMLElement | null>(null);

function keyOf(msg: T | undefined, fallback: number): string | number {
  return msg?.id ?? fallback;
}

/** 话总是看最后一句：新消息、流式增量、以及刚挂载（开面板）时都落到底。 */
async function scrollToEnd(): Promise<void> {
  await nextTick();
  if (container.value) {
    container.value.scrollTop = container.value.scrollHeight;
  }
}

/**
 * 默认 flush: 'pre' —— 回调跑在 DOM 打补丁之前，所以这里量到的还是旧高度，
 * 正好用来在「头上插了旧话」之后把视线钉回原处。
 */
watch(
  () => [
    props.messages.length,
    keyOf(props.messages[0], -1),
    keyOf(props.messages[props.messages.length - 1], -2),
    props.messages[props.messages.length - 1]?.content,
  ] as const,
  async ([len, firstKey, lastKey], prev) => {
    const el = container.value;
    const grewAtHead = Boolean(
      prev && el && len > prev[0] && firstKey !== prev[1] && lastKey === prev[2],
    );
    if (!grewAtHead) {
      void scrollToEnd();
      return;
    }
    const prevHeight = el!.scrollHeight;
    const prevTop = el!.scrollTop;
    await nextTick();
    if (container.value) {
      container.value.scrollTop = Math.max(0, container.value.scrollHeight - prevHeight + prevTop);
    }
  },
);

function onScroll(): void {
  const el = container.value;
  if (!el || props.reachStartThreshold <= 0) return;
  if (el.scrollTop <= props.reachStartThreshold) {
    emit('reach-start');
  }
}

onMounted(() => { void scrollToEnd(); });
</script>

<template>
  <div ref="container" class="chat-list" @scroll="onScroll">
    <div v-if="!messages.length" class="chat-empty">
      {{ emptyText ?? '暂无消息' }}
    </div>
    <div
      v-for="(msg, index) in messages"
      :key="msg.id ?? index"
      class="chat-msg"
      :class="`is-${msg.role}`"
    >
      <div class="chat-bubble" :data-mentioned="msg.mentioned ? 'true' : undefined">
        <slot name="body" :msg="msg" :index="index">
          <span class="chat-text">{{ msg.content }}</span>
        </slot>
        <span v-if="msg.time" class="chat-time">{{ msg.time }}</span>
      </div>
    </div>
    <div v-if="streaming" class="chat-msg is-assistant">
      <div class="chat-bubble chat-streaming">
        <span class="chat-cursor" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat-list {
  flex: 1;
  overflow-y: auto;
  padding: var(--s3) var(--s1);
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.chat-empty {
  margin: auto;
  color: var(--ink-muted);
  font-size: var(--fs-body);
}

.chat-msg {
  display: flex;
}

.chat-msg.is-user {
  justify-content: flex-end;
}

.chat-bubble {
  max-width: 82%;
  padding: var(--s2) var(--s3);
  border-radius: var(--r-panel);
  font-size: var(--fs-body);
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}

.chat-bubble[data-mentioned='true'] {
  box-shadow: inset 3px 0 0 var(--act);
}

.is-user .chat-bubble {
  background: var(--act-soft);
  color: var(--ink);
}

.is-assistant .chat-bubble {
  background: var(--face-work);
  border: 1px solid var(--line);
  color: var(--ink);
}

.is-system .chat-bubble {
  background: transparent;
  color: var(--ink-muted);
  font-size: var(--fs-label);
  padding: 2px 0;
}

.chat-time {
  display: block;
  margin-top: var(--s1);
  font-size: var(--fs-label);
  font-family: var(--mono);
  color: var(--ink-muted);
}

.chat-streaming {
  display: inline-flex;
}

.chat-cursor {
  width: 7px;
  height: 14px;
  background: var(--act);
}

/* 气泡里的 Markdown：段距收紧，代码用等宽 + 一层淡墨底。
   各页把渲染好的 HTML 塞进 #body 即可，不要再各自写一套。 */
.chat-bubble :deep(p) {
  margin: 0 0 var(--s2);
}

.chat-bubble :deep(p:last-child) {
  margin-bottom: 0;
}

.chat-bubble :deep(strong) {
  font-weight: var(--fw-semibold);
}

.chat-bubble :deep(code) {
  padding: 1px var(--s1);
  border-radius: var(--r-cell);
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  font-family: var(--mono);
  font-size: var(--fs-label);
}

.chat-bubble :deep(pre) {
  margin: var(--s2) 0 0;
  padding: var(--s2) var(--s3);
  overflow: auto;
  border-radius: var(--r-cell);
  background: color-mix(in srgb, var(--ink) 8%, transparent);
}

.chat-bubble :deep(pre code) {
  padding: 0;
  background: none;
}

.chat-bubble :deep(ul),
.chat-bubble :deep(ol) {
  margin: 0 0 var(--s2);
  padding-left: var(--s4);
}

.chat-bubble :deep(a) {
  color: var(--act);
}

/* 引述（流事件日志常用）：一根线在左，次墨，不另起底 */
.chat-bubble :deep(blockquote) {
  margin: 0;
  padding-left: var(--s2);
  border-left: 2px solid var(--line-strong);
  color: var(--ink-subtle);
}

.chat-bubble :deep(hr) {
  height: 1px;
  margin: var(--s2) 0;
  border: 0;
  background: var(--line);
}
</style>
