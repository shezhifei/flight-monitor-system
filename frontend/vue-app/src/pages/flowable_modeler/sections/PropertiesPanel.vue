<script setup lang="ts">
import { ref, watch } from 'vue';
import type { FormTaskBindingConfig } from '../types';
import FormFieldDesigner from '../components/FormFieldDesigner.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import { useFlowableModeler } from '../composables/useFlowableModeler';

type PanelTab = 'config' | 'node' | 'ai';

const p = useFlowableModeler();
const sampleTemplateToken = '{{flight_no}}';
const activeTab = ref<PanelTab>('config');

/** 对齐 legacy：选中画布节点后自动切到「节点属性」 */
watch(
  () => p.selectedTaskId.value,
  (id) => {
    if (id) activeTab.value = 'node';
  },
);

function updateSelectedTaskName(event: Event) {
  p.updateSelectedTaskName((event.target as HTMLInputElement).value);
}

function updateSelectedTaskConfig(partial: Partial<FormTaskBindingConfig>) {
  p.updateSelectedTaskConfig(partial);
}

function parseRoles(raw: string): string[] {
  return raw.split(',').map((r) => r.trim()).filter(Boolean);
}

function insertNotifyToken(field: 'title' | 'bodyTemplate', token: string) {
  const rule = p.selectedNotificationRule.value;
  if (!rule) return;
  const current = String(rule[field] || '');
  const next = `${current}${current && !current.endsWith(' ') ? ' ' : ''}\${${token}}`;
  p.updateSelectedNotificationRule({ [field]: next });
}
</script>

