<script setup lang="ts">
// FlightImports Page - parity with legacy flight_imports.html + flight_import_schemas.rs
import { pageUrl } from '@/shared/page-routes';
import { useFlightImports } from '@/composables/useFlightImports';
import { useAuth } from '@/composables/useAuth';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiPill from '@/components/ui/UiPill.vue';
import { ref } from 'vue';

const {
  loading,
  snapshot,
  summary,
  fieldMapping,
  airportContext,
  sourceFile,
  status,
  rows,
  globalErrors,
  fileSelected,
  fileName,
  importProgress,
  previewId,
  canCommit,
  preview,
  commitImport,
  reset,
} = useFlightImports();

const auth = useAuth();
function handleLogout() {
  auth.logout();
}

const fileInputRef = ref<HTMLInputElement | null>(null);
const selectedFile = ref<File | null>(null);

function handleFileChange(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0] ?? null;
  selectedFile.value = file;
  // Explicit user action only: do NOT auto-preview on file select.
  if (file) {
    fileName.value = file.name;
  }
}

function handleUploadClick() {
  const file = selectedFile.value ?? fileInputRef.value?.files?.[0] ?? null;
  if (!file) {
    window.alert('请选择 PAYLOAD.txt 或 json 文件');
    return;
  }
  void preview(file);
}

function handleReset() {
  selectedFile.value = null;
  reset({ clearFileInput: fileInputRef.value });
}

function pretty(value: unknown): string {
  try {
    return JSON.stringify(value ?? {}, null, 2);
  } catch {
    return String(value ?? '');
  }
}

function localizeMatchStrategy(value: string | null | undefined): string {
  const mapping: Record<string, string> = {
    identity_binding: '稳定 ID',
    'natural_key:inbound': '自然键: 进港',
    'natural_key:outbound': '自然键: 出港',
    'natural_key:inbound_outbound': '自然键: 进出港',
    none: '无',
  };
  return mapping[value || 'none'] || value || '无';
}

function actionLabel(action: string): string {
  if (action === 'create') return '新建';
  if (action === 'update') return '更新';
  if (action === 'skip') return '跳过';
  return action || '-';
}

type PillTone = 'act' | 'ok' | 'warn' | 'danger' | 'mute';
function actionTone(action: string): PillTone {
  if (action === 'create') return 'ok';
  if (action === 'update') return 'act';
  return 'warn';
}

const summaryCards = [
  { key: 'total_rows', label: '总行数' },
  { key: 'valid_rows', label: '有效' },
  { key: 'create_count', label: '新建' },
  { key: 'update_count', label: '更新' },
  { key: 'skip_count', label: '跳过' },
  { key: 'failed_count', label: '失败' },
  { key: 'warning_count', label: '警告' },
  { key: 'error_count', label: '错误' },
] as const;
</script>

