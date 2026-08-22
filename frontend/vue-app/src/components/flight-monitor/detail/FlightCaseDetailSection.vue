<script setup lang="ts">
import { computed, inject } from 'vue';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';
import DynamicWorkflowForm from '../DynamicWorkflowForm.vue';
import MentionInput from '../MentionInput.vue';
import UiAvatar from '../../ui/UiAvatar.vue';
import UiBanner from '../../ui/UiBanner.vue';
import UiButton from '../../ui/UiButton.vue';
import UiFacts, { type Fact } from '../../ui/UiFacts.vue';
import UiModal from '../../ui/UiModal.vue';
import UiPill from '../../ui/UiPill.vue';
import UiReadout from '../../ui/UiReadout.vue';
import UiSelect from '../../ui/UiSelect.vue';
import UiSkeleton from '../../ui/UiSkeleton.vue';
import {
  formatCaseTime,
  getAppendAcknowledgedCount,
  getAppendAcknowledgedTime,
  getAppendAuthorBadge,
  getAppendDisplayName,
  getAppendMentionCount,
  getBoundCaseFlightLabel,
  getCaseAuthorBadge,
  getCaseDisplayName,
  getCaseStatusLabel,
  getCaseStatusTone,
  getReceiptItemAccountName,
  getReceiptItemStatusLabel,
  getReceiptItemStatusTone,
  getReceiptSeverityLabel,
  getReceiptSeverityTone,
  getWorkflowFormMetaText,
  getWorkflowFormStatusLabel,
  getWorkflowFormStatusTone,
  getWorkflowProjectionDisplayData,
  getWorkflowProjectionMetaText,
  getWorkflowSubmissionDisplayData,
  hasAppendAcknowledged,
} from './businessCaseHelpers';

/**
 * 业务事项详情：弹窗里的一件事（信号面 §3.8）。
 *
 * 身给 bleed —— 里面是「主贴 + 旁路」这一整套栏布局，自己撑高、自己滚。
 * 帽、幕、Esc、关都归 UiModal，本页不再自备遮罩与 body 滚动锁。
 * 楼与楼、块与块之间只有一根线：不做卡、不描第二道边、不铺第二张工作面。
 */
const ctx = inject(flightBusinessCaseKey)!;

const isOpen = computed(() => Boolean(ctx.activeCaseId.value));
const caseData = computed(() => ctx.activeCaseData.value);
const hasWorkflowColumn = computed(() => ctx.activeCaseHasWorkflowPanel.value);
/** 仅在尚无详情数据时画骨架（有旧数据就留着内容，不要整页闪一下） */
const showSkeleton = computed(() => Boolean(ctx.caseDetailLoading.value) && !caseData.value);

const appendEntries = computed(() => ctx.activeCaseAppendEntries.value);
const receipt = computed(() => ctx.activeCaseReceipt.value);
const receiptItems = computed(() => receipt.value?.items || []);
const workflowForms = computed(() => ctx.activeCaseWorkflowForms.value?.forms || []);
const workflowProjections = computed(() => ctx.activeCaseWorkflowProjectionEntries.value);
/** 与旧版一致：拉表单失败但手上有内容时，先把失败说出来 */
const showWorkflowError = computed(() => Boolean(
  ctx.workflowFormsError.value && (workflowForms.value.length > 0 || workflowProjections.value.length > 0),
));

/** 事项概览：状态与范围已经在帽下那条上，这里只报属性 */
const facts = computed<Fact[]>(() => {
  const data = caseData.value;
  if (!data) return [];
  const visibility = ctx.activeCaseVisibility.value;
  const items: Fact[] = [
    { label: '创建人', value: data.created_by },
    { label: '创建时间', value: formatCaseTime(data.created_at), mono: true },
  ];
  if (data.finished_at) {
    items.push({ label: '完成时间', value: formatCaseTime(data.finished_at), mono: true });
  }
  if (visibility.departmentName) {
    items.push({ label: '归属部门', value: visibility.departmentName });
  }
  const boundFlight = getBoundCaseFlightLabel(data.context);
  if (boundFlight) {
    items.push({ label: '绑定航班', value: boundFlight });
  }
  return items;
});
</script>