<template>
  <aside class="editor-properties-panel">
    <!-- legacy: 简洁顶栏 -->
    <header class="panel-header">
      <span class="panel-header-title">
        {{ p.hasSelectedDiagram.value ? (p.diagramName.value || '属性面板') : '属性面板' }}
      </span>
      <UiPill
        v-if="p.hasSelectedDiagram.value"
        :tone="p.selectedFormTaskConfig.value || p.selectedNotificationRule.value ? 'act' : 'ok'"
      >
        {{ p.selectedFormTaskConfig.value || p.selectedNotificationRule.value ? '节点编辑' : '浏览中' }}
      </UiPill>
    </header>

    <!-- legacy: 三 Tab -->
    <div class="panel-tabs" role="tablist" aria-label="属性面板分区">
      <button
        type="button"
        class="panel-tab"
        role="tab"
        :aria-selected="activeTab === 'config'"
        @click="activeTab = 'config'"
      >
        <SvgIcon src="/frontend/icons/settings.svg" :size="14" />
        流程配置
      </button>
      <button
        type="button"
        class="panel-tab"
        role="tab"
        :aria-selected="activeTab === 'node'"
        @click="activeTab = 'node'"
      >
        <SvgIcon src="/frontend/icons/edit.svg" :size="14" />
        节点属性
      </button>
      <button
        type="button"
        class="panel-tab"
        role="tab"
        :aria-selected="activeTab === 'ai'"
        @click="activeTab = 'ai'"
      >
        <SvgIcon src="/frontend/icons/ai.svg" :size="14" />
        AI 工具
      </button>
    </div>

    <!-- ========== Tab 1: 流程配置 ========== -->
    <div v-show="activeTab === 'config'" class="panel-tab-content" role="tabpanel">
      <div class="panel-field">
        <div class="panel-section-title is-plain">
          业务事项描述
          <span class="panel-section-hint">(AI 助手使用)</span>
        </div>
        <textarea
          class="panel-textarea"
          rows="3"
          :value="p.caseDescription.value"
          placeholder="描述该业务事项发生的情况和背景，方便 AI 理解…"
          @input="p.caseDescription.value = ($event.target as HTMLTextAreaElement).value"
        />
      </div>

      <div class="panel-section-title">
        配置摘要
      </div>
      <div class="panel-card">
        <div class="summary-row">
          <span class="summary-key">编码</span>
          <code class="summary-val mono">{{ p.diagramCode.value || '—' }}</code>
        </div>
        <div class="summary-row">
          <span class="summary-key">作用域</span>
          <span class="summary-val">{{ p.activeScopeLabel.value }}</span>
        </div>
        <div class="summary-row">
          <span class="summary-key">Tenant</span>
          <code class="summary-val mono">{{ p.activeTenantLabel.value }}</code>
        </div>
        <div class="summary-row">
          <span class="summary-key">表单任务</span>
          <span class="summary-val accent">{{ p.persistedFormTaskCount.value }}</span>
        </div>
        <div class="summary-row is-last">
          <span class="summary-key">状态</span>
          <span class="summary-val">{{ p.selectedFormTaskConfig.value || p.selectedNotificationRule.value ? '节点编辑中' : '浏览模式' }}</span>
        </div>
      </div>

      <div class="panel-section-title">
        流程上下文变量
      </div>
      <div class="panel-card">
        <p class="panel-hint">
          可在描述模板中引用，例如 <code v-text="sampleTemplateToken" />
        </p>
        <div class="context-variable-list">
          <span
            v-for="variable in p.contextVariables.value"
            :key="variable.key"
            class="context-variable-chip"
          >
            <code>{{ '${' + variable.key + '}' }}</code>
            <span>{{ variable.label }}</span>
          </span>
        </div>
      </div>
    </div>

    <!-- ========== Tab 2: 节点属性 ========== -->
    <div v-show="activeTab === 'node'" class="panel-tab-content" role="tabpanel">
      <!-- 通知节点 -->
      <template v-if="p.selectedTaskId.value && p.selectedNodeType.value === 'notification' && p.selectedNotificationRule.value">
        <div class="panel-section-title">
          通知节点
        </div>
        <div class="panel-card">
          <div class="panel-card-title accent">
            当前选中 · <code>{{ p.selectedTaskId.value }}</code>
          </div>

          <label class="panel-field">
            <span class="panel-label">节点标题</span>
            <input
              type="text"
              class="panel-input"
              :value="p.selectedTaskName.value"
              maxlength="120"
              placeholder="发送调度通知"
              @input="updateSelectedTaskName($event)"
            >
          </label>

          <label class="panel-field">
            <span class="panel-label">通知标题</span>
            <input
              type="text"
              class="panel-input"
              :value="p.selectedNotificationRule.value.title"
              @input="p.updateSelectedNotificationRule({ title: ($event.target as HTMLInputElement).value })"
            >
            <div class="token-row">
              <button
                v-for="tok in ['flight_no', 'gate', 'trigger_reason']"
                :key="tok"
                type="button"
                class="token-btn"
                @click="insertNotifyToken('title', tok)"
              >
                + {{ tok }}
              </button>
            </div>
          </label>

          <label class="panel-field">
            <span class="panel-label">通知正文模板</span>
            <textarea
              rows="4"
              class="panel-textarea"
              :value="p.selectedNotificationRule.value.bodyTemplate"
              @input="p.updateSelectedNotificationRule({ bodyTemplate: ($event.target as HTMLTextAreaElement).value })"
            />
            <div class="token-row">
              <button
                v-for="tok in ['flight_no', 'gate', 'trigger_reason', 'extra_info']"
                :key="tok"
                type="button"
                class="token-btn"
                @click="insertNotifyToken('bodyTemplate', tok)"
              >
                + {{ tok }}
              </button>
            </div>
          </label>

          <div class="panel-grid-2">
            <label class="panel-field">
              <span class="panel-label">通知动作</span>
              <input
                type="text"
                class="panel-input mono"
                :value="p.selectedNotificationRule.value.action"
                @input="p.updateSelectedNotificationRule({ action: ($event.target as HTMLInputElement).value })"
              >
            </label>
            <label class="panel-field">
              <span class="panel-label">通知级别</span>
              <select class="panel-input" :value="p.selectedNotificationRule.value.severity" @change="p.updateSelectedNotificationRule({ severity: ($event.target as HTMLSelectElement).value as 'info' | 'warning' | 'critical' })">
                <option value="info">
                  info
                </option>
                <option value="warning">
                  warning
                </option>
                <option value="critical">
                  critical
                </option>
              </select>
            </label>
            <label class="panel-field">
              <span class="panel-label">是否要求回执</span>
              <select class="panel-input" :value="String(p.selectedNotificationRule.value.receiptRequired)" @change="p.updateSelectedNotificationRule({ receiptRequired: ($event.target as HTMLSelectElement).value === 'true' })">
                <option value="true">
                  需要
                </option>
                <option value="false">
                  不需要
                </option>
              </select>
            </label>
            <label class="panel-field">
              <span class="panel-label">拼接额外信息</span>
              <select class="panel-input" :value="String(p.selectedNotificationRule.value.appendExtraInfo)" @change="p.updateSelectedNotificationRule({ appendExtraInfo: ($event.target as HTMLSelectElement).value === 'true' })">
                <option value="true">
                  拼接
                </option>
                <option value="false">
                  不拼接
                </option>
              </select>
            </label>
          </div>

          <div class="panel-label block">
            通知角色
          </div>
          <div class="check-row">
            <label class="check">
              <input type="checkbox" :checked="p.selectedNotificationRule.value.roles.includes('dispatcher')" @change="p.toggleNotificationRole('dispatcher', ($event.target as HTMLInputElement).checked)">
              <span>dispatcher</span>
            </label>
            <label class="check">
              <input type="checkbox" :checked="p.selectedNotificationRule.value.roles.includes('supervisor')" @change="p.toggleNotificationRole('supervisor', ($event.target as HTMLInputElement).checked)">
              <span>supervisor</span>
            </label>
            <label class="check">
              <input type="checkbox" :checked="p.selectedNotificationRule.value.deduplicate" @change="p.updateSelectedNotificationRule({ deduplicate: ($event.target as HTMLInputElement).checked })">
              <span>去重收件人</span>
            </label>
          </div>

          <div class="panel-label block">
            通知科室
          </div>
          <div v-if="p.referenceDepartmentsLoading.value" class="panel-hint">
            正在加载科室主数据…
          </div>
          <div v-else-if="p.referenceDepartmentsError.value" class="field-error">
            {{ p.referenceDepartmentsError.value }}
          </div>
          <div v-else class="dept-grid">
            <label v-for="dep in p.referenceDepartments.value" :key="dep.id" class="check dept">
              <input
                type="checkbox"
                :checked="p.selectedNotificationRule.value.departmentIds.includes(dep.id)"
                @change="p.toggleNotificationDepartment(dep.id, ($event.target as HTMLInputElement).checked)"
              >
              <span :title="dep.name">{{ dep.name }}</span>
            </label>
            <div v-if="p.referenceDepartments.value.length === 0" class="panel-hint">
              暂无科室数据
            </div>
          </div>
          <div v-if="p.selectedNotificationRule.value.departmentIds.length === 0" class="field-error">
            至少选择一个通知科室，否则运行时无法解析收件人。
          </div>
        </div>
      </template>

      <!-- 等待回执 -->
      <template v-else-if="p.selectedTaskId.value && p.selectedNodeType.value === 'wait_receipts'">
        <div class="panel-section-title">
          等待回执节点
        </div>
        <div class="panel-card">
          <div class="panel-card-title accent">
            当前选中 · <code>{{ p.selectedTaskId.value }}</code>
          </div>
          <label class="panel-field">
            <span class="panel-label">节点标题</span>
            <input
              type="text"
              class="panel-input"
              :value="p.selectedTaskName.value"
              @input="updateSelectedTaskName($event)"
            >
          </label>
          <p class="panel-hint">
            约定 id 为 <code>wait_receipts</code>，用于回执汇聚。保存时会写入 BPMN。
          </p>
        </div>
      </template>

      <!-- 表单任务 -->
      <template v-else-if="p.selectedTaskId.value && p.selectedFormTaskConfig.value">
        <div class="panel-section-title">
          节点配置
        </div>
        <div class="panel-card">
          <div class="panel-card-title accent">
            正在编辑 · <code>{{ p.selectedTaskId.value }}</code>
          </div>

          <label class="panel-field">
            <span class="panel-label">节点标题</span>
            <input
              type="text"
              class="panel-input"
              :value="p.selectedTaskName.value"
              maxlength="120"
              placeholder="例如：填写处理表单"
              @input="updateSelectedTaskName($event)"
            >
          </label>

          <div class="panel-label block">
            绑定标识
          </div>
          <div class="panel-grid-2">
            <label class="panel-field">
              <span class="panel-label">模板代码 Template</span>
              <input
                type="text"
                class="panel-input mono"
                :value="p.selectedFormTaskConfig.value.templateCode"
                placeholder="form_template_xxx"
                @input="updateSelectedTaskConfig({ templateCode: ($event.target as HTMLInputElement).value })"
              >
            </label>
            <label class="panel-field">
              <span class="panel-label">表单代码 Form</span>
              <input
                type="text"
                class="panel-input mono"
                :value="p.selectedFormTaskConfig.value.formCode"
                placeholder="form_xxx"
                @input="updateSelectedTaskConfig({ formCode: ($event.target as HTMLInputElement).value })"
              >
            </label>
            <label class="panel-field">
              <span class="panel-label">版本 Version</span>
              <input
                type="number"
                min="1"
                class="panel-input"
                :value="p.selectedFormTaskConfig.value.version"
                @input="updateSelectedTaskConfig({ version: Number(($event.target as HTMLInputElement).value) || 1 })"
              >
            </label>
            <label class="panel-field">
              <span class="panel-label">所属部门 Department</span>
              <input
                type="text"
                class="panel-input"
                :value="p.selectedFormTaskConfig.value.department"
                placeholder="可选"
                @input="updateSelectedTaskConfig({ department: ($event.target as HTMLInputElement).value })"
              >
            </label>
            <label class="panel-field">
              <span class="panel-label">角色 Roles</span>
              <input
                type="text"
                class="panel-input mono"
                :value="p.selectedTaskRolesText.value"
                placeholder="dispatcher, supervisor"
                @input="updateSelectedTaskConfig({ roles: parseRoles(($event.target as HTMLInputElement).value) })"
              >
            </label>
            <label class="panel-field">
              <span class="panel-label">回写键 Write Back</span>
              <input
                type="text"
                class="panel-input mono"
                :value="p.selectedFormTaskConfig.value.writeBackKey"
                placeholder="forms.Activity_xxx"
                @input="updateSelectedTaskConfig({ writeBackKey: ($event.target as HTMLInputElement).value })"
              >
            </label>
          </div>

          <div class="panel-label block">
            行为选项
          </div>
          <div class="toggle-stack">
            <label class="toggle-row">
              <input type="checkbox" :checked="p.selectedFormTaskConfig.value.completeTaskOnSubmit" @change="updateSelectedTaskConfig({ completeTaskOnSubmit: ($event.target as HTMLInputElement).checked })">
              <span>
                <strong>提交后自动完成任务</strong>
                <small>表单提交成功即 complete 当前 UserTask</small>
              </span>
            </label>
            <label class="toggle-row">
              <input type="checkbox" :checked="p.selectedFormTaskConfig.value.allowResubmit" @change="updateSelectedTaskConfig({ allowResubmit: ($event.target as HTMLInputElement).checked })">
              <span>
                <strong>允许重复提交</strong>
                <small>任务完成后仍可再次提交表单</small>
              </span>
            </label>
          </div>

          <label class="panel-field">
            <span class="panel-label">表单说明</span>
            <textarea
              rows="3"
              class="panel-textarea"
              :value="p.selectedFormTaskConfig.value.description"
              placeholder="展示给现场人员的操作提示…"
              @input="updateSelectedTaskConfig({ description: ($event.target as HTMLTextAreaElement).value })"
            />
          </label>

          <div class="panel-divider" />
          <FormFieldDesigner
            :model-value="p.selectedFormTaskConfig.value.fields"
            @update:model-value="updateSelectedTaskConfig({ fields: $event })"
          />
        </div>
      </template>

      <!-- 未选中节点 -->
      <template v-else>
        <div class="panel-card empty-node-card">
          <div class="panel-card-title success">
            当前选中节点
          </div>
          <div class="empty-node-hint">
            请在左侧画布选择一个用户任务节点，在此配置动作与通知。
          </div>
        </div>
      </template>
    </div>

    <!-- ========== Tab 3: AI 工具 ========== -->
    <div v-show="activeTab === 'ai'" class="panel-tab-content" role="tabpanel">
      <template v-if="!p.hasSelectedDiagram.value">
        <div class="panel-card empty-node-card">
          <div class="empty-node-hint">
            请先在左侧选择业务事项类型。
          </div>
        </div>
      </template>

      <template v-else>
        <div class="panel-section-title">
          AI 语音业务事项抽取
        </div>
        <div class="panel-card">
          <div class="panel-card-title accent">
            语音助手识别与航班匹配
          </div>

          <label class="toggle-row block">
            <input v-model="p.aiConfig.value.enabled" type="checkbox">
            <span>
              <strong>启用语音助手 AI 抽取能力</strong>
              <small>关闭后该事项不会参与语音建单</small>
            </span>
          </label>

          <template v-if="p.aiConfig.value.enabled">
            <label class="panel-field">
              <span class="panel-label">事项别名（英文逗号分隔）</span>
              <input
                v-model="p.aiConfigAliasesText.value"
                type="text"
                class="panel-input"
                placeholder="例如: 开包, 登机口开包"
              >
            </label>
            <label class="panel-field">
              <span class="panel-label">触发短语（英文逗号分隔）</span>
              <input
                v-model="p.aiConfigTriggerText.value"
                type="text"
                class="panel-input"
                placeholder="例如: 登机口需要开包"
              >
            </label>

            <div class="panel-subblock">
              <div class="panel-subblock-title">
                航段绑定校验
              </div>
              <div class="check-row">
                <label class="check"><input v-model="p.aiConfig.value.leg_binding.allowed" type="checkbox" value="outbound"><span>出港 outbound</span></label>
                <label class="check"><input v-model="p.aiConfig.value.leg_binding.allowed" type="checkbox" value="inbound"><span>进港 inbound</span></label>
              </div>
              <div class="panel-grid-2">
                <label class="panel-field">
                  <span class="panel-label">默认航段</span>
                  <select v-model="p.aiConfig.value.leg_binding.default" class="panel-input">
                    <option :value="null">
                      不指定
                    </option>
                    <option value="outbound">
                      出港 outbound
                    </option>
                    <option value="inbound">
                      进港 inbound
                    </option>
                  </select>
                </label>
                <label class="toggle-row compact">
                  <input v-model="p.aiConfig.value.leg_binding.required" type="checkbox">
                  <span><strong>强制要求航段</strong></span>
                </label>
              </div>
            </div>

            <div class="panel-subblock">
              <div class="panel-subblock-title">
                航班匹配策略
              </div>
              <div class="panel-grid-2">
                <label class="panel-field"><span class="panel-label">起飞前窗口 (小时)</span><input
                  v-model.number="p.aiConfig.value.flight_matching.window_hours_before"
                  type="number"
                  min="0"
                  class="panel-input"
                ></label>
                <label class="panel-field"><span class="panel-label">起飞后窗口 (小时)</span><input
                  v-model.number="p.aiConfig.value.flight_matching.window_hours_after"
                  type="number"
                  min="0"
                  class="panel-input"
                ></label>
                <label class="panel-field">
                  <span class="panel-label">优先匹配航段</span>
                  <select v-model="p.aiConfig.value.flight_matching.prefer_leg" class="panel-input">
                    <option :value="null">
                      无
                    </option>
                    <option value="outbound">
                      出港 outbound
                    </option>
                    <option value="inbound">
                      进港 inbound
                    </option>
                  </select>
                </label>
                <label class="panel-field"><span class="panel-label">自动匹配置信度</span><input
                  v-model.number="p.aiConfig.value.flight_matching.min_auto_match_score"
                  type="number"
                  step="0.05"
                  min="0"
                  max="1"
                  class="panel-input"
                ></label>
              </div>
              <div class="check-col">
                <label class="check"><input v-model="p.aiConfig.value.flight_matching.exclude_cancelled" type="checkbox"><span>排除已取消航班</span></label>
                <label class="check"><input v-model="p.aiConfig.value.flight_matching.exclude_departed" type="checkbox"><span>排除已起飞/到达航班</span></label>
                <label class="check"><input v-model="p.aiConfig.value.flight_matching.exclude_actual_departure" type="checkbox"><span>排除已有实际起飞时间的航班</span></label>
              </div>
            </div>

            <div class="panel-subblock">
              <div class="panel-subblock-title">
                描述与备注模板
              </div>
              <label class="panel-field"><span class="panel-label">描述模板</span><textarea
                v-model="p.aiConfig.value.description_template"
                rows="2"
                class="panel-textarea"
                placeholder="登机口开包，座位号 {{seat_no}}"
              /></label>
              <label class="panel-field"><span class="panel-label">备注模板</span><textarea
                v-model="p.aiConfig.value.remarks_template"
                rows="2"
                class="panel-textarea"
                placeholder="座位号 {{seat_no}}"
              /></label>
            </div>

            <div class="panel-subblock">
              <div class="panel-subblock-title">
                抽取字段
              </div>
              <label class="panel-field"><span class="panel-label">禁止录入字段（英文逗号分隔）</span><input
                v-model="p.aiConfigForbiddenText.value"
                type="text"
                class="panel-input"
                placeholder="gate, stand"
              ></label>
              <label class="panel-field">
                <span class="panel-label">抽取字段定义（JSON Map）</span>
                <textarea
                  v-model="p.aiConfigFieldsJsonText.value"
                  rows="6"
                  class="panel-textarea mono"
                  placeholder="{&quot;seat_no&quot;: {&quot;type&quot;: &quot;string&quot;, &quot;label&quot;: &quot;座位号&quot;, &quot;required&quot;: true}}"
                />
                <span v-if="p.aiConfigFieldsJsonError.value" class="field-error">{{ p.aiConfigFieldsJsonError.value }}</span>
              </label>
            </div>
          </template>

          <UiButton
            variant="primary"
            size="md"
            class="panel-save"
            @click="p.saveAiConfig"
          >
            保存 AI 抽取配置
          </UiButton>
        </div>

        <div class="panel-section-title">
          业务规则属性
        </div>
        <div class="panel-card">
          <div class="panel-card-title">
            航班绑定、通知回执与重复防护
          </div>

          <div class="panel-subblock">
            <div class="panel-subblock-title">
              航班与航段绑定
            </div>
            <div class="check-row">
              <label class="check"><input v-model="p.caseProperties.value.binding_policy.flight_required" type="checkbox"><span>必须绑定航班</span></label>
              <label class="check"><input v-model="p.caseProperties.value.binding_policy.leg_type_required" type="checkbox"><span>必须绑定航段</span></label>
            </div>
            <div class="check-row">
              <span class="inline-label">允许航段</span>
              <label class="check"><input v-model="p.caseProperties.value.binding_policy.allowed_leg_types" type="checkbox" value="outbound"><span>出港</span></label>
              <label class="check"><input v-model="p.caseProperties.value.binding_policy.allowed_leg_types" type="checkbox" value="inbound"><span>进港</span></label>
            </div>
            <div class="panel-grid-2">
              <label class="panel-field">
                <span class="panel-label">默认航段</span>
                <select v-model="p.caseProperties.value.binding_policy.default_leg_type" class="panel-input">
                  <option :value="null">
                    不指定
                  </option>
                  <option value="outbound">
                    出港 outbound
                  </option>
                  <option value="inbound">
                    进港 inbound
                  </option>
                </select>
              </label>
              <label class="panel-field"><span class="panel-label">自动匹配置信度</span><input
                v-model.number="p.caseProperties.value.binding_policy.flight_match_policy.min_auto_match_score"
                type="number"
                step="0.05"
                min="0"
                max="1"
                class="panel-input"
              ></label>
              <label class="panel-field"><span class="panel-label">起飞前窗口 (小时)</span><input
                v-model.number="p.caseProperties.value.binding_policy.flight_match_policy.time_window_hours_before"
                type="number"
                min="0"
                class="panel-input"
              ></label>
              <label class="panel-field"><span class="panel-label">起飞后窗口 (小时)</span><input
                v-model.number="p.caseProperties.value.binding_policy.flight_match_policy.time_window_hours_after"
                type="number"
                min="0"
                class="panel-input"
              ></label>
            </div>
            <div class="check-col">
              <label class="check"><input v-model="p.caseProperties.value.binding_policy.flight_match_policy.allow_numeric_suffix" type="checkbox"><span>允许纯数字航班号后缀匹配</span></label>
              <label class="check"><input v-model="p.caseProperties.value.binding_policy.flight_match_policy.exclude_cancelled" type="checkbox"><span>排除已取消航班</span></label>
              <label class="check"><input v-model="p.caseProperties.value.binding_policy.flight_match_policy.exclude_departed" type="checkbox"><span>排除已起飞/到达航班</span></label>
              <label class="check"><input v-model="p.caseProperties.value.binding_policy.flight_match_policy.exclude_actual_departure" type="checkbox"><span>排除已有实际起飞时间的航班</span></label>
            </div>
          </div>

          <div class="panel-subblock">
            <div class="panel-subblock-title">
              额外信息
            </div>
            <label class="panel-field"><span class="panel-label">摘要模板</span><textarea
              v-model="p.caseProperties.value.extra_info_schema.summary_template"
              rows="2"
              class="panel-textarea"
              placeholder="座位号 {{seat_no}}"
            /></label>
            <label class="panel-field">
              <span class="panel-label">字段定义（JSON Map）</span>
              <textarea
                v-model="p.casePropertiesFieldsJsonText.value"
                rows="5"
                class="panel-textarea mono"
                placeholder="{&quot;seat_no&quot;: {&quot;type&quot;: &quot;string&quot;, &quot;label&quot;: &quot;座位号&quot;}}"
              />
              <span v-if="p.casePropertiesFieldsJsonError.value" class="field-error">{{ p.casePropertiesFieldsJsonError.value }}</span>
            </label>
          </div>

          <div class="panel-subblock">
            <div class="panel-subblock-title">
              批量流程通知
            </div>
            <label class="toggle-row block">
              <input v-model="p.caseProperties.value.workflow_policy.batch_notification_enabled" type="checkbox">
              <span><strong>启用同组聚合通知</strong></span>
            </label>
            <label class="panel-field">
              <span class="panel-label">回执模式</span>
              <select v-model="p.caseProperties.value.workflow_policy.batch_receipt_mode" class="panel-input">
                <option value="per_case">
                  每个事项独立回执
                </option>
                <option value="shared_group">
                  同组共享回执
                </option>
              </select>
            </label>
          </div>

          <div class="panel-subblock">
            <div class="panel-subblock-title">
              重复事项防护
            </div>
            <label class="toggle-row block">
              <input v-model="p.caseProperties.value.duplicate_policy.enabled" type="checkbox">
              <span><strong>启用重复检查</strong></span>
            </label>
            <label class="panel-field"><span class="panel-label">参与比对字段（英文逗号分隔）</span><input
              v-model="p.duplicatePolicyFieldsText.value"
              type="text"
              class="panel-input"
              placeholder="seat_no"
            ></label>
            <label class="panel-field"><span class="panel-label">活动状态白名单（英文逗号分隔）</span><input
              v-model="p.duplicatePolicyStatusesText.value"
              type="text"
              class="panel-input"
              placeholder="INITIAL, PENDING"
            ></label>
            <div class="check-col">
              <label class="check"><input v-model="p.caseProperties.value.duplicate_policy.include_bound_leg" type="checkbox"><span>比对绑定航段</span></label>
              <label class="check"><input v-model="p.caseProperties.value.duplicate_policy.include_extra_info" type="checkbox"><span>比对额外信息摘要</span></label>
            </div>
          </div>

          <UiButton
            variant="primary"
            size="md"
            class="panel-save"
            @click="p.saveCaseProperties"
          >
            保存业务规则属性
          </UiButton>
        </div>
      </template>
    </div>
  </aside>