<template>
  <div class="admin-container">
    <aside class="admin-sidebar">
      <div class="sidebar-header">
        <div class="sidebar-logo">
          <a :href="pageUrl('dashboard')">
            <SvgIcon src="/frontend/icons/fast.svg" />
            <span>系统管理</span>
          </a>
        </div>
      </div>

      <div class="sidebar-nav" />

      <div class="sidebar-footer">
        <div class="user-info">
          <div id="userAvatar" class="user-avatar">
            A
          </div>
          <div class="user-details">
            <div id="userName" class="user-name">
              加载中...
            </div>
            <div id="userRole" class="user-role">
              系统管理员
            </div>
          </div>
        </div>
        <div class="sidebar-footer-actions">
          <button
            class="logout-btn"
            title="退出登录"
            type="button"
            @click="handleLogout"
          >
            <SvgIcon src="/frontend/icons/logout.svg" :size="14" />
          </button>
          <a :href="pageUrl('dashboard')" class="nav-item sidebar-home-link">
            <span class="nav-item-icon"><SvgIcon src="/frontend/icons/home.svg" /></span>
            <span>返回工作台</span>
          </a>
        </div>
      </div>
    </aside>

    <main class="main-content">
      <header class="content-header">
        <div class="content-heading">
          <div class="content-title">
            航班导入
          </div>
          <div class="content-subtitle">
            上传航班文件，先预览快照与错误，再人工确认导入。
          </div>
        </div>
        <div class="header-actions" />
      </header>
      <div class="content-body">
        <div class="import-shell">
          <section class="hero">
            <h1>PAYLOAD 文件导入</h1>
            <p>
              工作流固定为上传解析预览、人工确认导入。预览会冻结快照；有错误的整批文件不能提交。机位仅保留最新单值。
            </p>
          </section>

          <div class="grid">
            <section class="panel">
              <h2>上传与提交</h2>
              <div class="upload-box">
                <input
                  id="fileInput"
                  ref="fileInputRef"
                  type="file"
                  accept=".txt,.json"
                  @change="handleFileChange"
                >
                <div v-if="fileName" class="file-name">
                  已选择: {{ fileName }}
                </div>
                <div class="actions">
                  <UiButton
                    id="previewBtn"
                    variant="primary"
                    :disabled="loading"
                    @click="handleUploadClick"
                  >
                    {{ loading ? '解析中...' : '上传并预览' }}
                  </UiButton>
                  <UiButton
                    id="commitBtn"
                    variant="tonal"
                    :disabled="!canCommit"
                    @click="commitImport()"
                  >
                    确认导入
                  </UiButton>
                  <UiButton
                    v-if="fileSelected || selectedFile"
                    variant="ghost"
                    @click="handleReset"
                  >
                    重置
                  </UiButton>
                </div>
                <div v-if="importProgress > 0 && importProgress < 100" class="progress-wrap">
                  <div class="progress-track">
                    <div class="progress-bar" :style="{ width: importProgress + '%' }" />
                  </div>
                </div>
              </div>
              <div class="meta-list meta-list-top">
                <div class="meta-item">
                  <div class="muted">
                    当前预览 ID
                  </div>
                  <div id="previewId" class="code">
                    {{ previewId || '-' }}
                  </div>
                </div>
                <div class="meta-item">
                  <div class="muted">
                    状态
                  </div>
                  <div id="previewStatus" class="code">
                    {{ loading ? '解析中...' : (status || (fileSelected ? '就绪' : '-')) }}
                  </div>
                </div>
                <div class="meta-item">
                  <div class="muted">
                    机场上下文
                  </div>
                  <div id="airportContext" class="code">
                    {{ snapshot ? pretty(airportContext) : '-' }}
                  </div>
                </div>
              </div>
            </section>

            <section class="panel">
              <h2>预览摘要</h2>
              <div id="summaryGrid" class="summary-grid">
                <template v-if="snapshot">
                  <div
                    v-for="card in summaryCards"
                    :key="card.key"
                    class="summary-card"
                  >
                    <span>{{ card.label }}</span>
                    <strong>{{ summary[card.key] }}</strong>
                  </div>
                </template>
              </div>
              <div class="meta-list">
                <div class="meta-item">
                  <div class="muted">
                    源文件
                  </div>
                  <div id="sourceFile" class="code">
                    {{ sourceFile ? pretty(sourceFile) : (fileName || '-') }}
                  </div>
                </div>
                <div class="meta-item">
                  <div class="muted">
                    字段映射
                  </div>
                  <div id="fieldMapping" class="code">
                    {{ snapshot ? pretty(fieldMapping) : '-' }}
                  </div>
                </div>
                <div class="meta-item">
                  <div class="muted">
                    全局错误
                  </div>
                  <div id="globalErrors" class="code">
                    {{ globalErrors.length ? pretty(globalErrors) : '无' }}
                  </div>
                </div>
              </div>
            </section>
          </div>

          <section class="panel panel-rows">
            <h2>行级预览</h2>
            <div id="rowsContainer">
              <template v-if="rows.length">
                <article
                  v-for="row in rows"
                  :key="row.source_row_key"
                  class="row-card"
                >
                  <div class="row-head">
                    <div>
                      <div>
                        <strong>{{ row.source_row_key || '-' }}</strong>
                      </div>
                      <div class="muted">
                        匹配策略: {{ localizeMatchStrategy(row.match_strategy) }}
                        · 已匹配航班: {{ row.matched_flight_id || '-' }}
                      </div>
                    </div>
                    <UiPill :tone="actionTone(row.action)">
                      {{ actionLabel(row.action) }}
                    </UiPill>
                  </div>
                  <div class="kv">
                    <div class="meta-item">
                      <div class="muted">
                        原始值
                      </div>
                      <div class="code">
                        {{ pretty(row.raw_values || {}) }}
                      </div>
                    </div>
                    <div class="meta-item">
                      <div class="muted">
                        归一化航班
                      </div>
                      <div class="code">
                        {{ pretty(row.normalized_flight || {}) }}
                      </div>
                    </div>
                    <div class="meta-item">
                      <div class="muted">
                        落库附加信息
                      </div>
                      <div class="code">
                        {{ pretty({
                          business_date: row.business_date,
                          natural_key: row.natural_key,
                          timeline_events: (row.timeline_events || []).length,
                          source_ids: row.source_ids || {},
                        }) }}
                      </div>
                    </div>
                  </div>
                  <div v-if="(row.warnings || []).length">
                    <div class="muted">
                      预警
                    </div>
                    <ul class="list">
                      <li v-for="(w, idx) in row.warnings" :key="'w' + idx">
                        {{ w }}
                      </li>
                    </ul>
                  </div>
                  <div v-if="(row.errors || []).length">
                    <div class="muted">
                      错误
                    </div>
                    <ul class="list">
                      <li v-for="(err, idx) in row.errors" :key="'e' + idx">
                        {{ err }}
                      </li>
                    </ul>
                  </div>
                </article>
              </template>
              <template v-else>
                <div class="muted">
                  {{ loading ? '正在解析...' : '尚未生成预览。' }}
                </div>
              </template>
            </div>
          </section>
        </div>
      </div>
    </main>
    <ThemeToggle />
  </div>
