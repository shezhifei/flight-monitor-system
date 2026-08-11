<script setup lang="ts">
import { inject } from 'vue';
import { renderMarkdown } from '../../../lib/marked';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';

const ctx = inject(flightBusinessCaseKey)!;
</script>

<template>
  <section v-if="ctx.diagnosisResult.value" class="detail-card diagnosis-card">
    <div class="diagnosis-header">
      <h3 class="diagnosis-title">
        AI 诊断结果
      </h3>
      <button class="close-diagnosis" aria-label="关闭诊断" @click="ctx.diagnosisResult.value = null">
        ×
      </button>
    </div>
    <div class="diagnosis-content">
      <div v-if="ctx.diagnosisResult.value.summary" class="diagnosis-summary" v-html="renderMarkdown(ctx.diagnosisResult.value.summary)" />
      <div v-if="ctx.diagnosisResult.value.recommendations && ctx.diagnosisResult.value.recommendations.length > 0" class="diagnosis-recommendations">
        <h4>建议措施</h4>
        <ul class="recommendation-list">
          <li v-for="(rec, i) in ctx.diagnosisResult.value.recommendations" :key="i" v-html="renderMarkdown(rec)" />
        </ul>
      </div>
      <div v-if="ctx.diagnosisResult.value.details" class="diagnosis-details" v-html="renderMarkdown(ctx.diagnosisResult.value.details)" />
    </div>
  </section>

  <section v-if="ctx.journeyResult.value" class="detail-card diagnosis-card">
    <div class="diagnosis-header">
      <h3 class="diagnosis-title">
        AI 事件全经过
      </h3>
      <button class="close-diagnosis" aria-label="关闭" @click="ctx.journeyResult.value = null">
        ×
      </button>
    </div>
    <div class="diagnosis-content">
      <div v-if="ctx.journeyResult.value.summary" class="diagnosis-summary" v-html="renderMarkdown(ctx.journeyResult.value.summary)" />
      <div v-if="ctx.journeyResult.value.details" class="diagnosis-details" v-html="renderMarkdown(ctx.journeyResult.value.details)" />
    </div>
  </section>

  <section v-if="ctx.reportResult.value" class="detail-card diagnosis-card">
    <div class="diagnosis-header">
      <h3 class="diagnosis-title">
        动态报表
      </h3>
      <button class="close-diagnosis" aria-label="关闭" @click="ctx.reportResult.value = null">
        ×
      </button>
    </div>
    <div class="diagnosis-content">
      <div v-if="ctx.reportResult.value.summary" class="diagnosis-summary" v-html="renderMarkdown(ctx.reportResult.value.summary)" />
      <div v-if="ctx.reportResult.value.details" class="diagnosis-details" v-html="renderMarkdown(ctx.reportResult.value.details)" />
    </div>
  </section>
</template>

<style scoped>
.diagnosis-card {
  margin-top: 16px;
  border: 1px solid var(--color-primary-light, #bbdefb);
  border-radius: 8px;
  background: linear-gradient(135deg, var(--status-bg-scheduled) 0%, var(--bg-card) 100%);
}
.diagnosis-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
.diagnosis-title { margin: 0; font-size: 15px; font-weight: 600; }
.close-diagnosis { background: none; border: none; font-size: 20px; cursor: pointer; color: var(--text-secondary); padding: 0 4px; line-height: 1; }
.close-diagnosis:hover { color: var(--text-primary); }
.diagnosis-summary { margin-bottom: 12px; line-height: 1.7; }
.diagnosis-recommendations { margin-bottom: 12px; }
.diagnosis-recommendations h4 { margin: 0 0 8px; font-size: 14px; font-weight: 600; }
.recommendation-list { margin: 0; padding-left: 20px; }
.recommendation-list li { margin-bottom: 4px; line-height: 1.6; }
.diagnosis-details { line-height: 1.7; color: var(--text-secondary); }
</style>