</template>

<style scoped>
/* 信号面 token + UI 库件（UiButton / UiPill） */

.editor-properties-panel {
  width: min(480px, 42vw);
  min-width: 360px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--face-work);
  border-left: 1px solid var(--line);
  color: var(--ink);
  z-index: 10;
  position: relative;
}

/* ---- Header ---- */
.panel-header {
  padding: var(--s3) var(--s4);
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s2);
  flex-shrink: 0;
  background: var(--face-work);
}

.panel-header-title {
  font-size: var(--fs-section);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
  min-width: 0;
}

/* ---- Tabs ---- */
.panel-tabs {
  display: flex;
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
  background: var(--face-work);
}

.panel-tab {
  flex: 1;
  padding: var(--s3) var(--s1);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
  text-align: center;
  cursor: pointer;
  border: none;
  border-bottom: 2px solid transparent;
  transition: color var(--t-fast) var(--ease), background var(--t-fast) var(--ease), border-color var(--t-fast) var(--ease);
  user-select: none;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--s1);
  background: none;
  font-family: inherit;
}

.panel-tab :deep(.svg-icon),
.panel-tab :deep(svg) {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  opacity: 0.85;
}

.panel-tab:hover {
  color: var(--ink);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
}

.panel-tab[aria-selected='true'] {
  color: var(--act);
  border-bottom-color: var(--act);
  font-weight: var(--fw-semibold);
}

