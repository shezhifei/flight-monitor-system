<script setup lang="ts">
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import EmptyState from '@/components/ui/EmptyState.vue';
import { useOntologyWorkbench } from './useOntologyWorkbench';
import {
  idField,
  linkStatusTone,
  suggestionStatusTone,
  type OntologyTabId,
} from './types';

const {
  activeTab,
  contextMode,
  contextKey,
  busy,
  loadingView,
  flightView,
  aircraftView,
  suggestions,
  links,
  lastWarnings,
  lastScan,
  reassignForm,
  standForm,
  gateForm,
  suggestionForm,
  linkForm,
  canRead,
  canReassign,
  canStand,
  canGate,
  canConfirm,
  loadContextView,
  loadSuggestions,
  submitReassign,
  submitAllocateStand,
  submitAllocateGate,
  submitCreateSuggestion,
  acceptSuggestion,
  rejectSuggestion,
  submitCreateLink,
  breakLink,
  runAutoScan,
  confirmDrafts,
} = useOntologyWorkbench();

const tabs: { id: OntologyTabId; label: string }[] = [
  { id: 'views', label: '资源视图' },
  { id: 'reassign', label: '换机' },
  { id: 'resources', label: '机位 / 登机口' },
  { id: 'suggestions', label: '资源建议' },
  { id: 'links', label: '周转链接' },
];

function countLabel(n: number | undefined): string {
  return String(n ?? 0);
}
</script>

