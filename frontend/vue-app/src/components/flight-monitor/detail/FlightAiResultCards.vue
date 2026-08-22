<script setup lang="ts">
import { computed, inject } from 'vue';
import { renderMarkdown } from '../../../lib/marked';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';
import UiButton from '../../ui/UiButton.vue';

const ctx = inject(flightBusinessCaseKey)!;

type AiResultPanel = {
  key: string;
  title: string;
  summary?: string;
  recommendations?: string[];
  details?: string;
  dismiss: () => void;
};

/**
 * 三张 AI 结果原来是三段一模一样的手搓卡：同一个形不写三遍（§2.3 法「同类、同级、同形」）。
 * 谁在场谁排队，形只此一套。
 */
const panels = computed<AiResultPanel[]>(() => {
  const list: AiResultPanel[] = [];
  const diagnosis = ctx.diagnosisResult.value;
  if (diagnosis) {
    list.push({
      key: 'diagnosis',
      title: 'AI 诊断结果',
      ...diagnosis,
      dismiss: () => { ctx.diagnosisResult.value = null; },
    });
  }
  const journey = ctx.journeyResult.value;
  if (journey) {
    list.push({
      key: 'journey',
      title: 'AI 事件全经过',
      ...journey,
      dismiss: () => { ctx.journeyResult.value = null; },
    });
  }
  const report = ctx.reportResult.value;
  if (report) {
    list.push({
      key: 'report',
      title: '动态报表',
      ...report,
      dismiss: () => { ctx.reportResult.value = null; },
    });
  }
  return list;
});
</script>

<template>
  <!-- 每张结果就是详情盘上的一块工作面：.detail-card 已经给了边、圆角和面，
       身里不再描第二道边、不铺渐变（§2.4 面只有三级 / §4.21 禁止套盒）。 -->
  <section v-for="panel in panels" :key="panel.key" class="detail-card ai-result">
    <div class="ai-result__bar">
      <h3 class="ai-result__title">
        {{ panel.title }}
      </h3>
      <!-- 关是可逃的那一档谓词，不是一个 20px 的「×」字形（§2.6） -->
      <UiButton variant="quiet" :aria-label="`关闭${panel.title}`" @click="panel.dismiss()">
        关闭
      </UiButton>
    </div>
    <div class="ai-result__body">
      <div v-if="panel.summary" v-html="renderMarkdown(panel.summary)" />
      <div v-if="panel.recommendations && panel.recommendations.length > 0">
        <p class="ai-result__sub">
          建议措施
        </p>
        <ul class="ai-result__list">
          <li v-for="(rec, i) in panel.recommendations" :key="i" v-html="renderMarkdown(rec)" />
        </ul>
      </div>
      <div v-if="panel.details" v-html="renderMarkdown(panel.details)" />
    </div>
  </section>
</template>

<style scoped>
.ai-result {
  margin-top: var(--s3);
}

/* 帽子跟详情盘里其它小节同一套：一根线分身，不投影、不换圆角 */
.ai-result__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding: 12px 16px;
  border-bottom: 1px solid var(--line);
}

.ai-result__title {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.ai-result__body {
  padding: 12px 16px 16px;
  color: var(--ink-subtle);
  font-size: var(--fs-body);
  line-height: 1.7;
}

.ai-result__body > * + * {
  margin-top: var(--s3);
}

.ai-result__sub {
  margin: 0 0 var(--s2);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-muted);
}

.ai-result__list {
  margin: 0;
  padding-left: var(--s4);
}

.ai-result__list li + li {
  margin-top: var(--s1);
}
</style>
