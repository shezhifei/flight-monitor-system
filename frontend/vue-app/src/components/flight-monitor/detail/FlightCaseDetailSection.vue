<script setup lang="ts">
import { computed, inject, onMounted, onUnmounted, watch } from 'vue';
import { flightBusinessCaseKey } from '../../../composables/useFlightBusinessCases';
import DynamicWorkflowForm from '../DynamicWorkflowForm.vue';
import MentionInput from '../MentionInput.vue';
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
  getCaseReceiptSummaryText,
  getCaseStatusClass,
  getCaseStatusLabel,
  getReceiptItemAccountName,
  getReceiptItemStatusLabel,
  getReceiptSeverityLabel,
  getWorkflowFormMetaText,
  getWorkflowFormStatusLabel,
  getWorkflowProjectionDisplayData,
  getWorkflowProjectionMetaText,
  getWorkflowSubmissionDisplayData,
  hasAppendAcknowledged,
} from './businessCaseHelpers';

const ctx = inject(flightBusinessCaseKey)!;

const isOpen = computed(() => Boolean(ctx.activeCaseId.value));
const hasWorkflowColumn = computed(() => ctx.activeCaseHasWorkflowPanel.value);
/** 仅在尚无详情数据时展示骨架（有旧数据时保留内容，避免整页闪成「加载中」框） */
const showSkeleton = computed(
  () => Boolean(ctx.caseDetailLoading.value) && !ctx.activeCaseData.value,
);

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && isOpen.value) {
    event.preventDefault();
    ctx.closeCaseDetail();
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown);
});

// Prevent body scroll while modal is open (matches legacy fixed overlay UX).
watch(isOpen, (open) => {
  if (typeof document === 'undefined') return;
  document.body.style.overflow = open ? 'hidden' : '';
}, { immediate: true });

onUnmounted(() => {
  if (typeof document !== 'undefined') {
    document.body.style.overflow = '';
  }
});
</script>