.panel-tab:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: -2px;
}

/* ---- Tab body ---- */
.panel-tab-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--s4);
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.panel-section-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  margin: 0;
  padding-bottom: var(--s2);
  border-bottom: 2px solid var(--line);
}

.panel-section-title.is-plain {
  border-bottom: none;
  padding-bottom: 0;
  margin-bottom: var(--s2);
}

.panel-section-hint {
  font-weight: var(--fw-regular);
  color: var(--ink-muted);
  font-size: var(--fs-label);
  margin-left: var(--s1);
}

.panel-card {
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  padding: var(--s3) var(--s4);
  box-shadow: var(--shadow-sm);
}

.panel-card-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  margin-bottom: var(--s3);
  color: var(--ink);
}

.panel-card-title.accent {
  color: var(--act);
}

.panel-card-title.success {
  color: var(--ok);
}

.panel-card-title code {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
}

.panel-field {
  display: flex;
  flex-direction: column;
  gap: var(--s1);
  margin-bottom: var(--s3);
}

.panel-field:last-child {
  margin-bottom: 0;
}

.panel-label {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  font-weight: var(--fw-medium);
}

.panel-label.block {
  display: block;
  margin: var(--s1) 0 var(--s2);
}

.panel-input,
.panel-textarea {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid var(--line);
  border-radius: var(--r-cell);
  padding: var(--s2) var(--s3);
  font-size: var(--fs-label);
  color: var(--ink);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  font-family: inherit;
  transition: border-color var(--t-fast) var(--ease), background var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
}

