<script setup lang="ts">
// FlightImports Page - parity with legacy flight_imports.html + flight_import_schemas.rs
import { pageUrl } from '@/shared/page-routes';
import { useFlightImports } from '@/composables/useFlightImports';
import { useAuth } from '@/composables/useAuth';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';
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

function actionBadgeClass(action: string): string {
  if (action === 'create') return 'badge badge-create';
  if (action === 'update') return 'badge badge-update';
  return 'badge badge-skip';
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
          <button class="logout-btn" title="退出登录" type="button" @click="handleLogout">
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
                  <button
                    id="previewBtn"
                    class="btn btn-primary"
                    type="button"
                    :disabled="loading"
                    @click="handleUploadClick"
                  >
                    {{ loading ? '解析中...' : '上传并预览' }}
                  </button>
                  <button
                    id="commitBtn"
                    class="btn btn-secondary"
                    type="button"
                    :disabled="!canCommit"
                    @click="commitImport()"
                  >
                    确认导入
                  </button>
                  <button
                    v-if="fileSelected || selectedFile"
                    class="btn btn-secondary"
                    type="button"
                    @click="handleReset"
                  >
                    重置
                  </button>
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
                    <span :class="actionBadgeClass(row.action)">
                      {{ actionLabel(row.action) }}
                    </span>
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
  background: var(--admin-card-bg, var(--bg-card));
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 14px;
  box-shadow: var(--ws-shadow-sm, 0 10px 28px rgba(15, 23, 42, 0.05));
  padding: 24px 28px;
  margin-bottom: 24px;
}

.hero h1 {
  margin: 0 0 6px;
  font-size: 26px;
  color: var(--admin-text, var(--text-primary));
}

.hero p {
  margin: 0;
  color: var(--admin-text-subtle, var(--text-tertiary));
  line-height: 1.6;
}

.grid {
  display: grid;
  grid-template-columns: 360px 1fr;
  gap: 20px;
  align-items: start;
}

.panel {
  background: var(--admin-card-bg, var(--bg-card));
  border: 1px solid var(--admin-border, var(--border-light));
  border-radius: 14px;
  box-shadow: var(--ws-shadow-sm, 0 10px 28px rgba(15, 23, 42, 0.05));
  padding: 20px 22px;
}

.panel-rows {
  margin-top: 20px;
}

.panel h2 {
  margin: 0 0 14px;
  font-size: 16px;
  color: var(--admin-text, var(--text-primary));
}

.muted {
  color: var(--admin-text-subtle, var(--text-tertiary));
  font-size: 13px;
}

.upload-box {
  border: 1px dashed var(--admin-border, var(--border-light));
  border-radius: 14px;
  padding: 18px;
  background: var(--ws-surface-muted, transparent);
}

.file-name {
  margin-top: 8px;
  font-size: 13px;
  color: var(--admin-text, var(--text-primary));
}

.actions {
  display: flex;
  gap: 10px;
  margin-top: 14px;
  flex-wrap: wrap;
}

.progress-wrap {
  margin-top: 8px;
}

.progress-track {
  background: var(--bg-input);
  border-radius: 4px;
  height: 8px;
  overflow: hidden;
}

.progress-bar {
  background: var(--status-progress);
  height: 100%;
  transition: width 0.3s;
}

.meta-list {
  display: grid;
  gap: 10px;
}

.meta-list-top {
  margin-top: 16px;
}

.meta-item {
  padding: 12px 14px;
  border-radius: 12px;
  background: var(--glass-bg);
  border: 1px solid var(--border-light);
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
  margin-bottom: 18px;
}

.summary-card {
  padding: 12px 14px;
  border-radius: 12px;
  background: var(--glass-bg);
  border: 1px solid var(--border-light);
}

.summary-card strong {
  display: block;
  font-size: 22px;
  color: var(--text-primary);
}

.summary-card span {
  font-size: 12px;
  color: var(--text-tertiary);
  text-transform: uppercase;
}

.code {
  font-family: "SFMono-Regular", Consolas, monospace;
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-word;
}

.badge {
  display: inline-flex;
  align-items: center;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 700;
}

.badge-create {
  background: var(--dh-signal-ok-soft);
  color: var(--status-text-departed);
}

.badge-update {
  background: #dbeafe;
  color: #1d4ed8;
}

.badge-skip {
  background: var(--dh-signal-warn-soft);
  color: var(--status-text-checkin-end);
}

.row-card {
  border: 1px solid var(--border-light);
  border-radius: 14px;
  padding: 16px;
  margin-bottom: 14px;
  background: var(--bg-card);
}

.row-head {
  display: flex;
  justify-content: space-between;
  gap: 14px;
  align-items: center;
  margin-bottom: 12px;
  flex-wrap: wrap;
}

.kv {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  margin-bottom: 12px;
}

.kv .meta-item {
  min-height: 84px;
}

.list {
  margin: 8px 0 0;
  padding-left: 18px;
  color: var(--text-secondary);
}

.list li {
  margin: 4px 0;
}

@media (max-width: 1080px) {
  .grid,
  .summary-grid,
  .kv {
    grid-template-columns: 1fr;
  }
}
</style>