<template>
  <Teleport v-if="isOpen" to="body">
    <div
      id="businessCaseDetailModal"
      class="case-detail-modal-host"
      role="presentation"
    >
      <div
        class="case-detail-modal-backdrop"
        aria-hidden="true"
        @click="ctx.closeCaseDetail"
      />

      <div
        class="case-detail-modal"
        :class="{ 'has-workflow': hasWorkflowColumn }"
        role="dialog"
        aria-modal="true"
        aria-labelledby="case-detail-modal-title"
        @click.stop
      >
        <div class="case-drawer-header">
          <div>
            <div id="case-detail-modal-title" class="case-drawer-title">
              业务事项详情
            </div>
            <div v-if="ctx.activeCaseData.value" class="case-drawer-meta">
              事项类型：{{ getCaseDisplayName(ctx.activeCaseData.value) }}
              | 状态：{{ getCaseStatusLabel(ctx.activeCaseData.value.status, ctx.caseStatusOptions.value) }}
              | {{ ctx.activeCaseVisibility.value.scopeLabel }}
            </div>
            <div v-else-if="showSkeleton" class="case-drawer-meta case-drawer-meta-skeleton" aria-hidden="true">
              <span class="sk sk-line sk-w-48" />
            </div>
          </div>
          <button
            type="button"
            class="case-drawer-close"
            aria-label="关闭"
            @click="ctx.closeCaseDetail"
          >
            ×
          </button>
        </div>

        <div class="case-detail-modal-body">
          <!-- 与最终布局同构的骨架屏，避免「加载中…」白框闪一下 -->
          <div
            v-if="showSkeleton"
            class="case-detail-shell case-detail-skeleton"
            aria-busy="true"
            aria-label="业务事项详情加载中"
          >
            <div class="case-main-column">
              <section class="case-thread-card">
                <div class="case-thread-header">
                  <span class="sk sk-line sk-w-24 sk-h-title" />
                  <span class="sk sk-line sk-w-32" />
                </div>
                <article class="case-floor case-floor-root">
                  <div class="case-floor-side">
                    <div class="sk sk-avatar" />
                    <span class="sk sk-line sk-w-full" />
                  </div>
                  <div class="case-floor-body case-floor-body-sk">
                    <div class="case-floor-head">
                      <span class="sk sk-line sk-w-40" />
                      <div class="case-floor-tags">
                        <span class="sk sk-pill" />
                        <span class="sk sk-pill" />
                      </div>
                    </div>
                    <div class="sk-lines">
                      <span class="sk sk-line sk-w-full" />
                      <span class="sk sk-line sk-w-full" />
                      <span class="sk sk-line sk-w-72" />
                    </div>
                    <div class="case-floor-foot">
                      <span class="sk sk-line sk-w-28" />
                      <span class="sk sk-line sk-w-36" />
                    </div>
                  </div>
                </article>
                <div class="case-reply-stream case-reply-stream-sk">
                  <article class="case-floor case-floor-reply">
                    <div class="case-floor-side">
                      <div class="sk sk-avatar sk-avatar-sm" />
                    </div>
                    <div class="case-floor-body case-floor-body-sk">
                      <span class="sk sk-line sk-w-32" />
                      <div class="sk-lines">
                        <span class="sk sk-line sk-w-full" />
                        <span class="sk sk-line sk-w-56" />
                      </div>
                    </div>
                  </article>
                </div>
              </section>
              <section class="case-reply-editor-card">
                <span class="sk sk-line sk-w-24 sk-h-title" />
                <div class="sk sk-block sk-editor" />
                <div class="case-reply-editor-actions case-reply-editor-actions-sk">
                  <span class="sk sk-btn" />
                  <span class="sk sk-btn sk-btn-primary" />
                </div>
              </section>
            </div>
            <aside class="case-side-column">
              <section class="case-side-card">
                <span class="sk sk-line sk-w-28 sk-h-title" />
                <div class="case-summary-grid">
                  <div v-for="n in 4" :key="`sum-${n}`" class="case-summary-item case-summary-item-sk">
                    <span class="sk sk-line sk-w-40" />
                    <span class="sk sk-line sk-w-56 sk-h-value" />
                  </div>
                </div>
                <div class="case-status-panel case-status-panel-sk">
                  <span class="sk sk-line sk-w-24" />
                  <div class="case-status-editor">
                    <span class="sk sk-select" />
                    <span class="sk sk-btn sk-btn-primary" />
                  </div>
                </div>
              </section>
              <section class="case-side-card">
                <span class="sk sk-line sk-w-28 sk-h-title" />
                <div class="receipt-summary-strip">
                  <div v-for="n in 3" :key="`rc-${n}`" class="receipt-summary-chip case-summary-item-sk">
                    <span class="sk sk-line sk-w-24 sk-h-value" />
                    <span class="sk sk-line sk-w-40" />
                  </div>
                </div>
              </section>
            </aside>
          </div>

          <template v-else-if="ctx.activeCaseData.value">
            <div class="case-detail-shell" :class="{ 'has-workflow': hasWorkflowColumn }">
              <div class="case-main-column">
                <section class="case-thread-card">
                  <div class="case-thread-header">
                    <div>
                      <div class="case-thread-title">
                        事项记录
                      </div>
                    </div>
                    <div class="case-thread-stats">
                      <span>共 {{ ctx.activeCaseThreadTotal.value }} 条记录</span>
                      <span v-if="ctx.activeCaseAppendEntries.value.length > 0">追加 {{ ctx.activeCaseAppendEntries.value.length }} 次</span>
                      <span v-else>暂无回复</span>
                    </div>
                  </div>

                  <article class="case-floor case-floor-root">
                    <div class="case-floor-side">
                      <div class="case-floor-avatar root">
                        {{ getCaseAuthorBadge(ctx.activeCaseData.value.created_by) }}
                      </div>
                      <div class="case-floor-index">
                        {{ ctx.activeCaseData.value.created_by || '系统' }}
                      </div>
                    </div>
                    <div class="case-floor-body">
                      <div class="case-floor-head">
                        <div class="case-floor-author-group">
                          <div class="case-floor-meta">
                            发布于 {{ formatCaseTime(ctx.activeCaseData.value.created_at) }}
                            <span v-if="ctx.activeCaseData.value.finished_at"> · 完成于 {{ formatCaseTime(ctx.activeCaseData.value.finished_at) }}</span>
                          </div>
                        </div>
                        <div class="case-floor-tags">
                          <span class="case-floor-pill scope">{{ ctx.activeCaseVisibility.value.scopeLabel }}</span>
                          <span
                            class="case-floor-pill status"
                            :class="getCaseStatusClass(ctx.activeCaseData.value.status, ctx.caseStatusOptions.value)"
                          >
                            {{ getCaseStatusLabel(ctx.activeCaseData.value.status, ctx.caseStatusOptions.value) }}
                          </span>
                        </div>
                      </div>

                      <div class="case-floor-content">
                        {{ ctx.activeCaseData.value.description || '当前事项未填写描述。' }}
                      </div>

                      <div class="case-floor-foot">
                        <span v-if="ctx.activeCaseVisibility.value.departmentName">归属部门：{{ ctx.activeCaseVisibility.value.departmentName }}</span>
                        <span v-if="getBoundCaseFlightLabel(ctx.activeCaseData.value.context)">绑定航班：{{ getBoundCaseFlightLabel(ctx.activeCaseData.value.context) }}</span>
                        <span>事项类型：{{ getCaseDisplayName(ctx.activeCaseData.value) }}</span>
                      </div>
                    </div>
                  </article>

                  <div v-if="ctx.activeCaseAppendEntries.value.length > 0" class="case-reply-stream">
                    <article
                      v-for="entry in ctx.activeCaseAppendEntries.value"
                      :key="entry.append_id"
                      class="case-floor case-floor-reply"
                    >
                      <div class="case-floor-side">
                        <div class="case-floor-avatar">
                          {{ getAppendAuthorBadge(entry) }}
                        </div>
                        <div class="case-floor-index">
                          {{ getAppendDisplayName(entry) }}
                        </div>
                      </div>
                      <div class="case-floor-body">
                        <div class="case-floor-head">
                          <div class="case-floor-author-group">
                            <div class="case-floor-meta">
                              {{ formatCaseTime(entry.appended_at) }} · {{ entry.submitted_by }}
                            </div>
                          </div>
                          <div class="case-floor-tags">
                            <span v-if="getAppendMentionCount(entry) > 0" class="case-floor-pill mention">
                              @{{ getAppendMentionCount(entry) }} 人
                            </span>
                            <span
                              v-if="getAppendMentionCount(entry) > 0"
                              class="case-floor-pill ack"
                            >
                              已确认 {{ getAppendAcknowledgedCount(entry) }}/{{ getAppendMentionCount(entry) }}
                            </span>
                          </div>
                        </div>

                        <div class="case-floor-content">
                          {{ entry.content }}
                        </div>

                        <div
                          v-if="getAppendMentionCount(entry) > 0"
                          class="case-floor-foot"
                        >
                          <span>提醒已发送给相关人员</span>
                          <template v-if="entry.metadata.mention_user_ids?.includes(ctx.getCurrentUserId())">
                            <button
                              v-if="!hasAppendAcknowledged(entry, ctx.getCurrentUserId())"
                              type="button"
                              class="ack-button"
                              @click.stop="ctx.acknowledgeAppend(entry)"
                            >
                              ✓ 确认收到
                            </button>
                            <span v-else class="ack-done">
                              ✓ 已确认 {{ getAppendAcknowledgedTime(entry, ctx.getCurrentUserId()) }}
                            </span>
                          </template>
                        </div>
                      </div>
                    </article>
                  </div>

                  <div v-else class="case-thread-empty">
                    暂无回复。
                  </div>
                </section>

                <section class="case-reply-editor-card">
                  <div class="case-reply-editor-header">
                    <div class="case-thread-title">
                      追加回复
                    </div>
                  </div>
                  <MentionInput
                    v-model="ctx.appendContent.value"
                    :stakeholders="ctx.mentionCandidates.value"
                    @update:mention-ids="ctx.appendMentionIds.value = $event"
                  />
                  <div v-if="ctx.appendMentionIds.value.length > 0" class="mention-summary">
                    将通知 {{ ctx.appendMentionIds.value.length }} 人
                  </div>
                  <div class="case-reply-editor-actions">
                    <button type="button" class="btn btn-outline" @click="ctx.closeCaseDetail">
                      关闭
                    </button>
                    <button
                      type="button"
                      class="btn btn-primary"
                      :disabled="!ctx.appendContent.value.trim() || ctx.appendSubmitting.value"
                      @click="ctx.submitAppend"
                    >
                      {{ ctx.appendSubmitting.value ? '提交中...' : '发布回复' }}
                    </button>
                  </div>
                </section>
              </div>

              <aside class="case-side-column">
                <section class="case-side-card">
                  <div class="case-side-card-title">
                    事项概览
                  </div>
                  <div class="case-summary-grid">
                    <div class="case-summary-item">
                      <span class="case-summary-label">状态</span>
                      <span class="case-summary-value">{{ getCaseStatusLabel(ctx.activeCaseData.value.status, ctx.caseStatusOptions.value) }}</span>
                    </div>
                    <div class="case-summary-item">
                      <span class="case-summary-label">范围</span>
                      <span class="case-summary-value">{{ ctx.activeCaseVisibility.value.scopeLabel }}</span>
                    </div>
                    <div class="case-summary-item">
                      <span class="case-summary-label">创建人</span>
                      <span class="case-summary-value">{{ ctx.activeCaseData.value.created_by || '-' }}</span>
                    </div>
                    <div class="case-summary-item">
                      <span class="case-summary-label">创建时间</span>
                      <span class="case-summary-value">{{ formatCaseTime(ctx.activeCaseData.value.created_at) }}</span>
                    </div>
                    <div v-if="ctx.activeCaseData.value.finished_at" class="case-summary-item">
                      <span class="case-summary-label">完成时间</span>
                      <span class="case-summary-value">{{ formatCaseTime(ctx.activeCaseData.value.finished_at) }}</span>
                    </div>
                    <div v-if="ctx.activeCaseVisibility.value.departmentName" class="case-summary-item">
                      <span class="case-summary-label">归属部门</span>
                      <span class="case-summary-value">{{ ctx.activeCaseVisibility.value.departmentName }}</span>
                    </div>
                  </div>

                  <div v-if="ctx.canAttemptActiveCaseStatusEdit.value" class="case-status-panel">
                    <div class="case-side-card-subtitle">
                      状态流转
                    </div>
                    <div class="case-status-editor">
                      <select
                        v-model="ctx.caseStatusDraft.value"
                        class="case-status-select"
                        :disabled="ctx.caseStatusSaving.value"
                      >
                        <option
                          v-for="option in ctx.activeCaseStatusOptions.value"
                          :key="option.value"
                          :value="option.value"
                        >
                          {{ option.label }}
                        </option>
                      </select>
                      <button
                        type="button"
                        class="btn btn-primary"
                        :disabled="ctx.caseStatusSaving.value || ctx.caseStatusDraft.value === ctx.activeCaseStatusValue.value"
                        @click="ctx.submitCaseStatusUpdate"
                      >
                        {{ ctx.caseStatusSaving.value ? '更新中...' : '更新状态' }}
                      </button>
                    </div>
                    <div v-if="ctx.showCaseStatusPermissionHint.value" class="case-status-hint">
                      当前账号可尝试更新，最终以后端权限校验为准。
                    </div>
                    <div v-if="ctx.caseStatusMetadataError.value" class="case-status-hint">
                      {{ ctx.caseStatusMetadataError.value }}
                    </div>
                  </div>
                </section>

                <section v-if="ctx.activeCaseReceipt.value" class="case-side-card">
                  <div class="case-side-card-title case-side-card-title-row">
                    <span>通知回执</span>
                    <span class="receipt-severity-chip">级别 {{ getReceiptSeverityLabel(ctx.activeCaseReceipt.value) }}</span>
                  </div>
                  <div class="receipt-summary-strip">
                    <div class="receipt-summary-chip">
                      <span class="receipt-summary-number">{{ ctx.activeCaseReceipt.value.summary.acknowledged_count }}</span>
                      <span class="receipt-summary-label">已回执</span>
                    </div>
                    <div class="receipt-summary-chip">
                      <span class="receipt-summary-number">{{ ctx.activeCaseReceipt.value.summary.pending_count }}</span>
                      <span class="receipt-summary-label">待回执</span>
                    </div>
                    <div class="receipt-summary-chip">
                      <span class="receipt-summary-number">{{ ctx.activeCaseReceipt.value.summary.rejected_count }}</span>
                      <span class="receipt-summary-label">拒绝</span>
                    </div>
                  </div>
                  <div class="case-side-note">
                    {{ getCaseReceiptSummaryText(ctx.activeCaseData.value) }}
                  </div>
                  <div
                    v-if="(ctx.activeCaseReceipt.value?.items || []).length > 0"
                    class="receipt-items-list"
                  >
                    <div
                      v-for="item in ctx.activeCaseReceipt.value?.items || []"
                      :key="`${item.user_id}-${item.updated_at || item.ack_at || ''}`"
                      class="receipt-item-row"
                    >
                      <span class="receipt-item-user">{{ getReceiptItemAccountName(item) }}</span>
                      <span class="receipt-item-status">{{ getReceiptItemStatusLabel(item) }}</span>
                      <span class="receipt-item-time">{{ item.ack_at ? formatCaseTime(item.ack_at) : '--' }}</span>
                      <span class="receipt-item-note">{{ item.ack_note || '--' }}</span>
                    </div>
                  </div>
                </section>
              </aside>

              <aside v-if="hasWorkflowColumn" class="case-workflow-column">
                <section class="case-side-card">
                  <div class="case-side-card-title">
                    流程表单
                  </div>

                  <div v-if="ctx.workflowFormsLoading.value" class="workflow-forms-empty">
                    加载中...
                  </div>
                  <div
                    v-else-if="ctx.workflowFormsError.value && ((ctx.activeCaseWorkflowForms.value?.forms || []).length > 0 || ctx.activeCaseWorkflowProjectionEntries.value.length > 0)"
                    class="workflow-forms-error"
                  >
                    {{ ctx.workflowFormsError.value }}
                  </div>

                  <template v-else-if="(ctx.activeCaseWorkflowForms.value?.forms || []).length > 0">
                    <div class="workflow-form-list">
                      <div
                        v-for="form in ctx.activeCaseWorkflowForms.value?.forms || []"
                        :key="`${form.task_id}-${form.form_code}`"
                        class="workflow-form-card"
                      >
                        <div class="workflow-form-card-header">
                          <div>
                            <div class="workflow-form-card-title">
                              {{ form.name }}
                            </div>
                            <div class="workflow-form-card-meta">
                              {{ form.task_name }} · {{ form.form_code }} · v{{ form.form_version }}
                            </div>
                          </div>
                          <span
                            class="workflow-form-card-status"
                            :class="{ editable: form.can_submit, readonly: !form.can_submit }"
                          >
                            {{ getWorkflowFormStatusLabel(form, ctx.activeCaseData.value) }}
                          </span>
                        </div>

                        <div v-if="getWorkflowFormMetaText(form, ctx.activeCaseData.value)" class="workflow-form-card-note">
                          {{ getWorkflowFormMetaText(form, ctx.activeCaseData.value) }}
                        </div>
                        <div
                          v-if="!form.can_submit && form.readonly_reason && !getWorkflowFormMetaText(form, ctx.activeCaseData.value)"
                          class="workflow-form-card-note"
                        >
                          {{ form.readonly_reason }}
                        </div>

                        <DynamicWorkflowForm
                          :schema="form.schema"
                          :ui-schema="form.ui_schema"
                          :initial-value="getWorkflowSubmissionDisplayData(form, ctx.activeCaseData.value)"
                          :readonly="!form.can_submit"
                          :submitting="ctx.workflowFormSubmittingCode.value === form.form_code"
                          empty-text="暂无表单字段"
                          @submit="ctx.submitWorkflowForm(form, $event)"
                        />
                      </div>
                    </div>
                  </template>

                  <template v-else-if="ctx.activeCaseWorkflowProjectionEntries.value.length > 0">
                    <div class="workflow-form-list">
                      <div
                        v-for="projection in ctx.activeCaseWorkflowProjectionEntries.value"
                        :key="projection.submission_id"
                        class="workflow-form-card"
                      >
                        <div class="workflow-form-card-header">
                          <div>
                            <div class="workflow-form-card-title">
                              {{ projection.form_code }}
                            </div>
                            <div class="workflow-form-card-meta">
                              {{ projection.task_definition_key }} · v{{ projection.form_version }}
                            </div>
                          </div>
                          <span class="workflow-form-card-status readonly">已提交</span>
                        </div>

                        <div class="workflow-form-card-note">
                          {{ getWorkflowProjectionMetaText(projection) }}
                        </div>

                        <DynamicWorkflowForm
                          :initial-value="getWorkflowProjectionDisplayData(projection)"
                          readonly
                          empty-text="暂无可回显的表单内容"
                        />
                      </div>
                    </div>
                  </template>
                </section>
              </aside>
            </div>
          </template>

          <div v-else class="case-drawer-loading">
            未能加载业务事项详情
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* Host mirrors legacy #businessCaseDetailModal: body-level fixed overlay.
   Colors use theme tokens so light/dark both work. */
