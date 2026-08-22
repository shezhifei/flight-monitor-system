<script setup lang="ts">
import { ref, watch, nextTick, onUnmounted } from 'vue';
import { useAiStream } from '../../composables/useAiStream';
import { fetchFlightEventJourney } from '../../composables/useFlightData';
import { useAuth } from '../../composables/useAuth';
import UiModal from '../ui/UiModal.vue';
import UiPill from '../ui/UiPill.vue';
import UiBanner from '../ui/UiBanner.vue';
import UiSkeleton from '../ui/UiSkeleton.vue';

const props = defineProps<{
  isOpen: boolean;
  flightId?: string | null;
  flightNo?: string | null;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const { messages, startStream, stopStream, clearMessages } = useAiStream();
const auth = useAuth();

interface InsightLog {
  id: number;
  type: 'info' | 'error' | 'success' | 'warning' | 'worker';
  content: string;
  timestamp: string;
}

interface TopologyNode {
  id: string;
  label: string;
  type: string;
  status: string;
  description: string;
}

/** 级联节点的事态映射回四声；标签给中文，不把后端枚举原样丢给值班的人。 */
function nodeTone(status: string): 'ok' | 'warn' | 'danger' | 'mute' {
  const s = status.trim().toLowerCase();
  if (s === 'critical' || s === 'blocked') return 'danger';
  if (s === 'affected' || s === 'delayed') return 'warn';
  if (s === 'ok' || s === 'normal' || s === 'resolved') return 'ok';
  return 'mute';
}

function nodeLabel(status: string): string {
  const s = status.trim().toLowerCase();
  if (s === 'critical') return '严重';
  if (s === 'blocked') return '受阻';
  if (s === 'affected') return '受影响';
  if (s === 'delayed') return '延误';
  if (s === 'ok' || s === 'normal') return '正常';
  if (s === 'resolved') return '已消解';
  return status;
}

const logs = ref<InsightLog[]>([]);
const topologyNodes = ref<TopologyNode[]>([]);
const isAnalyzing = ref(false);
const analysisError = ref('');
const terminalBody = ref<HTMLElement | null>(null);
let terminalTimeoutId: ReturnType<typeof setTimeout> | null = null;
const TERMINAL_TIMEOUT_MS = 30000;

/** 键必须稳定且唯一：同一毫秒内会连写两条日志，Date.now() 会撞键 */
let logSeq = 0;

function addLog(content: string, type: InsightLog['type'] = 'info') {
  logs.value.push({
    id: ++logSeq,
    type,
    content,
    timestamp: new Date().toLocaleTimeString()
  });
  scrollToBottom();
}

async function scrollToBottom() {
  await nextTick();
  if (terminalBody.value) {
    terminalBody.value.scrollTop = terminalBody.value.scrollHeight;
  }
}

function clearTerminalTimeout() {
  if (terminalTimeoutId !== null) {
    clearTimeout(terminalTimeoutId);
    terminalTimeoutId = null;
  }
}

function startTerminalTimeout() {
  clearTerminalTimeout();
  terminalTimeoutId = setTimeout(() => {
    if (isAnalyzing.value) {
      isAnalyzing.value = false;
      analysisError.value = '分析超时：未收到完成信号';
      addLog('[ERROR] 分析超时：后端未在规定时间内返回完成信号。', 'error');
    }
  }, TERMINAL_TIMEOUT_MS);
}

async function startInsight() {
  if (!props.flightId) return;

  isAnalyzing.value = true;
  analysisError.value = '';
  logs.value = [];
  topologyNodes.value = [];
  clearMessages();

  addLog(`[SYS] AI Vision Engine Initialized for ${props.flightNo}...`, 'success');
  addLog('[SYS] 正在挂载航班全域流转数据池...', 'info');

  startTerminalTimeout();

  try {
    const result = await fetchFlightEventJourney(props.flightId, {
      apiBase: auth.apiBase.value,
      authFetch: auth.fetch,
    });

    addLog('[WORKER] 后端推演引擎已就绪，正在接收流式洞察数据...', 'worker');

    if (result?.data) {
       addLog(`[SYS] 获取到事件流: ${result.data}`, 'info');
    }
  } catch (err: unknown) {
    addLog(`[ERROR] 洞察启动失败: ${(err as { message?: string }).message}`, 'error');
    analysisError.value = (err as { message?: string }).message || '洞察启动失败';
    isAnalyzing.value = false;
    clearTerminalTimeout();
  }
}

watch(() => props.isOpen, (val) => {
  if (val) {
    analysisError.value = '';
    startStream(['smart_monitor', 'ai_execution']);
    startInsight();
  } else {
    stopStream();
    clearTerminalTimeout();
    isAnalyzing.value = false;
    analysisError.value = '';
  }
});

watch(() => messages.value.length, (newLen) => {
  if (newLen > 0) {
    const latest = messages.value[newLen - 1];

    if (latest.type === 'text') {
      addLog(`[WORKER] ${latest.content}`, 'worker');
    } else if (latest.type === 'ai_suggestion') {
      const node = latest.data as Record<string, unknown> | undefined;
      if (node && !topologyNodes.value.some(n => n.id === String(node.id))) {
        topologyNodes.value.push({
          id: String(node.id || Date.now().toString()),
          label: String(node.label || latest.content || '发现新洞察'),
          type: String(node.type || 'impact'),
          status: String(node.status || 'affected'),
          description: String(node.description || '')
        });
        addLog(`[SYS] 发现异常节点: ${node.label || latest.content}`, 'warning');
      }
    } else if (latest.type === 'done' || latest.type === 'ai_complete') {
      addLog('[SYS] 航班全景洞察推演完成。', 'success');
      clearTerminalTimeout();
      isAnalyzing.value = false;
    }
  }
});

onUnmounted(() => {
  clearTerminalTimeout();
  stopStream();
});
</script>

<template>
  <UiModal
    :open="isOpen"
    title="航班洞察"
    :width="900"
    bleed
    @close="emit('close')"
  >
    <div class="insight">
      <!-- 帽下一条：这是哪架航班、推演到哪一步（状态只在这里报一次） -->
      <div class="insight__bar">
        <span class="insight__flight">{{ flightNo || '未选航班' }}</span>
        <UiPill :tone="isAnalyzing ? 'act' : 'ok'">
          {{ isAnalyzing ? '推演中' : '已完成' }}
        </UiPill>
      </div>

      <div class="insight__cols">
        <!-- 左：诊断输出，降一级到页底 + 等宽（§3.7 嵌板那一类内容） -->
        <div class="insight__log">
          <div class="insight__head">
            <span class="insight__name">诊断日志</span>
          </div>
          <div ref="terminalBody" class="insight__stream">
            <p
              v-for="log in logs"
              :key="log.id"
              class="line"
              :data-kind="log.type"
            >
              <span class="line__time">[{{ log.timestamp }}]</span> {{ log.content }}
            </p>
            <p v-if="logs.length === 0" class="void">
              等待后端连接
            </p>
          </div>
        </div>

        <!-- 右：级联影响，一行一个节点，声画在节点上 -->
        <div class="insight__impact">
          <div class="insight__head">
            <span class="insight__name">级联影响</span>
            <span v-if="topologyNodes.length" class="insight__count">{{ topologyNodes.length }}</span>
          </div>
          <div v-if="analysisError" class="insight__alert">
            <UiBanner tone="danger">
              <span>{{ analysisError }}</span>
            </UiBanner>
          </div>
          <div v-if="topologyNodes.length > 0" class="insight__nodes">
            <div v-for="node in topologyNodes" :key="node.id" class="node">
              <span class="node__id">
                <span class="node__label">{{ node.label }}</span>
                <span v-if="node.description" class="node__desc">{{ node.description }}</span>
              </span>
              <UiPill :tone="nodeTone(node.status)">
                {{ nodeLabel(node.status) }}
              </UiPill>
            </div>
          </div>
          <!-- 等的时候画同构的版（§3.9）：几行、名在左、胶囊在右 -->
          <div
            v-else-if="isAnalyzing && !analysisError"
            class="insight__nodes"
            aria-busy="true"
            aria-label="正在推演级联影响"
          >
            <div v-for="i in 3" :key="i" class="node">
              <span class="node__id">
                <UiSkeleton width="140px" height="14px" />
                <UiSkeleton width="200px" height="11px" />
              </span>
              <UiSkeleton shape="pill" width="52px" height="22px" />
            </div>
          </div>
          <p v-else-if="!analysisError" class="void">
            等待任务启动
          </p>
        </div>
      </div>
    </div>
  </UiModal>
</template>

<style scoped>
.insight {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 420px;
}

.insight__bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--s3);
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
}