.panel-textarea {
  min-height: 72px;
  resize: vertical;
  line-height: 1.45;
  border-radius: var(--r-control);
  padding: var(--s3);
  font-size: var(--fs-body);
}

.panel-input:hover,
.panel-textarea:hover {
  border-color: color-mix(in srgb, var(--act) 30%, var(--line));
}

.panel-input:focus,
.panel-textarea:focus {
  outline: 2px solid var(--act);
  outline-offset: 2px;
  border-color: var(--act);
  background: var(--face-work);
}

.panel-input.mono,
.panel-textarea.mono,
.mono {
  font-family: var(--mono);
  font-size: var(--fs-label);
}

.panel-grid-2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--s3);
  margin-bottom: var(--s3);
}

.panel-grid-2 .panel-field {
  margin-bottom: 0;
}

.panel-hint {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  line-height: 1.45;
  margin: 0 0 var(--s3);
}

.panel-hint code {
  font-size: var(--fs-label);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  padding: 0 4px;
  border-radius: 4px;
}

.panel-divider {
  height: 1px;
  background: var(--line);
  margin: var(--s3) 0;
}

.panel-save {
  width: 100%;
  margin-top: var(--s1);
}

/* summary rows (flat, not metric tiles) */
.summary-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding: var(--s2) 0;
  border-bottom: 1px solid var(--line);
  font-size: var(--fs-label);
}

