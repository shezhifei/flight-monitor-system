<script lang="ts">
/**
 * 一件事的最小形：组件只认这四样，各页可以往上加字段（胶囊要的数、原始对象）。
 * 组件是泛型的，加出来的字段会原样出现在 #item 槽里，类型不丢。
 */
export interface UiTimelineItem {
  key: string;
  title: string;
  time?: string;
  tone?: 'mute' | 'act' | 'ok' | 'warn' | 'danger';
}
</script>

<script setup lang="ts" generic="T extends UiTimelineItem">
/**
 * 时间线（信号面 §2.4）：一列事件。点的色就是那件事的事态，鞭只是一根 line。
 *
 * 行里要放胶囊、要能点开详情，就用 #item 槽自己渲染 ——
 * 点、鞭、声仍归本组件，各页不要再画第二套点和第二套 tone 映射。
 */
defineProps<{
  items: T[];
}>();
</script>

<template>
  <ol class="ui-timeline">
    <li
      v-for="(item, index) in items"
      :key="item.key"
      class="ui-timeline-item"
      :data-tone="item.tone ?? 'mute'"
    >
      <span class="ui-timeline-dot" aria-hidden="true" />
      <div class="ui-timeline-content">
        <slot name="item" :item="item" :index="index">
          <div class="ui-timeline-title">{{ item.title }}</div>
          <div v-if="item.time" class="ui-timeline-time">{{ item.time }}</div>
        </slot>
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

/* 槽里放整行（名 + 时刻 + 胶囊）时要能自己撑满、自己截断 */
.ui-timeline-content {
  flex: 1;
  min-width: 0;
}

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