.case-detail-modal-host {
  position: fixed;
  inset: 0;
  z-index: 99998;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px 16px;
  box-sizing: border-box;
  pointer-events: none;
}

.case-detail-modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 42, 0.55);
  z-index: 99998;
  pointer-events: auto;
}

:global([data-theme="dark"]) .case-detail-modal-backdrop {
  background: rgba(0, 0, 0, 0.62);
}

.case-detail-modal {
  position: relative;
  z-index: 99999;
  width: min(1180px, calc(100vw - 32px));
  max-height: calc(100vh - 48px);
  overflow: auto;
  background: var(--admin-card-bg, var(--bg-card, #fff));
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 20px;
  box-shadow: var(--ws-shadow-md, 0 24px 80px rgba(15, 23, 42, 0.28));
  color: var(--admin-text, var(--text-primary));
  display: flex;
  flex-direction: column;
  pointer-events: auto;
}

.case-detail-modal.has-workflow {
  width: min(1520px, calc(100vw - 32px));
}

.case-drawer-header {
  padding: 20px 24px;
  border-bottom: 1px solid var(--admin-border, var(--border-light));
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
  flex-shrink: 0;
  background: var(--admin-card-bg, var(--bg-card, #fff));
  position: sticky;
  top: 0;
  z-index: 1;
}

.case-drawer-title {
  font-size: 18px;
  font-weight: 700;
  color: var(--admin-text, var(--text-primary));
}

.case-drawer-meta {
  margin-top: 6px;
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.case-drawer-close {
  border: none;
  background: transparent;
  font-size: 24px;
  line-height: 1;
  color: var(--admin-text-muted, var(--text-tertiary));
  cursor: pointer;
  padding: 0 4px;
  flex-shrink: 0;
}

.case-drawer-close:hover {
  color: var(--admin-text, var(--text-primary));
}

.case-detail-modal-body {
  padding: 20px 24px;
  background: var(--bg-sidebar, var(--bg-page, #f8fafc));
  flex: 1;
  min-height: 0;
}

/* —— Skeleton (matches final 2-col layout) —— */
.case-drawer-meta-skeleton {
  display: flex;
  align-items: center;
  margin-top: 8px;
}

.case-detail-skeleton {
  pointer-events: none;
  user-select: none;
}

.case-floor-body-sk {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.sk-lines {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 4px;
}

.case-reply-stream-sk {
  margin-top: 14px;
  padding-top: 14px;
}

.case-reply-editor-actions-sk {
  margin-top: 12px;
}

.case-summary-item-sk {
  gap: 8px;
}

.case-status-panel-sk {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.sk {
  display: block;
  border-radius: 6px;
  background: linear-gradient(
    90deg,
    var(--admin-border, rgba(15, 23, 42, 0.08)) 0%,
    var(--bg-sidebar, rgba(148, 163, 184, 0.18)) 45%,
    var(--admin-border, rgba(15, 23, 42, 0.08)) 90%
  );
  background-size: 200% 100%;
  animation: case-sk-shimmer 1.25s ease-in-out infinite;
}

:global([data-theme="dark"]) .sk {
  background: linear-gradient(
    90deg,
    rgba(100, 140, 190, 0.1) 0%,
    rgba(140, 180, 230, 0.22) 45%,
    rgba(100, 140, 190, 0.1) 90%
  );
  background-size: 200% 100%;
}

@keyframes case-sk-shimmer {
  0% { background-position: 100% 0; }
  100% { background-position: -100% 0; }
}

.sk-line {
  height: 12px;
  width: 100%;
}

.sk-h-title { height: 16px; border-radius: 8px; }
.sk-h-value { height: 14px; }

.sk-w-full { width: 100%; }
.sk-w-72 { width: 72%; }
.sk-w-56 { width: 56%; }
.sk-w-48 { width: 48%; min-width: 160px; }
.sk-w-40 { width: 40%; }
.sk-w-36 { width: 36%; }
.sk-w-32 { width: 32%; }
.sk-w-28 { width: 28%; min-width: 72px; }
.sk-w-24 { width: 24%; min-width: 64px; }

.sk-avatar {
  width: 42px;
  height: 42px;
  border-radius: 50%;
  flex-shrink: 0;
}

.sk-avatar-sm {
  width: 36px;
  height: 36px;
}

.sk-pill {
  width: 52px;
  height: 22px;
  border-radius: 999px;
}

.sk-block {
  width: 100%;
  border-radius: 12px;
}

.sk-editor {
  height: 96px;
  margin-top: 12px;
}

.sk-btn {
  width: 72px;
  height: 34px;
  border-radius: 999px;
}

.sk-btn-primary {
  width: 96px;
}

.sk-select {
  width: 180px;
  height: 36px;
  border-radius: 10px;
}

.case-detail-shell {
  display: grid;
  grid-template-columns: minmax(0, 1.5fr) minmax(320px, 0.92fr);
  gap: 20px;
  align-items: start;
}

.case-detail-shell.has-workflow {
  grid-template-columns: minmax(0, 1.2fr) minmax(280px, 0.7fr) minmax(320px, 0.9fr);
}

.case-main-column,
.case-side-column,
.case-workflow-column {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.case-thread-card,
.case-reply-editor-card,
.case-side-card {
  padding: 18px;
  background: var(--admin-card-bg, var(--bg-card, #fff));
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 16px;
  box-shadow: var(--ws-shadow-sm, 0 10px 24px rgba(15, 23, 42, 0.04));
}

.case-thread-header,
.case-reply-editor-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
  margin-bottom: 14px;
}

.case-thread-title {
  font-size: 15px;
  font-weight: 700;
  color: var(--admin-text, var(--text-primary));
}

.case-thread-stats {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: flex-end;
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.case-floor {
  display: grid;
  grid-template-columns: 56px minmax(0, 1fr);
  gap: 14px;
}

.case-floor-side {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}

.case-floor-avatar {
  width: 42px;
  height: 42px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #64748b 0%, #475569 100%);
  color: #fff;
  font-size: 16px;
  font-weight: 700;
  box-shadow: 0 10px 18px rgba(71, 85, 105, 0.18);
}

.case-floor-avatar.root {
  background: linear-gradient(135deg, #f59e0b 0%, #f97316 100%);
  box-shadow: 0 10px 18px rgba(249, 115, 22, 0.18);
}

.case-floor-index {
  font-size: 11px;
  color: var(--admin-text-muted, var(--text-tertiary));
  font-weight: 600;
  max-width: 56px;
  overflow-wrap: anywhere;
  text-align: center;
  line-height: 1.35;
}

.case-floor-body {
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 16px;
  background: var(--system-blue-subtle, rgba(0, 122, 255, 0.08));
  padding: 14px 16px;
  min-width: 0;
}

.case-floor-reply .case-floor-body {
  background: var(--bg-sidebar, var(--ws-surface-muted, rgba(248, 250, 252, 0.9)));
}

.case-floor-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
}

.case-floor-meta {
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.case-floor-tags {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.case-floor-pill {
  display: inline-flex;
  align-items: center;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  background: var(--bg-sidebar, rgba(15, 23, 42, 0.08));
  color: var(--admin-text-subtle, #475569);
}

.case-floor-pill.scope {
  background: rgba(14, 165, 233, 0.14);
  color: var(--system-blue, #0369a1);
}

.case-floor-pill.mention {
  background: rgba(99, 102, 241, 0.14);
  color: #818cf8;
}

.case-floor-pill.ack {
  background: var(--success-bg-subtle, rgba(34, 197, 94, 0.12));
  color: var(--system-green, #15803d);
}

.case-floor-content {
  margin-top: 12px;
  font-size: 14px;
  line-height: 1.8;
  color: var(--admin-text, var(--text-primary));
  white-space: pre-wrap;
  word-break: break-word;
}

.case-floor-foot {
  margin-top: 12px;
  display: flex;
  flex-wrap: wrap;
  gap: 10px 14px;
  font-size: 12px;
  line-height: 1.6;
  color: var(--admin-text-muted, var(--text-tertiary));
  align-items: center;
}

.case-reply-stream {
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid var(--admin-border, var(--border-light));
  display: grid;
  gap: 14px;
}

.case-thread-empty {
  margin-top: 12px;
  font-size: 13px;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.mention-summary {
  margin-top: 6px;
  font-size: 11px;
  color: var(--system-blue, #007aff);
  min-height: 16px;
}

.case-reply-editor-actions {
  margin-top: 12px;
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.ack-button {
  border: 1px solid rgba(34, 197, 94, 0.35);
  background: rgba(34, 197, 94, 0.1);
  color: #15803d;
  border-radius: 999px;
  padding: 4px 10px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.ack-done {
  color: #15803d;
  font-weight: 600;
}

.case-side-card-title {
  font-size: 15px;
  font-weight: 700;
  color: var(--admin-text, var(--text-primary));
}

.case-side-card-title-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
}

.case-side-card-subtitle {
  margin-bottom: 10px;
  font-size: 12px;
  font-weight: 700;
  color: var(--admin-text, var(--text-primary));
}

.case-summary-grid {
  margin-top: 12px;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.case-summary-item {
  padding: 12px;
  border-radius: 12px;
  background: var(--bg-sidebar, var(--ws-surface-muted));
  border: 1px solid var(--admin-border, var(--border-light));
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.case-summary-label {
  font-size: 11px;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.case-summary-value {
  font-size: 13px;
  font-weight: 600;
  color: var(--admin-text, var(--text-primary));
  line-height: 1.5;
  word-break: break-word;
}

.case-status-panel {
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid var(--admin-border, var(--border-light));
}

.case-status-editor {
  display: flex;
  gap: 10px;
  align-items: center;
  flex-wrap: wrap;
}

.case-status-select {
  min-width: 180px;
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 10px;
  padding: 8px 10px;
  font-size: 13px;
  color: var(--admin-text, var(--text-primary));
  background: var(--admin-card-bg, var(--bg-card, #fff));
}

.case-status-hint {
  margin-top: 8px;
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.receipt-severity-chip {
  display: inline-flex;
  align-items: center;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  background: rgba(239, 68, 68, 0.1);
  color: #b91c1c;
}

.receipt-summary-strip {
  margin-top: 12px;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.receipt-summary-chip {
  padding: 10px 8px;
  border-radius: 12px;
  background: var(--bg-sidebar, var(--ws-surface-muted));
  border: 1px solid var(--admin-border, var(--border-light));
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.receipt-summary-number {
  font-size: 16px;
  font-weight: 700;
  color: var(--admin-text, var(--text-primary));
}

.receipt-summary-label {
  font-size: 11px;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.case-side-note {
  margin-top: 10px;
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-tertiary));
  line-height: 1.5;
}

.receipt-items-list {
  margin-top: 12px;
  display: grid;
  gap: 8px;
}

.receipt-item-row {
  display: grid;
  grid-template-columns: minmax(0, 1.1fr) auto auto minmax(0, 1fr);
  gap: 8px;
  align-items: center;
  padding: 8px 10px;
  border-radius: 10px;
  background: var(--bg-sidebar, var(--ws-surface-muted));
  border: 1px solid var(--admin-border, var(--border-light));
  font-size: 12px;
  color: var(--admin-text-subtle, var(--text-secondary));
}

.receipt-item-user {
  font-weight: 600;
  color: var(--admin-text, var(--text-primary));
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.receipt-item-note {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.workflow-forms-empty,
.workflow-forms-error {
  margin-top: 12px;
  padding: 12px 14px;
  border-radius: 12px;
  font-size: 13px;
}

.workflow-forms-empty {
  color: var(--admin-text-muted, var(--text-tertiary));
  background: var(--bg-sidebar, var(--ws-surface-muted));
  border: 1px solid var(--admin-border, var(--border-light));
}

.workflow-forms-error {
  color: var(--system-red, #b42318);
  background: var(--error-bg-subtle, rgba(254, 242, 242, 0.92));
  border: 1px solid var(--error-border-subtle, rgba(239, 68, 68, 0.18));
}

.workflow-form-list {
  margin-top: 12px;
  display: grid;
  gap: 12px;
}

.workflow-form-card {
  padding: 14px;
  border-radius: 12px;
  border: 1px solid var(--admin-border, var(--border-light));
  background: var(--admin-card-bg, var(--bg-card));
}

.workflow-form-card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 10px;
  margin-bottom: 10px;
}

.workflow-form-card-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--admin-text, var(--text-primary));
}

.workflow-form-card-meta {
  margin-top: 4px;
  font-size: 11px;
  color: var(--admin-text-muted, var(--text-tertiary));
}

.workflow-form-card-status {
  display: inline-flex;
  align-items: center;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  white-space: nowrap;
}

.workflow-form-card-status.editable {
  background: var(--success-bg-subtle, rgba(34, 197, 94, 0.12));
  color: var(--system-green, #15803d);
}

.workflow-form-card-status.readonly {
  background: var(--bg-sidebar, rgba(15, 23, 42, 0.08));
  color: var(--admin-text-subtle, #475569);
}

.workflow-form-card-note {
  margin-bottom: 10px;
  font-size: 12px;
  color: var(--admin-text-muted, var(--text-tertiary));
  line-height: 1.5;
}

@media (max-width: 1100px) {
  .case-detail-shell,
  .case-detail-shell.has-workflow {
    grid-template-columns: 1fr;
  }

  .case-detail-modal,
  .case-detail-modal.has-workflow {
    width: min(760px, calc(100vw - 32px));
  }
}
</style>