.summary-row.is-last {
  border-bottom: none;
  padding-bottom: 0;
}

.summary-row:first-child {
  padding-top: 0;
}

.summary-key {
  color: var(--ink-subtle);
  flex-shrink: 0;
}

.summary-val {
  font-weight: var(--fw-semibold);
  color: var(--ink);
  text-align: right;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.summary-val.accent {
  color: var(--act);
}

/* context vars */
.context-variable-list {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
}

.context-variable-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  padding: var(--s1) var(--s2);
  border-radius: var(--r-pill);
  font-size: var(--fs-label);
  background: var(--act-soft);
  color: var(--act);
  border: 1px solid color-mix(in srgb, var(--act) 16%, transparent);
}

.context-variable-chip code {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
}

/* checks / toggles */
.check-row,
.check-col {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2) var(--s3);
  margin-bottom: var(--s3);
}

.check-col {
  flex-direction: column;
  gap: var(--s2);
}

.check {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
  font-size: var(--fs-label);
  color: var(--ink);
  cursor: pointer;
}

.check input {
  width: 14px;
  height: 14px;
  accent-color: var(--act);
}

.check.dept {
  width: calc(50% - var(--s2));
  min-width: 120px;
}

.dept-grid {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
  max-height: 220px;
  overflow-y: auto;
  padding: var(--s3);
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  margin-bottom: var(--s2);
}