</template>

<style scoped>
/* 壳层复用 admin-layout / admin-page */

.import-shell {
  max-width: 1240px;
  margin: 0 auto;
  width: 100%;
}

.hero {
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-sm);
  padding: var(--s4) var(--s5);
  margin-bottom: var(--s4);
}

/* hero 大标题是展示级（26px），刻意不入字阶梯子 */
.hero h1 {
  margin: 0 0 var(--s2);
  font-size: 26px;
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.hero p {
  margin: 0;
  color: var(--ink-subtle);
  line-height: 1.6;
}

.grid {
  display: grid;
  grid-template-columns: 360px 1fr;
  gap: var(--s4);
  align-items: start;
}

.panel {
  background: var(--face-work);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-sm);
  padding: var(--s4);
}

.panel-rows {
  margin-top: var(--s4);
}

.panel h2 {
  margin: 0 0 var(--s3);
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.muted {
  color: var(--ink-subtle);
  font-size: var(--fs-body);
}

.upload-box {
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-panel);
  padding: var(--s3);
  background: var(--face-page);
}

.file-name {
  margin-top: var(--s2);
  font-size: var(--fs-body);
  color: var(--ink);
}

.actions {
  display: flex;
  gap: var(--s2);
  margin-top: var(--s3);
  flex-wrap: wrap;
}

.progress-wrap {
  margin-top: var(--s2);
}

.progress-track {
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  border-radius: var(--r-pill);
  height: 8px;
  overflow: hidden;
}

.progress-bar {
  background: var(--act);
  height: 100%;
  transition: width var(--t-slow) var(--ease);
}

.meta-list {
  display: grid;
  gap: var(--s2);
}

.meta-list-top {
  margin-top: var(--s3);
}

.meta-item {
  padding: var(--s3);
  border-radius: var(--r-control);
  background: var(--face-page);
  border: 1px solid var(--line);
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: var(--s2);
  margin-bottom: var(--s3);
}

.summary-card {
  padding: var(--s3);
  border-radius: var(--r-control);
  background: var(--face-page);
  border: 1px solid var(--line);
}

/* 展示级大数字（22px）刻意不入字阶梯子 */
.summary-card strong {
  display: block;
  font-size: 22px;
  font-variant-numeric: tabular-nums;
  color: var(--ink);
}

.summary-card span {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  text-transform: uppercase;
}

.code {
  font-family: var(--mono);
  font-size: var(--fs-label);
  white-space: pre-wrap;
  word-break: break-word;
}

.row-card {
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  padding: var(--s3);
  margin-bottom: var(--s3);
  background: var(--face-work);
}

.row-head {
  display: flex;
  justify-content: space-between;
  gap: var(--s3);
  align-items: center;
  margin-bottom: var(--s3);
  flex-wrap: wrap;
}

.kv {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--s2);
  margin-bottom: var(--s3);
}

.kv .meta-item {
  min-height: 84px;
}

.list {
  margin: var(--s2) 0 0;
  padding-left: 18px;
  color: var(--ink-subtle);
}

.list li {
  margin: var(--s1) 0;
}

@media (max-width: 1080px) {
  .grid,
  .summary-grid,
  .kv {
    grid-template-columns: 1fr;
  }
}
</style>
