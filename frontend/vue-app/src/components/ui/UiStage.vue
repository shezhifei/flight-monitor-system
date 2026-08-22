<script setup lang="ts">
/**
 * 仪器（信号面 §3.1）：一页一块工作面，禁止套盒。
 *
 * 从上到下只有四层，少一层可以，多一层不行：
 *   1. 地点条  #place    —— 面包屑只报地点 + 当前谓词下的读出
 *   2. 工具条  #toolbar  —— 寻左决右，常驻不随表滚
 *   3. 表/主体 default   —— 工作面延续下去，滚动口只在这里
 *   4. 升      #alert    —— 只在事态升级时插在工具条与表之间
 *
 * 旁路（详情）用 #aside：降一级到页底，不再嵌第二张工作面。
 * 组件自己就是那唯一一张工作面：里面不要再描边、不要再换圆角。
 */
withDefaults(defineProps<{
  /** 无障碍名称，落在 section 上 */
  label?: string;
  /** 主体是否自带滚动口；长表用 true，短表单可给 false 让页面滚 */
  scroll?: boolean;
  /** 主体内边距：表贴边（none），表单/卡列给 body */
  pad?: 'none' | 'body';
  /** 旁路宽度（仅在提供 #aside 时生效） */
  asideWidth?: string;
}>(), {
  label: undefined,
  scroll: true,
  pad: 'none',
  asideWidth: '360px',
});
</script>

<template>
  <section class="ui-stage" :aria-label="label" :data-scroll="scroll ? 'true' : 'false'">
    <div class="ui-stage__main">
      <!-- 常驻骨架：钉在滚动口外，表自己滚 -->
      <div v-if="$slots.place || $slots.toolbar || $slots.alert" class="ui-stage__fixed">
        <slot name="place" />
        <slot name="toolbar" />
        <div v-if="$slots.alert" class="ui-stage__alert">
          <slot name="alert" />
        </div>
      </div>
      <div class="ui-stage__body" :data-pad="pad">
        <slot />
      </div>
    </div>
    <aside
      v-if="$slots.aside"
      class="ui-stage__aside"
      :style="{ '--stage-aside-w': asideWidth }"
    >
      <slot name="aside" />
    </aside>
  </section>
</template>

<style scoped>
.ui-stage {
  display: flex;
  align-items: stretch;
  min-height: 0;
  min-width: 0;
  /* 唯一一张工作面 */
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  overflow: hidden;
}

.ui-stage__main {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
}

/* 地点条 + 工具条 + 升：常驻，不随表滚 */
.ui-stage__fixed {
  flex: none;
  border-bottom: 1px solid var(--line);
}

.ui-stage__alert {
  padding: 0 16px 12px;
}

.ui-stage__body {
  flex: 1 1 auto;
  min-height: 0;
  min-width: 0;
}

.ui-stage[data-scroll='true'] .ui-stage__body {
  overflow: auto;
}

.ui-stage__body[data-pad='body'] {
  padding: var(--s3) 16px 16px;
}

/* 旁路降为页底一级，形成凹面；自带滚动口 */
.ui-stage__aside {
  flex: none;
  width: var(--stage-aside-w, 360px);
  min-width: 0;
  border-left: 1px solid var(--line);
  background: var(--face-page);
  overflow: auto;
}

@media (max-width: 960px) {
  .ui-stage {
    flex-direction: column;
  }

  .ui-stage__aside {
    width: auto;
    border-left: 0;
    border-top: 1px solid var(--line);
  }
}
</style>