.toggle-stack {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
  margin-bottom: var(--s3);
}

.toggle-row {
  display: flex;
  align-items: flex-start;
  gap: var(--s3);
  cursor: pointer;
  padding: var(--s3);
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  font-size: var(--fs-label);
  color: var(--ink);
}

.toggle-row.block {
  margin-bottom: var(--s3);
}

.toggle-row.compact {
  align-items: center;
  margin-bottom: 0;
  padding: var(--s2) var(--s3);
  min-height: var(--h-md);
}

.toggle-row input {
  margin-top: 2px;
  width: 15px;
  height: 15px;
  accent-color: var(--act);
  flex-shrink: 0;
}

.toggle-row strong {
  display: block;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.toggle-row small {
  display: block;
  margin-top: 2px;
  font-size: var(--fs-label);
  color: var(--ink-muted);
  line-height: 1.35;
}

.token-row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
  margin-top: var(--s2);
}

.token-btn {
  border: 1px solid var(--line);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  color: var(--act);
  border-radius: var(--r-pill);
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  padding: var(--s1) var(--s2);
  cursor: pointer;
  font-family: inherit;
}

.token-btn:hover {
  border-color: color-mix(in srgb, var(--act) 40%, var(--line));
}

.panel-subblock {
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  padding: var(--s3);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  margin-bottom: var(--s3);
}

.panel-subblock-title {
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--ink-subtle);
  margin-bottom: var(--s3);
}

.inline-label {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  font-weight: var(--fw-semibold);
  align-self: center;
}

.field-error {
  color: var(--danger);
  font-size: var(--fs-label);
  margin-top: var(--s1);
}

.empty-node-card {
  text-align: center;
}

.empty-node-hint {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  background: color-mix(in srgb, var(--ink) 6%, transparent);
  padding: var(--s3);
  border-radius: var(--r-cell);
  border: 1px dashed var(--line);
  line-height: 1.5;
}

@media (max-width: 1200px) {
  .editor-properties-panel {
    width: min(400px, 46vw);
    min-width: 320px;
  }

  .panel-grid-2 {
    grid-template-columns: 1fr;
  }
}
</style>