<template>
  <UiModal
    id="businessCaseDetailModal"
    :open="isOpen"
    title="业务事项详情"
    :width="hasWorkflowColumn ? 1520 : 1180"
    bleed
    @close="ctx.closeCaseDetail"
  >
    <!-- 等的时候画版：与最终布局同构，不写「加载中…」 -->
    <div
      v-if="showSkeleton"
      class="case"
      aria-busy="true"
      aria-label="业务事项详情加载中"
    >
      <div class="case__bar">
        <UiSkeleton width="220px" />
      </div>
      <div class="case__cols">
        <div class="case__main">
          <UiSkeleton width="88px" height="14px" />
          <div class="case__floor">
            <div class="case__floor-side">
              <UiSkeleton shape="circle" width="30px" />
            </div>
            <div class="case__sk-lines">
              <UiSkeleton width="40%" />
              <UiSkeleton />
              <UiSkeleton width="72%" />
            </div>
          </div>
          <div class="case__floor">
            <div class="case__floor-side">
              <UiSkeleton shape="circle" width="24px" />
            </div>
            <div class="case__sk-lines">
              <UiSkeleton width="32%" />
              <UiSkeleton width="56%" />
            </div>
          </div>
          <div class="case__sk-editor">
            <UiSkeleton shape="block" height="88px" />
          </div>
        </div>
        <aside class="case__aside">
          <UiSkeleton width="72px" height="14px" />
          <div class="case__sk-facts">
            <div v-for="n in 4" :key="`fact-sk-${n}`" class="case__sk-fact">
              <UiSkeleton width="48px" height="10px" />
              <UiSkeleton width="80%" />
            </div>
          </div>
        </aside>
      </div>
    </div>

    <div
      v-else-if="caseData"
      class="case"
      :class="{ 'case--wide': hasWorkflowColumn }"
    >
      <div class="case__bar">
        <span class="case__subject">{{ getCaseDisplayName(caseData) }}</span>
        <UiPill :tone="getCaseStatusTone(caseData.status, ctx.caseStatusOptions.value)">
          {{ getCaseStatusLabel(caseData.status, ctx.caseStatusOptions.value) }}
        </UiPill>
        <UiPill>{{ ctx.activeCaseVisibility.value.scopeLabel }}</UiPill>
      </div>

      <div class="case__cols">
        <div class="case__main">
          <section class="case__block">
            <div class="case__block-head">
              <h4 class="case__block-title">
                事项记录
              </h4>
              <span class="case__block-note">
                共 {{ ctx.activeCaseThreadTotal.value }} 条记录
                <template v-if="appendEntries.length > 0"> · 追加 {{ appendEntries.length }} 次</template>
              </span>
            </div>

            <article class="case__floor">
              <div class="case__floor-side">
                <UiAvatar
                  :text="getCaseAuthorBadge(caseData.created_by)"
                  :label="caseData.created_by || '系统'"
                />
                <span class="case__floor-name">{{ caseData.created_by || '系统' }}</span>
              </div>
              <div class="case__floor-body">
                <div class="case__floor-head">
                  <span class="case__floor-meta">
                    发布于 {{ formatCaseTime(caseData.created_at) }}
                    <template v-if="caseData.finished_at"> · 完成于 {{ formatCaseTime(caseData.finished_at) }}</template>
                  </span>
                </div>
                <p class="case__floor-text">
                  {{ caseData.description || '当前事项未填写描述。' }}
                </p>
              </div>
            </article>

            <article
              v-for="entry in appendEntries"
              :key="entry.append_id"
              class="case__floor"
            >
              <div class="case__floor-side">
                <UiAvatar
                  size="sm"
                  :text="getAppendAuthorBadge(entry)"
                  :label="getAppendDisplayName(entry)"
                />
                <span class="case__floor-name">{{ getAppendDisplayName(entry) }}</span>
              </div>
              <div class="case__floor-body">
                <div class="case__floor-head">
                  <span class="case__floor-meta">
                    {{ formatCaseTime(entry.appended_at) }} · {{ entry.submitted_by }}
                  </span>
                  <span v-if="getAppendMentionCount(entry) > 0" class="case__floor-tags">
                    <UiPill tone="act">@{{ getAppendMentionCount(entry) }} 人</UiPill>
                    <UiPill :tone="getAppendAcknowledgedCount(entry) >= getAppendMentionCount(entry) ? 'ok' : 'warn'">
                      已确认 {{ getAppendAcknowledgedCount(entry) }}/{{ getAppendMentionCount(entry) }}
                    </UiPill>
                  </span>
                </div>

                <p class="case__floor-text">
                  {{ entry.content }}
                </p>

                <div
                  v-if="entry.metadata.mention_user_ids?.includes(ctx.getCurrentUserId())"
                  class="case__floor-foot"
                >
                  <UiButton
                    v-if="!hasAppendAcknowledged(entry, ctx.getCurrentUserId())"
                    @click.stop="ctx.acknowledgeAppend(entry)"
                  >
                    确认收到
                  </UiButton>
                  <UiPill v-else tone="ok">
                    已确认 {{ getAppendAcknowledgedTime(entry, ctx.getCurrentUserId()) }}
                  </UiPill>
                </div>
              </div>
            </article>

            <p v-if="appendEntries.length === 0" class="case__quiet">
              暂无回复。
            </p>
          </section>

          <section class="case__block">
            <h4 class="case__block-title">
              追加回复
            </h4>
            <div class="case__editor">
              <MentionInput
                v-model="ctx.appendContent.value"
                :stakeholders="ctx.mentionCandidates.value"
                @update:mention-ids="ctx.appendMentionIds.value = $event"
              />
            </div>
            <p v-if="ctx.appendMentionIds.value.length > 0" class="case__quiet">
              将通知 {{ ctx.appendMentionIds.value.length }} 人
            </p>
            <div class="case__actions">
              <UiButton @click="ctx.closeCaseDetail">
                关闭
              </UiButton>
              <UiButton
                variant="primary"
                :disabled="!ctx.appendContent.value.trim() || ctx.appendSubmitting.value"
                @click="ctx.submitAppend"
              >
                {{ ctx.appendSubmitting.value ? '提交中…' : '发布回复' }}
              </UiButton>
            </div>
          </section>
        </div>

        <aside class="case__aside">
          <section class="case__block">
            <h4 class="case__block-title">
              事项概览
            </h4>
            <div class="case__facts">
              <UiFacts :items="facts" />
            </div>
          </section>

          <section v-if="ctx.canAttemptActiveCaseStatusEdit.value" class="case__block">
            <h4 class="case__block-title">
              状态流转
            </h4>
            <div class="case__row">
              <UiSelect
                v-model="ctx.caseStatusDraft.value"
                :options="ctx.activeCaseStatusOptions.value"
                :disabled="ctx.caseStatusSaving.value"
                label="目标状态"
                min-width="150px"
              />
              <UiButton
                :disabled="ctx.caseStatusSaving.value || ctx.caseStatusDraft.value === ctx.activeCaseStatusValue.value"
                @click="ctx.submitCaseStatusUpdate"
              >
                {{ ctx.caseStatusSaving.value ? '更新中…' : '更新状态' }}
              </UiButton>
            </div>
            <p v-if="ctx.showCaseStatusPermissionHint.value" class="case__quiet">
              当前账号可尝试更新，最终以后端权限校验为准。
            </p>
            <div v-if="ctx.caseStatusMetadataError.value" class="case__note">
              <UiBanner tone="warn">
                {{ ctx.caseStatusMetadataError.value }}
              </UiBanner>
            </div>
          </section>

          <section v-if="receipt" class="case__block">
            <div class="case__block-head">
              <h4 class="case__block-title">
                通知回执
              </h4>
              <UiPill :tone="getReceiptSeverityTone(receipt)">
                级别 {{ getReceiptSeverityLabel(receipt) }}
              </UiPill>
            </div>

            <div class="case__counts" role="group" aria-label="回执统计">
              <UiReadout label="已回执" :value="receipt.summary.acknowledged_count" tone="ok" />
              <UiReadout label="待回执" :value="receipt.summary.pending_count" tone="warn" />
              <UiReadout label="拒绝" :value="receipt.summary.rejected_count" tone="danger" />
            </div>

            <div v-if="receiptItems.length > 0" class="case__receipts">
              <div
                v-for="item in receiptItems"
                :key="`${item.user_id}-${item.updated_at || item.ack_at || ''}`"
                class="case__receipt"
              >
                <span class="case__receipt-user">{{ getReceiptItemAccountName(item) }}</span>
                <UiPill :tone="getReceiptItemStatusTone(item)">
                  {{ getReceiptItemStatusLabel(item) }}
                </UiPill>
                <span class="case__receipt-time">{{ item.ack_at ? formatCaseTime(item.ack_at) : '—' }}</span>
                <span v-if="item.ack_note" class="case__receipt-note">{{ item.ack_note }}</span>
              </div>
            </div>
          </section>
        </aside>

        <aside v-if="hasWorkflowColumn" class="case__aside">
          <section class="case__block">
            <h4 class="case__block-title">
              流程表单
            </h4>

            <div v-if="ctx.workflowFormsLoading.value" class="case__sk-form" aria-busy="true">
              <UiSkeleton width="40%" />
              <UiSkeleton shape="block" height="120px" />
            </div>

            <div v-else-if="showWorkflowError" class="case__note">
              <UiBanner tone="danger">
                {{ ctx.workflowFormsError.value }}
              </UiBanner>
            </div>

            <div v-else-if="workflowForms.length > 0" class="case__forms">
              <section
                v-for="form in workflowForms"
                :key="`${form.task_id}-${form.form_code}`"
                class="case__form"
              >
                <div class="case__form-head">
                  <div class="case__form-ident">
                    <div class="case__form-title">
                      {{ form.name }}
                    </div>
                    <div class="case__form-meta">
                      {{ form.task_name }} · {{ form.form_code }} · v{{ form.form_version }}
                    </div>
                  </div>
                  <UiPill :tone="getWorkflowFormStatusTone(form, caseData)">
                    {{ getWorkflowFormStatusLabel(form, caseData) }}
                  </UiPill>
                </div>

                <p v-if="getWorkflowFormMetaText(form, caseData)" class="case__quiet">
                  {{ getWorkflowFormMetaText(form, caseData) }}
                </p>
                <p v-else-if="!form.can_submit && form.readonly_reason" class="case__quiet">
                  {{ form.readonly_reason }}
                </p>

                <DynamicWorkflowForm
                  :schema="form.schema"
                  :ui-schema="form.ui_schema"
                  :initial-value="getWorkflowSubmissionDisplayData(form, caseData)"
                  :readonly="!form.can_submit"
                  :submitting="ctx.workflowFormSubmittingCode.value === form.form_code"
                  empty-text="暂无表单字段"
                  @submit="ctx.submitWorkflowForm(form, $event)"
                />
              </section>
            </div>

            <div v-else-if="workflowProjections.length > 0" class="case__forms">
              <section
                v-for="projection in workflowProjections"
                :key="projection.submission_id"
                class="case__form"
              >
                <div class="case__form-head">
                  <div class="case__form-ident">
                    <div class="case__form-title">
                      {{ projection.form_code }}
                    </div>
                    <div class="case__form-meta">
                      {{ projection.task_definition_key }} · v{{ projection.form_version }}
                    </div>
                  </div>
                  <UiPill tone="ok">
                    已提交
                  </UiPill>
                </div>

                <p class="case__quiet">
                  {{ getWorkflowProjectionMetaText(projection) }}
                </p>

                <DynamicWorkflowForm
                  :initial-value="getWorkflowProjectionDisplayData(projection)"
                  readonly
                  empty-text="暂无可回显的表单内容"
                />
              </section>
            </div>
          </section>
        </aside>
      </div>
    </div>

    <p v-else class="case__empty">
      未能加载业务事项详情
    </p>
  </UiModal>
