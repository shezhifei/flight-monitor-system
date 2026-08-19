<script setup lang="ts">
import type { SubagentNodeModel, SubagentNodeStatus } from '@/lib/ai/subagentTreeModel';

withDefaults(defineProps<{
  nodes: SubagentNodeModel[];
  title?: string;
}>(), {
  title: '子代理树',
});

function toneOf(status: SubagentNodeStatus): string {
  if (status === 'done') return 'ok';
  if (status === 'error') return 'danger';
  return 'act';
}

function statusLabel(status: SubagentNodeStatus): string {
  if (status === 'done') return '已完成';
  if (status === 'error') return '失败';
  return '运行中';
}
</script>

<template>
  <section class="ai-subagent-tree" data-testid="subagent-tree">
    <h4 class="ai-panel-title">{{ title }}</h4>
    <p v-if="!nodes.length" class="ai-panel-empty">暂无子代理</p>
    <ul v-else class="ai-sub-list">
      <li
        v-for="node in nodes"
        :key="node.id"
        class="ai-sub-node"
        :style="{ paddingLeft: `${Math.max(0, node.depth - 1) * 20}px` }"
      >
        <span class="ai-sub-dot" :data-tone="toneOf(node.status)" aria-hidden="true" />
        <span class="ai-sub-label">{{ node.label || node.id }}</span>
        <span class="ai-sub-tag" :data-tone="toneOf(node.status)">{{ statusLabel(node.status) }}</span>
        <span v-if="node.proposalOnly" class="ai-sub-tag is-warn">proposal_only</span>
        <span v-if="node.toolCalls > 0" class="ai-sub-meta">工具调用 {{ node.toolCalls }}</span>
        <span v-if="node.lastActivity" class="ai-sub-meta is-mono">{{ node.lastActivity }}</span>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.ai-subagent-tree {
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  background: var(--face-work);
  padding: 12px 14px;
}

.ai-panel-title {
  margin: 0 0 8px;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.ai-panel-empty {
  margin: 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
  text-align: center;
  padding: 12px 0;
}

.ai-sub-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.ai-sub-node {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  font-size: var(--fs-label);
  line-height: 22px;
}

.ai-sub-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.ai-sub-dot[data-tone='ok'] { background: var(--ok); }
.ai-sub-dot[data-tone='danger'] { background: var(--danger); }
.ai-sub-dot[data-tone='act'] { background: var(--act); }

.ai-sub-label {
  font-weight: var(--fw-medium);
  color: var(--ink);
}

.ai-sub-tag {
  font-size: 11px;
  padding: 0 6px;
  border-radius: var(--r-cell);
}

.ai-sub-tag[data-tone='ok'] { color: var(--ok); background: var(--ok-soft); }
.ai-sub-tag[data-tone='danger'] { color: var(--danger); background: var(--danger-soft); }
.ai-sub-tag[data-tone='act'] { color: var(--act); background: var(--act-soft); }
.ai-sub-tag.is-warn { color: var(--warn); background: var(--warn-soft); }

.ai-sub-meta {
  color: var(--ink-subtle);
  font-size: 11px;
}

.ai-sub-meta.is-mono {
  font-family: var(--mono);
}
</style>
