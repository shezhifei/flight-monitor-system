<script setup lang="ts">
import { ref, watch, nextTick, onUnmounted } from 'vue';
import { useAiStream } from '../../composables/useAiStream';
import { fetchFlightEventJourney } from '../../composables/useFlightData';
import { useAuth } from '../../composables/useAuth';

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
    
    // Handle different message types from SSE
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

function handleOverlayClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-overlay')) {
    emit('close');
  }
}

onUnmounted(() => {
  clearTerminalTimeout();
  stopStream();
});
</script>

<template>
  <teleport to="body">
    <transition name="modal-fade">
      <div
        v-if="isOpen"
        class="modal modal-overlay"
        role="dialog"
        aria-modal="true"
        aria-labelledby="flightInsightModalTitle"
        @click="handleOverlayClick"
      >
        <div class="modal-container flight-insight-dialog">
          <div class="modal-header ai-chat-header">
            <div class="ai-chat-title-wrap">
              <h3 id="flightInsightModalTitle" style="color:var(--text-primary, #000)">
                全景 AI 洞察网络 - {{ flightNo }}
              </h3>
              <div class="ai-chat-meta">
                深度推演航班流转全生命周期异常因素与级联影响
              </div>
            </div>
            <div class="header-status">
              <span class="status-badge" :class="{ 'pulse': isAnalyzing }">
                {{ isAnalyzing ? 'ANALYZING' : 'COMPLETED' }}
              </span>
              <button
                class="close-modal ai-chat-close"
                type="button"
                aria-label="关闭洞察"
                @click="emit('close')"
              >
                &times;
              </button>
            </div>
          </div>
          
          <div class="modal-body flight-insight-body">
            <div class="insight-layout-wrapper">
              <div class="insight-left-panel">
                <div class="insight-terminal">
                  <div class="insight-terminal-header">
                    <span class="insight-terminal-title">TERMINAL > AI_DIAGNOSTICS</span>
                    <span class="insight-terminal-status" :class="isAnalyzing ? 'highlight-orange' : 'highlight-green'">
                      {{ isAnalyzing ? 'PROCESSING' : 'STANDBY' }}
                    </span>
                  </div>
                  <div ref="terminalBody" class="insight-terminal-body">
                    <p v-for="log in logs" :key="log.id" :class="`log-${log.type}`">
                      <span class="log-time">[{{ log.timestamp }}]</span> {{ log.content }}
                    </p>
                    <div v-if="isAnalyzing" class="terminal-cursor" />
                    <div v-if="logs.length === 0" class="terminal-placeholder">
                      Awaiting backend connection...
                    </div>
                  </div>
                </div>
              </div>
              <div class="insight-right-panel">
                <div class="panel-section-header">
                  <h4>全域影响级联扩散拓扑</h4>
                  <span v-if="topologyNodes.length" class="node-count">{{ topologyNodes.length }} 影响点</span>
                </div>
                
                <div v-if="topologyNodes.length > 0" class="topology-container">
                  <div
                    v-for="node in topologyNodes"
                    :key="node.id"
                    class="insight-node-card"
                    :class="node.status"
                  >
                    <div class="node-icon">
                      {{ node.type === 'flight' ? '✈️' : (node.type === 'resource' ? '🔧' : '⚠️') }}
                    </div>
                    <div class="node-info">
                      <div class="node-label">
                        {{ node.label }}
                      </div>
                      <div class="node-desc">
                        {{ node.description }}
                      </div>
                    </div>
                    <div class="node-status-tag">
                      {{ node.status }}
                    </div>
                  </div>
                </div>
                
                <div v-else-if="analysisError" class="insight-topology-placeholder error-state">
                  <span class="error-icon">⚠️</span>
                  <span>{{ analysisError }}</span>
                </div>

                <div v-else class="insight-topology-placeholder">
                  <div class="pulse-ring" />
                  <span>{{ isAnalyzing ? '正在推演级联影响...' : '等待任务启动' }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </transition>
  </teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  top: 0; left: 0; right: 0; bottom: 0;
  background-color: rgba(0, 0, 0, 0.75);
  z-index: 10000;
  display: flex;
  align-items: center;
  justify-content: center;
  backdrop-filter: blur(6px);
}