</template>

<style scoped>
/* 三栏都长在弹窗的身上（bleed）：这里只有栏、线与间距 —— 形都在库里。 */
.case {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

/* 帽下一条：事项类型 + 状态 + 范围，只此三样 */
.case__bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--s2);
  padding: var(--s2) var(--s4);
  border-bottom: 1px solid var(--line);
}

.case__subject {
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--ink);
}

.case__cols {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  display: grid;
  grid-template-columns: minmax(0, 1.5fr) minmax(300px, 0.92fr);
}

.case--wide .case__cols {
  grid-template-columns: minmax(0, 1.2fr) minmax(260px, 0.72fr) minmax(300px, 0.9fr);
}

.case__main {
  min-width: 0;
  padding: var(--s3) var(--s4) var(--s4);
}

/* 旁路降一级：页底 + 一根线分栏，不再嵌第二张工作面 */
.case__aside {
  min-width: 0;
  padding: var(--s3) var(--s4) var(--s4);
  background: var(--face-page);
  border-left: 1px solid var(--line);
}

.case__block + .case__block {
  margin-top: var(--s3);
  padding-top: var(--s3);
  border-top: 1px solid var(--line);
}

.case__block-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s2);
}

.case__block-title {
  margin: 0;
  font-size: var(--fs-section);
  font-weight: var(--fw-medium);
  color: var(--ink);
}

