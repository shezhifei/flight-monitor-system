<script setup lang="ts">
export interface UiTimelineItem {
  key: string;
  title: string;
  time?: string;
  tone?: 'neutral' | 'act' | 'ok' | 'warn' | 'danger';
}

defineProps<{
  items: UiTimelineItem[];
}>();
</script>

<template>
  <ol class="ui-timeline">
    <li v-for="item in items" :key="item.key" class="ui-timeline-item" :data-tone="item.tone ?? 'neutral'">
      <span class="ui-timeline-dot" aria-hidden="true" />
      <div class="ui-timeline-content">
        <div class="ui-timeline-title">{{ item.title }}</div>
        <div v-if="item.time" class="ui-timeline-time">{{ item.time }}</div>
      </div>
    </li>
  </ol>
</template>

<style scoped>
.ui-timeline {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
}

.ui-timeline-item {
  position: relative;
  display: flex;
  gap: 10px;
  padding-bottom: 12px;
}

.ui-timeline-item:last-child {
  padding-bottom: 0;
}

.ui-timeline-item:not(:last-child)::before {
  content: '';
  position: absolute;
  left: 3px;
  top: 12px;
  bottom: 0;
  width: 1px;
  background: var(--line);
}

.ui-timeline-dot {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  margin-top: 5px;
  border-radius: 50%;
  background: var(--ink-muted);
}

.ui-timeline-item[data-tone='act'] .ui-timeline-dot { background: var(--act); }
.ui-timeline-item[data-tone='ok'] .ui-timeline-dot { background: var(--ok); }
.ui-timeline-item[data-tone='warn'] .ui-timeline-dot { background: var(--warn); }
.ui-timeline-item[data-tone='danger'] .ui-timeline-dot { background: var(--danger); }

.ui-timeline-title {
  font-size: var(--fs-body);
  color: var(--ink);
  line-height: 1.5;
}

.ui-timeline-time {
  font-size: 11px;
  font-family: var(--mono);
  color: var(--ink-muted);
  margin-top: 1px;
}
</style>