.modal-container {
  background-color: var(--bg-app, #F5F5F7);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 12px;
  width: 95%;
  max-width: 1400px;
  height: 85vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 60px rgba(0,0,0,0.5);
  overflow: hidden;
}

.modal-header {
  padding: 16px 24px;
  border-bottom: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  display: flex;
  justify-content: space-between;
  align-items: center;
  background-color: var(--bg-card, #fff);
  /* 覆盖全局 layout.css .modal-header 的 margin-bottom: 1.25rem */
  margin-bottom: 0;
}

.header-status {
  display: flex;
  align-items: center;
  gap: 16px;
}

.status-badge {
  font-family: monospace;
  font-size: 11px;
  font-weight: 700;
  padding: 4px 8px;
  border-radius: 4px;
  background: var(--admin-text-muted);
  color: var(--admin-text-muted);
}

.status-badge.pulse {
  background: var(--system-blue-subtle);
  color: var(--system-blue);
  animation: badge-pulse 1.5s infinite;
}

@keyframes badge-pulse {
  0% { opacity: 1; }
  50% { opacity: 0.6; }
  100% { opacity: 1; }
}

.ai-chat-title-wrap h3 {
  margin: 0 0 4px 0;
  font-size: 18px;
  font-weight: 700;
}
.ai-chat-meta {
  font-size: 12px;
  color: var(--text-tertiary, #9CA3AF);
}

.close-modal {
  background: none;
  border: none;
  color: var(--text-secondary, #546E7A);
  font-size: 28px;
  cursor: pointer;
  line-height: 1;
}

.modal-body {
  padding: 0;
  flex-grow: 1;
  display: flex;
  overflow: hidden;
}

.insight-layout-wrapper {
  display: flex;
  width: 100%;
  height: 100%;
}

.insight-left-panel {
  flex: 1.2;
  border-right: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  padding: 24px;
  display: flex;
  flex-direction: column;
  background-color: #1a1a1a; /* Dark terminal look */
}

.insight-terminal {
  flex-grow: 1;
  background-color: #000;
  border: 1px solid var(--admin-text);
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  font-family: 'Fira Code', 'Monaco', monospace;
  overflow: hidden;
  box-shadow: inset 0 0 10px rgba(0,0,0,0.5);
}

.insight-terminal-header {
  background-color: var(--admin-text-muted);
  padding: 10px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid #3d3d3d;
  font-size: 11px;
}

.insight-terminal-title {
  color: var(--admin-text-muted);
}
.insight-terminal-status {
  font-weight: bold;
}

.insight-terminal-body {
  padding: 20px;
  font-size: 13px;
  color: #e0e0e0;
  line-height: 1.6;
  overflow-y: auto;
  flex: 1;
}

.insight-terminal-body p {
  margin: 0 0 6px 0;
}

.log-time {
  color: var(--admin-text-muted);
  margin-right: 8px;
}
.log-success { color: var(--system-green); }
.log-error { color: var(--system-red); }
.log-warning { color: var(--system-orange); }
.log-worker { color: var(--system-blue); }

.terminal-cursor {
  display: inline-block;
  width: 8px;
  height: 15px;
  background: var(--system-blue);
  margin-left: 4px;
  animation: cursor-blink 1s infinite;
  vertical-align: middle;
}

@keyframes cursor-blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0; }
}

.terminal-placeholder {
  color: var(--admin-text-muted);
  text-align: center;
  margin-top: 40px;
}

.insight-right-panel {
  flex: 1;
  padding: 24px;
  display: flex;
  flex-direction: column;
  background: white;
}

.panel-section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.insight-right-panel h4 {
  margin: 0;
  color: var(--system-blue, #007AFF);
  font-size: 16px;
  font-weight: 700;
}

.node-count {
  font-size: 12px;
  background: var(--bg-app);
  padding: 2px 8px;
  border-radius: 99px;
  color: var(--admin-text-muted);
}

.topology-container {
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow-y: auto;
  flex: 1;
}

.insight-node-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 16px;
  border: 1px solid var(--border-light);
  border-radius: 10px;
  background: var(--bg-card);
  transition: all 0.3s;
  animation: slide-in 0.4s ease-out;
}

@keyframes slide-in {
  from { opacity: 0; transform: translateX(20px); }
  to { opacity: 1; transform: translateX(0); }
}

.insight-node-card:hover {
  border-color: var(--system-blue);
  box-shadow: 0 4px 12px rgba(0,0,0,0.05);
}

.insight-node-card.affected {
  border-left: 4px solid var(--system-orange);
}
.insight-node-card.critical {
  border-left: 4px solid var(--system-red);
}

.node-icon {
  font-size: 20px;
}

.node-info {
  flex: 1;
}

.node-label {
  font-weight: 700;
  font-size: 14px;
  color: var(--text-primary);
  margin-bottom: 4px;
}

.node-desc {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.4;
}

.node-status-tag {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--admin-border);
  text-transform: uppercase;
}

.insight-topology-placeholder {
  flex-grow: 1;
  border: 1px dashed var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  position: relative;
  background-color: var(--bg-app, #F5F5F7);
  gap: 16px;
}

.pulse-ring {
  width: 50px;
  height: 50px;
  border-radius: 50%;
  border: 2px solid var(--system-blue, #007AFF);
  animation: pulse 2s cubic-bezier(0.25, 0.8, 0.25, 1) infinite;
}

@keyframes pulse {
  0% { transform: scale(0.5); opacity: 1; }
  100% { transform: scale(3); opacity: 0; }
}

.insight-topology-placeholder span {
  color: var(--text-tertiary, #9CA3AF);
  font-family: monospace;
  font-size: 13px;
}

.insight-topology-placeholder.error-state {
  border-color: var(--system-red, #FF3B30);
  color: var(--system-red, #FF3B30);
}

.error-icon {
  font-size: 24px;
}

.highlight-orange { color: var(--system-orange, #FF9500); }
.highlight-green { color: var(--system-green, #34C759); }

.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity 0.3s ease;
}

.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;
}
</style>