.case__block-note {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  text-align: right;
}

.case__quiet {
  margin: var(--s2) 0 0;
  font-size: var(--fs-label);
  line-height: 1.5;
  color: var(--ink-muted);
}

.case__note {
  margin-top: var(--s2);
}

.case__facts,
.case__editor {
  margin-top: var(--s2);
}

/* 一层楼：头像在左，话在右。楼与楼之间只有一根线，不做成卡。 */
.case__floor {
  display: grid;
  grid-template-columns: 46px minmax(0, 1fr);
  gap: var(--s3);
  padding: var(--s3) 0 var(--s2);
}

.case__floor + .case__floor {
  border-top: 1px solid var(--line);
}

.case__floor-side {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--s1);
  min-width: 0;
}

.case__floor-name {
  font-size: var(--fs-label);
  line-height: 1.35;
  color: var(--ink-muted);
  text-align: center;
  overflow-wrap: anywhere;
}

.case__floor-body {
  min-width: 0;
}

.case__floor-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s2);
  flex-wrap: wrap;
}

.case__floor-meta {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.case__floor-tags {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  flex-wrap: wrap;
}

.case__floor-text {
  margin: var(--s2) 0 0;
  font-size: var(--fs-body);
  line-height: 1.7;
  color: var(--ink);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.case__floor-foot {
  margin-top: var(--s2);
  display: flex;
  align-items: center;
  gap: var(--s2);
}

.case__actions {
  margin-top: var(--s3);
  display: flex;
  justify-content: flex-end;
  gap: var(--s2);
}

.case__row {
  margin-top: var(--s2);
  display: flex;
  align-items: center;
  gap: var(--s2);
  flex-wrap: wrap;
}

/* 三个数一排：分隔只用一根线，不做 KPI 卡 */
.case__counts {
  margin-top: var(--s3);
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2) var(--s4);
}

.case__counts > * + * {
  padding-left: var(--s4);
  border-left: 1px solid var(--line);
}

/* 回执一行一个人：行间一根线 */
.case__receipts {
  margin-top: var(--s3);
}

.case__receipt {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--s1) var(--s2);
  padding: var(--s2) 0;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.case__receipt + .case__receipt {
  border-top: 1px solid var(--line);
}

.case__receipt-user {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: var(--fw-medium);
  color: var(--ink);
}

.case__receipt-time {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}

.case__receipt-note {
  grid-column: 1 / -1;
  color: var(--ink-muted);
  overflow-wrap: anywhere;
}

.case__forms {
  margin-top: var(--s3);
}

.case__form + .case__form {
  margin-top: var(--s3);
  padding-top: var(--s3);
  border-top: 1px solid var(--line);
}

.case__form-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--s2);
}

.case__form-ident {
  min-width: 0;
}

.case__form-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--ink);
}

.case__form-meta {
  margin-top: 2px;
  font-family: var(--mono);
  font-size: var(--fs-label);
  color: var(--ink-muted);
  overflow-wrap: anywhere;
}

.case__empty {
  margin: 0;
  padding: var(--s4);
  font-size: var(--fs-body);
  color: var(--ink-muted);
}

/* 骨架：与上面同一套栏与间距，只是把字换成砖 */
.case__sk-lines {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
  min-width: 0;
}

.case__sk-editor {
  margin-top: var(--s3);
  padding-top: var(--s3);
  border-top: 1px solid var(--line);
}

.case__sk-facts {
  margin-top: var(--s2);
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--s3) var(--s3);
}

.case__sk-fact {
  display: flex;
  flex-direction: column;
  gap: var(--s1);
  min-width: 0;
}

.case__sk-form {
  margin-top: var(--s3);
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

@media (max-width: 1100px) {
  .case__cols,
  .case--wide .case__cols {
    grid-template-columns: 1fr;
  }

  .case__aside {
    border-left: 0;
    border-top: 1px solid var(--line);
  }
}
</style>