<template>
  <div class="ontology-page">
    <div class="ontology-shell">
      <header class="ontology-header">
        <div>
          <h1>本体资源台</h1>
          <p>
            飞机中心运行本体：换机、机位/登机口正式占用、分权建议与周转链接。
            界面使用工作区 dual-theme token（<code>--ws-*</code> / 信号色 <code>--dh-signal-*</code>）。
          </p>
          <div class="ontology-perm-bar" style="margin-top: 12px">
            <span class="ontology-pill" :class="canRead ? 'tone-ok' : 'tone-muted'">read</span>
            <span class="ontology-pill" :class="canReassign ? 'tone-ok' : 'tone-muted'">reassign</span>
            <span class="ontology-pill" :class="canStand ? 'tone-ok' : 'tone-muted'">stand</span>
            <span class="ontology-pill" :class="canGate ? 'tone-ok' : 'tone-muted'">gate</span>
            <span class="ontology-pill" :class="canConfirm ? 'tone-ok' : 'tone-muted'">confirm</span>
          </div>
        </div>
        <div class="ontology-header-actions">
          <ThemeToggle />
        </div>
      </header>

      <section class="ontology-context" aria-label="上下文查询">
        <div class="ontology-field">
          <label>上下文类型</label>
          <div class="ontology-seg" role="group" aria-label="查询模式">
            <button
              type="button"
              :class="{ 'is-active': contextMode === 'flight' }"
              @click="contextMode = 'flight'"
            >
              航班
            </button>
            <button
              type="button"
              :class="{ 'is-active': contextMode === 'aircraft' }"
              @click="contextMode = 'aircraft'"
            >
              机号
            </button>
          </div>
        </div>
        <div class="ontology-field">
          <label>{{ contextMode === 'flight' ? '航班 ID' : '机号 registration' }}</label>
          <input
            v-model="contextKey"
            type="text"
            :placeholder="contextMode === 'flight' ? '例如 FL…' : '例如 B-1234'"
            @keyup.enter="loadContextView()"
          />
        </div>
        <div class="ontology-actions" style="margin: 0">
          <button
            type="button"
            class="oc-btn oc-btn-primary"
            :disabled="loadingView || busy"
            @click="loadContextView()"
          >
            {{ loadingView ? '加载中…' : '加载资源视图' }}
          </button>
          <button
            v-if="canConfirm"
            type="button"
            class="oc-btn oc-btn-secondary"
            :disabled="busy"
            @click="confirmDrafts()"
          >
            确认 draft
          </button>
        </div>
      </section>

      <nav class="ontology-tabs" aria-label="功能分区">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          type="button"
          class="ontology-tab"
          :class="{ 'is-active': activeTab === tab.id }"
          @click="activeTab = tab.id"
        >
          {{ tab.label }}
        </button>
      </nav>

      <section v-show="activeTab === 'views'" class="ontology-panel">
        <h2>双视图</h2>
        <p class="ontology-panel-desc">
          航段资源视图 / 飞机资源视图（§5.3）。冲突与一致性以告警呈现，不阻断操作。
        </p>

        <div v-if="!flightView && !aircraftView" class="ontology-empty">
          <EmptyState
            icon="search"
            title="尚未加载上下文"
            description="输入航班 ID 或机号后点击「加载资源视图」。"
          />
        </div>

        <div v-else class="ontology-grid-2">
          <article v-if="flightView" class="ontology-card">
            <h3>航段资源</h3>
            <dl class="ontology-kv">
              <dt>航班</dt>
              <dd>{{ flightView.flight_id }}</dd>
              <dt>机号</dt>
              <dd>{{ flightView.registration || '—' }}</dd>
              <dt>计划机位</dt>
              <dd>{{ flightView.plan_stand || '—' }}</dd>
              <dt>计划登机口</dt>
              <dd>{{ flightView.plan_gate || '—' }}</dd>
              <dt>占用</dt>
              <dd>{{ countLabel(flightView.occupations?.length) }} 条</dd>
              <dt>口分配</dt>
              <dd>{{ countLabel(flightView.assignments?.length) }} 条</dd>
              <dt>链接</dt>
              <dd>{{ countLabel(flightView.turnaround_links?.length) }} 条</dd>
            </dl>
          </article>

          <article v-if="aircraftView" class="ontology-card">
            <h3>飞机资源</h3>
            <dl class="ontology-kv">
              <dt>机号</dt>
              <dd>{{ aircraftView.registration }}</dd>
              <dt>在场</dt>
              <dd>
                <span class="ontology-pill" :class="aircraftView.in_field ? 'tone-ok' : 'tone-muted'">
                  {{ aircraftView.in_field ? '在场' : '不在场' }}
                </span>
              </dd>
              <dt>当前机位</dt>
              <dd>{{ aircraftView.current_stand || '—' }}</dd>
              <dt>当前登机口</dt>
              <dd>{{ aircraftView.current_gate || '—' }}</dd>
              <dt>关联航班</dt>
              <dd>{{ countLabel(aircraftView.flights?.length) }}</dd>
            </dl>
          </article>
        </div>

        <div v-if="lastWarnings.length" class="ontology-alert warn" role="status">
          <strong>告警</strong>
          <ul class="ontology-json-list">
            <li v-for="(w, i) in lastWarnings" :key="i">{{ w }}</li>
          </ul>
        </div>
      </section>

      <section v-show="activeTab === 'reassign'" class="ontology-panel">
        <h2>换机 ReassignAircraft</h2>
        <p class="ontology-panel-desc">
          AOC 权限。进港前站起飞 / 出港登机后禁止换机；换机后维护周转链接健康并过期旧建议。
        </p>
        <div class="ontology-grid-2">
          <div class="ontology-field">
            <label for="reassign-flight-id">航班 ID</label>
            <input id="reassign-flight-id" v-model="reassignForm.flight_id" type="text" placeholder="flight_id" />
          </div>
          <div class="ontology-field">
            <label for="reassign-registration">新机号（原样）</label>
            <input id="reassign-registration" v-model="reassignForm.new_registration" type="text" placeholder="B-xxxx" />
          </div>
        </div>
        <div class="ontology-actions">
          <button
            type="button"
            class="oc-btn oc-btn-primary"
            :disabled="busy || !canReassign"
            @click="submitReassign()"
          >
            提交换机
          </button>
        </div>
      </section>

      <section v-show="activeTab === 'resources'" class="ontology-panel">
        <h2>正式机位 / 登机口</h2>
        <p class="ontology-panel-desc">
          机位 AOC（ontology.stand.manage），登机口 TOC（ontology.gate.manage）。时段重叠仅告警。
        </p>

        <div class="ontology-grid-2">
          <div class="ontology-card">
            <h3>分配机位</h3>
            <div class="ontology-grid-2">
              <div class="ontology-field">
                <label for="stand-registration">机号</label>
                <input id="stand-registration" v-model="standForm.registration" type="text" />
              </div>
              <div class="ontology-field">
                <label for="stand-code">机位</label>
                <input id="stand-code" v-model="standForm.stand_code" type="text" />
              </div>
              <div class="ontology-field">
                <label for="stand-starts">开始</label>
                <input id="stand-starts" v-model="standForm.starts_at" type="datetime-local" />
              </div>
              <div class="ontology-field">
                <label for="stand-ends">结束</label>
                <input id="stand-ends" v-model="standForm.ends_at" type="datetime-local" />
              </div>
              <div class="ontology-field">
                <label for="stand-kind">类型</label>
                <select id="stand-kind" v-model="standForm.kind">
                  <option value="normal">normal</option>
                  <option value="moving">moving</option>
                </select>
              </div>
              <div class="ontology-field">
                <label for="stand-moving-to">拖曳目标机位</label>
                <input id="stand-moving-to" v-model="standForm.moving_to_stand" type="text" :disabled="standForm.kind !== 'moving'" />
              </div>
              <div class="ontology-field">
                <label for="stand-flight-id">关联航班（可选）</label>
                <input id="stand-flight-id" v-model="standForm.flight_id" type="text" />
              </div>
              <label class="ontology-check">
                <input v-model="standForm.sync_flight_plan" type="checkbox" />
                同步回写 Flight.stand
              </label>
            </div>
            <div class="ontology-actions">
              <button
                type="button"
                class="oc-btn oc-btn-primary"
                :disabled="busy || !canStand"
                @click="submitAllocateStand()"
              >
                分配机位
              </button>
            </div>
          </div>

          <div class="ontology-card">
            <h3>分配登机口</h3>
            <div class="ontology-grid-2">
              <div class="ontology-field">
                <label for="gate-registration">机号</label>
                <input id="gate-registration" v-model="gateForm.registration" type="text" />
              </div>
              <div class="ontology-field">
                <label for="gate-code">登机口</label>
                <input id="gate-code" v-model="gateForm.gate_code" type="text" />
              </div>
              <div class="ontology-field">
                <label for="gate-starts">开始</label>
                <input id="gate-starts" v-model="gateForm.starts_at" type="datetime-local" />
              </div>
              <div class="ontology-field">
                <label for="gate-ends">结束</label>
                <input id="gate-ends" v-model="gateForm.ends_at" type="datetime-local" />
              </div>
              <div class="ontology-field">
                <label for="gate-flight-id">关联航班（可选）</label>
                <input id="gate-flight-id" v-model="gateForm.flight_id" type="text" />
              </div>
              <label class="ontology-check">
                <input v-model="gateForm.sync_flight_plan" type="checkbox" />
                同步回写 Flight.gate
              </label>
            </div>
            <div class="ontology-actions">
              <button
                type="button"
                class="oc-btn oc-btn-primary"
                :disabled="busy || !canGate"
                @click="submitAllocateGate()"
              >
                分配登机口
              </button>
            </div>
          </div>
        </div>
      </section>

      <section v-show="activeTab === 'suggestions'" class="ontology-panel">
        <h2>资源调整建议</h2>
        <p class="ontology-panel-desc">
          机位建议仅 AOC 可接受，登机口建议仅 TOC 可接受；接受即回写计划字段并落正式资源。
        </p>

        <div class="ontology-card" style="margin-bottom: 16px">
          <h3>新建建议</h3>
          <div class="ontology-grid-3">
            <div class="ontology-field">
              <label>航班 ID</label>
              <input v-model="suggestionForm.flight_id" type="text" />
            </div>
            <div class="ontology-field">
              <label>类型</label>
              <select v-model="suggestionForm.kind">
                <option value="stand">stand</option>
                <option value="gate">gate</option>
              </select>
            </div>
            <div class="ontology-field">
              <label>建议值</label>
              <input v-model="suggestionForm.suggested_value" type="text" />
            </div>
            <div class="ontology-field">
              <label>当前值</label>
              <input v-model="suggestionForm.current_value" type="text" />
            </div>
            <div class="ontology-field" style="grid-column: span 2">
              <label>原因</label>
              <input v-model="suggestionForm.reason" type="text" />
            </div>
          </div>
          <div class="ontology-actions">
            <button type="button" class="oc-btn oc-btn-primary" :disabled="busy" @click="submitCreateSuggestion()">
              创建建议
            </button>
            <button type="button" class="oc-btn oc-btn-secondary" :disabled="busy" @click="loadSuggestions()">
              刷新列表
            </button>
          </div>
        </div>

        <div v-if="!suggestions.length" class="ontology-empty">暂无建议</div>
        <div v-else class="ontology-table-wrap">
          <table class="ontology-table">
            <thead>
              <tr>
                <th>航班</th>
                <th>类型</th>
                <th>当前 → 建议</th>
                <th>状态</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in suggestions" :key="item.id">
                <td>{{ idField(item.flight_id) }}</td>
                <td>{{ item.kind }}</td>
                <td>{{ item.current_value || '—' }} → <strong>{{ item.suggested_value }}</strong></td>
                <td>
                  <span class="ontology-pill" :class="`tone-${suggestionStatusTone(item.status)}`">
                    {{ item.status }}
                  </span>
                </td>
                <td>
                  <div class="row-actions">
                    <button
                      v-if="item.status === 'pending'"
                      type="button"
                      class="oc-btn oc-btn-primary"
                      style="height: 30px; padding: 0 10px; font-size: 0.78rem"
                      :disabled="busy"
                      @click="acceptSuggestion(item)"
                    >
                      接受
                    </button>
                    <button
                      v-if="item.status === 'pending'"
                      type="button"
                      class="oc-btn oc-btn-danger"
                      style="height: 30px; padding: 0 10px; font-size: 0.78rem"
                      :disabled="busy"
                      @click="rejectSuggestion(item)"
                    >
                      驳回
                    </button>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <section v-show="activeTab === 'links'" class="ontology-panel">
        <h2>周转链接</h2>
        <p class="ontology-panel-desc">
          任务对链接（进港↔出港）。同机为 active，异机为 broken。支持自动扫描建链。
        </p>

        <div class="ontology-card" style="margin-bottom: 16px">
          <h3>手工建链</h3>
          <div class="ontology-grid-3">
            <div class="ontology-field">
              <label>进港航班</label>
              <input v-model="linkForm.inbound_flight_id" type="text" />
            </div>
            <div class="ontology-field">
              <label>出港航班</label>
              <input v-model="linkForm.outbound_flight_id" type="text" />
            </div>
            <div class="ontology-field">
              <label>来源</label>
              <select v-model="linkForm.source">
                <option value="manual">manual</option>
                <option value="auto">auto</option>
              </select>
            </div>
          </div>
          <div class="ontology-actions">
            <button type="button" class="oc-btn oc-btn-primary" :disabled="busy" @click="submitCreateLink()">
              创建链接
            </button>
            <button type="button" class="oc-btn oc-btn-secondary" :disabled="busy" @click="runAutoScan()">
              自动扫描建链
            </button>
          </div>
          <div v-if="lastScan" class="ontology-alert info" role="status">
            上次扫描：评估 {{ lastScan.evaluated }}，新建 {{ lastScan.created.length }}，
            跳过 {{ lastScan.skipped }}
            <span v-if="lastScan.errors.length">，错误 {{ lastScan.errors.length }}</span>
          </div>
        </div>

        <div v-if="!links.length" class="ontology-empty">当前上下文暂无周转链接</div>
        <div v-else class="ontology-table-wrap">
          <table class="ontology-table">
            <thead>
              <tr>
                <th>进港</th>
                <th>出港</th>
                <th>状态</th>
                <th>来源</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="link in links" :key="link.id">
                <td>{{ idField(link.inbound_flight_id) }}</td>
                <td>{{ idField(link.outbound_flight_id) }}</td>
                <td>
                  <span class="ontology-pill" :class="`tone-${linkStatusTone(link.status)}`">
                    {{ link.status }}
                  </span>
                </td>
                <td>{{ link.source }}</td>
                <td>
                  <button
                    v-if="link.status === 'active'"
                    type="button"
                    class="oc-btn oc-btn-danger"
                    style="height: 30px; padding: 0 10px; font-size: 0.78rem"
                    :disabled="busy"
                    @click="breakLink(link)"
                  >
                    拆链
                  </button>
                  <span v-else class="ontology-pill tone-muted">{{ link.broken_reason || '已拆' }}</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </div>
  </div>
</template>