.insight__flight {
  font-family: var(--mono);
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.insight__cols {
  display: flex;
  flex: 1;
  min-height: 0;
}

/* 诊断输出降一级：页底 + 一根线，不再自己描边、投影、换圆角 */
.insight__log {
  flex: 1.2;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: var(--face-page);
  border-right: 1px solid var(--line);
}

.insight__impact {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.insight__head {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--s3);
  padding: var(--s2) var(--s3);
  border-bottom: 1px solid var(--line);
}

.insight__name {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
}

.insight__count {
  margin-left: auto;
  font-size: var(--fs-label);
  font-variant-numeric: tabular-nums;
  color: var(--ink-subtle);
}

.insight__alert {
  flex: none;
  padding: var(--s3) var(--s3) 0;
}

.insight__stream {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--s3);
  font-family: var(--mono);
  font-size: var(--fs-body);
  line-height: 1.6;
}

/* 一行日志就是一个对象，事态染在这一行上 */
.line {
  margin: 0 0 var(--s2);
  color: var(--ink);
}

.line__time {
  margin-right: var(--s2);
  color: var(--ink-muted);
}

.line[data-kind='info'] { color: var(--ink-subtle); }
.line[data-kind='success'] { color: var(--ok); }
.line[data-kind='warning'] { color: var(--warn); }
.line[data-kind='error'] { color: var(--danger); }

.insight__nodes {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.node {
  display: flex;
  align-items: flex-start;
  gap: var(--s3);
  padding: var(--s2) var(--s3);
  border-bottom: 1px solid var(--line);
}

.node__id {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.node__label {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.node__desc {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  line-height: 1.4;
}

.void {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  margin: 0;
  padding: var(--s5) var(--s4);
  color: var(--ink-muted);
  font-size: var(--fs-body);
}
</style>
