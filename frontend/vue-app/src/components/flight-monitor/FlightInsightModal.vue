<script setup lang="ts">
import { ref, watch, nextTick, onUnmounted } from 'vue';
import { useAiStream } from '../../composables/useAiStream';
import { fetchFlightEventJourney } from '../../composables/useFlightData';
import { useAuth } from '../../composables/useAuth';
import UiModal from '../ui/UiModal.vue';

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

const logs = ref<InsightLog[]>([]);
const topologyNodes = ref<TopologyNode[]>([]);
const isAnalyzing = ref(false);
const analysisError = ref('');
const terminalBody = ref<HTMLElement | null>(null);
let terminalTimeoutId: ReturnType<typeof setTimeout> | null = null;
const TERMINAL_TIMEOUT_MS = 30000;

function addLog(content: string, type: InsightLog['type'] = 'info') {
  logs.value.push({
    id: Date.now(),
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
  <UiModal :open="isOpen" title="航班洞察" :width="900" @close="emit('close')">
    <div class="insight-meta">
      <span class="flight-no">{{ flightNo || '未选航班' }}</span>
      <span class="status-badge">{{ isAnalyzing ? '分析中' : '完成' }}</span>
    </div>
    <div class="insight-layout">
      <div class="insight-left">
        <div class="insight-terminal">
          <div class="insight-terminal-header">
            <span>诊断日志</span>
            <span :class="isAnalyzing ? 'tone-warn' : 'tone-ok'">
              {{ isAnalyzing ? '处理中' : '待命' }}
            </span>
          </div>
          <div ref="terminalBody" class="insight-terminal-body">
            <p v-for="log in logs" :key="log.id" :class="`log-${log.type}`">
              <span class="log-time">[{{ log.timestamp }}]</span> {{ log.content }}
            </p>
            <div v-if="logs.length === 0" class="terminal-placeholder">
              等待后端连接
            </div>
          </div>
        </div>
      </div>
      <div class="insight-right">
        <div class="panel-section-header">
          <h4>级联影响</h4>
          <span v-if="topologyNodes.length" class="node-count">{{ topologyNodes.length }}</span>
        </div>
        <div v-if="topologyNodes.length > 0" class="topology-container">
          <div
            v-for="node in topologyNodes"
            :key="node.id"
            class="insight-node-card"
            :data-status="node.status"
          >
            <div class="node-info">
              <div class="node-label">{{ node.label }}</div>
              <div class="node-desc">{{ node.description }}</div>
            </div>
            <div class="node-status-tag">{{ node.status }}</div>
          </div>
        </div>
        <div v-else-if="analysisError" class="insight-placeholder error-state">
          {{ analysisError }}
        </div>
        <div v-else class="insight-placeholder">
          {{ isAnalyzing ? '正在推演级联影响' : '等待任务启动' }}
        </div>
      </div>
    </div>
  </UiModal>
</template>

<style scoped>
.insight-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.flight-no {
  font-family: var(--mono);
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.status-badge {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  padding: 2px 8px;
  border-radius: var(--r-cell);
  background: var(--act-soft);
  color: var(--act);
}

.insight-layout {
  display: flex;
  min-height: 420px;
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  overflow: hidden;
}

.insight-left {
  flex: 1.2;
  border-right: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--face-page);
}

.insight-terminal {
  flex: 1;
  display: flex;
  flex-direction: column;
  font-family: var(--mono);
  min-height: 0;
}

.insight-terminal-header {
  padding: 8px 12px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid var(--line);
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.tone-warn { color: var(--warn); }
.tone-ok { color: var(--ok); }

.insight-terminal-body {
  padding: 12px;
  font-size: var(--fs-body);
  color: var(--ink);
  line-height: 1.6;
  overflow-y: auto;
  flex: 1;
}

.insight-terminal-body p {
  margin: 0 0 6px;
}

.log-time {
  color: var(--ink-muted);
  margin-right: 8px;
}
.log-success { color: var(--ok); }
.log-error { color: var(--danger); }
.log-warning { color: var(--warn); }
.log-worker { color: var(--act); }

.terminal-placeholder {
  color: var(--ink-muted);
  text-align: center;
  margin-top: 40px;
}

.insight-right {
  flex: 1;
  padding: 16px;
  display: flex;
  flex-direction: column;
  background: var(--face-work);
  min-width: 0;
}

.panel-section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.insight-right h4 {
  margin: 0;
  color: var(--ink);
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
}

.node-count {
  font-size: var(--fs-label);
  font-variant-numeric: tabular-nums;
  color: var(--ink-subtle);
}

.topology-container {
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow-y: auto;
  flex: 1;
}

.insight-node-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: var(--face-raised);
}

.insight-node-card[data-status="affected"] {
  border-left: 3px solid var(--warn);
}
.insight-node-card[data-status="critical"] {
  border-left: 3px solid var(--danger);
}

.node-info { flex: 1; }

.node-label {
  font-weight: var(--fw-semibold);
  font-size: var(--fs-section);
  color: var(--ink);
  margin-bottom: 4px;
}

.node-desc {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  line-height: 1.4;
}

.node-status-tag {
  font-size: 10px;
  font-weight: var(--fw-semibold);
  padding: 2px 6px;
  border-radius: var(--r-cell);
  background: var(--line);
  color: var(--ink-subtle);
  text-transform: uppercase;
}

.insight-placeholder {
  flex: 1;
  border: 1px dashed var(--line);
  border-radius: var(--r-control);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--ink-muted);
  font-size: var(--fs-body);
}

.insight-placeholder.error-state {
  border-color: var(--danger);
  color: var(--danger);
}
</style>
