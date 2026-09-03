(function () {
    'use strict';

    const MANIFEST_URL = '/frontend/static/ai/manifest.json';
    const loadedJs = new Set();
    const loadedCss = new Set();
    let manifestPromise = null;
    const ENTRY_LABELS = {
        ai_config_center: 'AI 配置中心',
        ai_monitor: 'AI 监控',
        dashboard_ai_widget: '工作台 AI 助手',
        dispatch_board_ai: '派工 AI 助手',
        flight_monitor_ai: '航班 AI 助手',
        flowable_assistant_ai: '流程助手',
        llm_eval_lab: 'LLM 评测实验室',
        nl_query: '自然语言查询',
    };

    class EntryNotFoundError extends Error {
        constructor(entryName) {
            super(`AI Entry "${entryName}" not found in manifest.`);
            this.name = 'EntryNotFoundError';
        }
    }

    class ManifestLoadError extends Error {
        constructor(status) {
            super(`Manifest load failed with status: ${status}`);
            this.name = 'ManifestLoadError';
            this.status = status;
        }
    }

    function normalizePath(path) {
        if (!path) {
            return '';
        }
        if (path.startsWith('http://') || path.startsWith('https://') || path.startsWith('/')) {
            return path;
        }
        return `/frontend/static/ai/${path}`;
    }

    function getEntryLabel(entryName) {
        return ENTRY_LABELS[entryName] || entryName || '当前模块';
    }

    function appendPlaceholderStyles() {
        if (document.getElementById('legacy-ai-entry-placeholder-style')) {
            return;
        }

        const style = document.createElement('style');
        style.id = 'legacy-ai-entry-placeholder-style';
        style.textContent = `
            .legacy-dev-placeholder {
                max-width: 760px;
                margin: 72px auto 0;
                padding: 28px;
                border: 1px solid rgba(59, 130, 246, 0.18);
                border-radius: 18px;
                background: linear-gradient(180deg, rgba(255, 255, 255, 0.98), rgba(248, 250, 252, 0.96));
                box-shadow: 0 20px 60px rgba(15, 23, 42, 0.10);
                color: #1f2937;
                font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Microsoft YaHei", sans-serif;
            }

            .legacy-dev-placeholder__eyebrow {
                margin: 0 0 8px;
                color: #2563eb;
                font-size: 12px;
                font-weight: 700;
                letter-spacing: 0.08em;
            }

            .legacy-dev-placeholder__title {
                margin: 0 0 10px;
                color: #0f172a;
                font-size: 24px;
                line-height: 1.35;
                font-weight: 700;
            }

            .legacy-dev-placeholder__message {
                margin: 0;
                color: #64748b;
                font-size: 14px;
                line-height: 1.8;
            }
        `;
        document.head.appendChild(style);
    }

    function notifyDevelopment(entryName) {
        const message = `${getEntryLabel(entryName)}功能正在开发中`;

        if (window.Toast && typeof window.Toast.show === 'function') {
            window.Toast.show('warning', message);
            return;
        }

        if (typeof window.showToast === 'function') {
            window.showToast(message, 'warning');
        }
    }

    function renderDevelopmentPlaceholder(host, entryName) {
        if (!(host instanceof HTMLElement)) {
            return;
        }

        appendPlaceholderStyles();

        const label = getEntryLabel(entryName);
        const placeholder = document.createElement('section');
        const eyebrow = document.createElement('p');
        const title = document.createElement('h1');
        const message = document.createElement('p');

        placeholder.className = 'legacy-dev-placeholder';
        placeholder.setAttribute('role', 'status');
        placeholder.setAttribute('aria-live', 'polite');

        eyebrow.className = 'legacy-dev-placeholder__eyebrow';
        eyebrow.textContent = 'LEGACY FRONTEND';

        title.className = 'legacy-dev-placeholder__title';
        title.textContent = '功能正在开发中';

        message.className = 'legacy-dev-placeholder__message';
        message.textContent = `${label} 的前端入口尚未发布到 legacy 静态资源中，后续版本补齐后可直接使用。`;

        placeholder.append(eyebrow, title, message);
        host.replaceChildren(placeholder);
        notifyDevelopment(entryName);
    }

    async function getManifest() {
        if (!manifestPromise) {
            manifestPromise = fetch(MANIFEST_URL, { credentials: 'same-origin' })
                .then((response) => {
                    if (!response.ok) {
                        throw new ManifestLoadError(response.status);
                    }
                    return response.json();
                })
                .catch((error) => {
                    manifestPromise = null;
                    throw error;
                });
        }
        return manifestPromise;
    }

    function resolveEntry(manifest, entryName) {
        const directKeys = [
            entryName,
            `${entryName}.js`,
            `src/entries/${entryName}.tsx`,
            `src/entries/${entryName}.ts`,
        ];
        for (const key of directKeys) {
            if (manifest[key]) {
                return manifest[key];
            }
        }
        const values = Object.entries(manifest);
        for (const [key, item] of values) {
            if (!item || typeof item !== 'object' || !item.isEntry) {
                continue;
            }
            const src = String(item.src || '');
            const file = String(item.file || '');
            if (key.includes(entryName) || src.includes(entryName) || file.includes(`${entryName}-`) || file.includes(`${entryName}.`)) {
                return item;
            }
        }
        return null;
    }

    function appendStyles(manifest, entry, visited = new Set()) {
        const id = String(entry && entry.file ? entry.file : '');
        if (id && visited.has(id)) {
            return;
        }
        if (id) {
            visited.add(id);
        }

        const imports = Array.isArray(entry && entry.imports) ? entry.imports : [];
        imports.forEach((importKey) => {
            const dep = manifest[importKey];
            if (dep) {
                appendStyles(manifest, dep, visited);
            }
        });

        const cssFiles = Array.isArray(entry && entry.css) ? entry.css : [];
        cssFiles.forEach((cssFile) => {
            const href = normalizePath(String(cssFile || ''));
            if (!href || loadedCss.has(href)) {
                return;
            }
            const link = document.createElement('link');
            link.rel = 'stylesheet';
            link.href = href;
            document.head.appendChild(link);
            loadedCss.add(href);
        });
    }

    async function loadEntry(entryName) {
        let manifest;
        try {
            manifest = await getManifest();
        } catch (error) {
            if (error instanceof ManifestLoadError && error.status === 404) {
                throw new EntryNotFoundError(entryName);
            }
            throw error;
        }

        const entry = resolveEntry(manifest, entryName);
        if (!entry || !entry.file) {
            throw new EntryNotFoundError(entryName);
        }

        appendStyles(manifest, entry);
        const moduleUrl = normalizePath(String(entry.file));
        if (!loadedJs.has(moduleUrl)) {
            await import(moduleUrl);
            loadedJs.add(moduleUrl);
        }
    }

    async function bootAllEntries() {
        const hosts = Array.from(document.querySelectorAll('[data-ai-entry]'));
        if (hosts.length <= 0) {
            return;
        }
        for (const host of hosts) {
            const entryName = String(host.getAttribute('data-ai-entry') || '').trim();
            if (!entryName) {
                continue;
            }
            const isOptional = host.getAttribute('data-ai-optional') !== 'false';
            try {
                await loadEntry(entryName);
                host.setAttribute('data-ai-loader', 'ready');
            } catch (error) {
                if (error instanceof EntryNotFoundError && isOptional) {
                    host.setAttribute('data-ai-loader', 'skipped');
                    renderDevelopmentPlaceholder(host, entryName);
                    console.debug(`[ai_entry_loader] Optional AI entry skipped: ${entryName}`);
                } else {
                    host.setAttribute('data-ai-loader', 'error');
                    console.error('[ai_entry_loader] load failed:', entryName, error);
                }
            }
        }
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', () => {
            bootAllEntries().catch((error) => {
                console.error('[ai_entry_loader] bootstrap failed', error);
            });
        });
    } else {
        bootAllEntries().catch((error) => {
            console.error('[ai_entry_loader] bootstrap failed', error);
        });
    }
})();
