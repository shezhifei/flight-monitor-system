(function attachLoadingComponent(window, document) {
    'use strict';

    if (!window || !document || window.Loading) {
        return;
    }

    const FULLSCREEN_OVERLAY_ID = 'loadingOverlay';
    const FULLSCREEN_TITLE_SELECTOR = '[data-loading-role="title"]';
    const FULLSCREEN_MESSAGE_SELECTOR = '[data-loading-role="message"]';
    const SKELETON_MIN_HEIGHT = 'calc(var(--spacing-xl) * 3)';
    const DEFAULTS = {
        fullscreen: {
            title: '请稍候',
            message: '正在加载数据',
        },
        skeleton: {
            message: '正在加载内容',
            lines: 4,
        },
        button: {
            label: '处理中...',
        },
    };

    const targetStates = new WeakMap();
    let fullscreenState = null;
    let fullscreenHideTimer = 0;

    function normalizeMode(mode) {
        const normalized = String(mode || '').trim().toLowerCase();
        if (normalized === 'button' || normalized === 'skeleton') {
            return normalized;
        }
        return 'fullscreen';
    }

    function normalizeText(value, fallback) {
        const normalized = String(value == null ? '' : value).trim();
        return normalized || fallback;
    }

    function normalizeLineCount(value) {
        const numeric = Number(value);
        if (!Number.isFinite(numeric)) {
            return DEFAULTS.skeleton.lines;
        }
        return Math.min(6, Math.max(2, Math.round(numeric)));
    }

    function normalizeCssLength(value, fallback) {
        if (typeof value === 'number' && Number.isFinite(value) && value >= 0) {
            return `${value}px`;
        }
        if (typeof value === 'string' && value.trim()) {
            return value.trim();
        }
        return fallback;
    }

    function resolveTarget(target) {
        if (target instanceof HTMLElement) {
            return target;
        }
        if (typeof target === 'string' && target.trim()) {
            return document.querySelector(target.trim());
        }
        return null;
    }

    function restoreAttribute(element, name, snapshot) {
        if (!(element instanceof HTMLElement)) {
            return;
        }
        if (snapshot && snapshot.hadAttribute) {
            element.setAttribute(name, snapshot.value);
            return;
        }
        element.removeAttribute(name);
    }

    function snapshotAttribute(element, name) {
        return {
            hadAttribute: element.hasAttribute(name),
            value: element.getAttribute(name) || '',
        };
    }

    function setBodyBusy(isBusy) {
        if (!document.body) {
            return;
        }

        if (!fullscreenState) {
            fullscreenState = {
                count: 0,
                previousBusy: snapshotAttribute(document.body, 'aria-busy'),
            };
        }

        if (isBusy) {
            if (fullscreenState.count <= 1) {
                fullscreenState.previousBusy = snapshotAttribute(document.body, 'aria-busy');
            }
            document.body.setAttribute('aria-busy', 'true');
            return;
        }

        restoreAttribute(document.body, 'aria-busy', fullscreenState.previousBusy);
    }

    function createIndicator(modifierClass) {
        const indicator = document.createElement('span');
        indicator.className = modifierClass ? `loading-indicator ${modifierClass}` : 'loading-indicator';
        indicator.setAttribute('aria-hidden', 'true');
        return indicator;
    }

    function createFullscreenOverlay() {
        let overlay = document.getElementById(FULLSCREEN_OVERLAY_ID);
        if (overlay) {
            return overlay;
        }

        overlay = document.createElement('section');
        overlay.id = FULLSCREEN_OVERLAY_ID;
        overlay.className = 'loading-overlay';
        overlay.hidden = true;
        overlay.setAttribute('aria-hidden', 'true');

        const surface = document.createElement('div');
        surface.className = 'loading-overlay__surface';
        surface.setAttribute('role', 'status');
        surface.setAttribute('aria-live', 'polite');
        surface.setAttribute('aria-atomic', 'true');

        const copy = document.createElement('div');
        copy.className = 'loading-overlay__copy';

        const title = document.createElement('p');
        title.className = 'loading-overlay__title';
        title.dataset.loadingRole = 'title';

        const message = document.createElement('p');
        message.className = 'loading-overlay__message';
        message.dataset.loadingRole = 'message';

        copy.append(title, message);
        surface.append(createIndicator('loading-indicator--overlay'), copy);
        overlay.appendChild(surface);
        document.body.appendChild(overlay);
        return overlay;
    }

    function updateFullscreenCopy(overlay, options) {
        const title = overlay.querySelector(FULLSCREEN_TITLE_SELECTOR);
        const message = overlay.querySelector(FULLSCREEN_MESSAGE_SELECTOR);
        if (title) {
            title.textContent = normalizeText(options.title, DEFAULTS.fullscreen.title);
        }
        if (message) {
            message.textContent = normalizeText(options.message, DEFAULTS.fullscreen.message);
        }
    }

    function showFullscreen(options) {
        const overlay = createFullscreenOverlay();
        if (!fullscreenState) {
            fullscreenState = { count: 0, previousBusy: snapshotAttribute(document.body, 'aria-busy') };
        }

        window.clearTimeout(fullscreenHideTimer);
        updateFullscreenCopy(overlay, options);
        fullscreenState.count += 1;
        setBodyBusy(true);

        overlay.hidden = false;
        overlay.setAttribute('aria-hidden', 'false');
        window.requestAnimationFrame(() => {
            overlay.classList.add('is-visible');
        });

        return overlay;
    }

    function hideFullscreen(force) {
        const overlay = document.getElementById(FULLSCREEN_OVERLAY_ID);
        if (!overlay || !fullscreenState) {
            return false;
        }

        if (force) {
            fullscreenState.count = 0;
        } else {
            fullscreenState.count = Math.max(0, fullscreenState.count - 1);
        }

        if (fullscreenState.count > 0) {
            return true;
        }

        overlay.classList.remove('is-visible');
        overlay.setAttribute('aria-hidden', 'true');
        setBodyBusy(false);
        fullscreenHideTimer = window.setTimeout(() => {
            overlay.hidden = true;
        }, 160);
        return true;
    }

    function buildSkeletonVisual(message, lineCount) {
        const wrapper = document.createElement('div');
        wrapper.className = 'loading-skeleton';

        const status = document.createElement('div');
        status.className = 'loading-skeleton__status';
        status.append(createIndicator('loading-indicator--skeleton'));

        const text = document.createElement('span');
        text.textContent = message;
        status.appendChild(text);

        const lines = document.createElement('div');
        lines.className = 'loading-skeleton__lines';

        const widths = [
            'loading-skeleton__line--title',
            'loading-skeleton__line--wide',
            'loading-skeleton__line--medium',
            'loading-skeleton__line--compact',
        ];

        for (let index = 0; index < lineCount; index += 1) {
            const line = document.createElement('span');
            line.className = `loading-skeleton__line ${widths[index % widths.length]}`;
            line.setAttribute('aria-hidden', 'true');
            lines.appendChild(line);
        }

        wrapper.append(status, lines);
        return wrapper;
    }

    function createSkeletonState(target, options) {
        const layer = document.createElement('div');
        const message = normalizeText(options.message, DEFAULTS.skeleton.message);
        const lineCount = normalizeLineCount(options.lines);
        const height = target.getBoundingClientRect().height;
        const restore = {
            ariaBusy: snapshotAttribute(target, 'aria-busy'),
            position: target.style.position,
            minHeight: target.style.minHeight,
        };

        layer.className = 'loading-skeleton-layer';
        layer.setAttribute('role', 'status');
        layer.setAttribute('aria-live', 'polite');
        layer.setAttribute('aria-atomic', 'true');
        layer.setAttribute('aria-label', message);
        layer.appendChild(buildSkeletonVisual(message, lineCount));

        if (window.getComputedStyle(target).position === 'static') {
            target.style.position = 'relative';
        }

        if (!target.style.minHeight && height < 1) {
            target.style.minHeight = normalizeCssLength(options.minHeight, SKELETON_MIN_HEIGHT);
        }

        target.classList.add('loading-host', 'is-loading-skeleton');
        target.setAttribute('aria-busy', 'true');
        target.appendChild(layer);

        return {
            mode: 'skeleton',
            count: 1,
            update(nextOptions) {
                const nextMessage = normalizeText(nextOptions.message, DEFAULTS.skeleton.message);
                layer.setAttribute('aria-label', nextMessage);
                layer.replaceChildren(buildSkeletonVisual(nextMessage, normalizeLineCount(nextOptions.lines)));
            },
            teardown() {
                if (layer.parentNode === target) {
                    target.removeChild(layer);
                }
                target.classList.remove('loading-host', 'is-loading-skeleton');
                restoreAttribute(target, 'aria-busy', restore.ariaBusy);
                target.style.position = restore.position;
                target.style.minHeight = restore.minHeight;
            },
        };
    }

    function isButtonInput(target) {
        if (!(target instanceof HTMLInputElement)) {
            return false;
        }
        const type = String(target.type || '').trim().toLowerCase();
        return type === 'button' || type === 'submit' || type === 'reset';
    }

    function buildButtonContent(label) {
        const content = document.createElement('span');
        content.className = 'loading-button__content';

        const indicator = createIndicator('loading-indicator--button');
        const text = document.createElement('span');
        text.className = 'loading-button__label';
        text.textContent = label;

        content.append(indicator, text);
        return content;
    }

    function createButtonState(target, options) {
        const label = normalizeText(options.label || options.message, DEFAULTS.button.label);
        const restore = {
            ariaBusy: snapshotAttribute(target, 'aria-busy'),
            ariaDisabled: snapshotAttribute(target, 'aria-disabled'),
            ariaLabel: snapshotAttribute(target, 'aria-label'),
            disabled: 'disabled' in target ? Boolean(target.disabled) : null,
            minWidth: target.style.minWidth,
            wasFocused: document.activeElement === target,
        };

        const measuredWidth = Math.ceil(target.getBoundingClientRect().width);
        target.classList.add('is-loading-button');
        target.style.minWidth = measuredWidth > 0 ? `${measuredWidth}px` : target.style.minWidth;
        target.setAttribute('aria-busy', 'true');
        target.setAttribute('aria-disabled', 'true');
        target.setAttribute('aria-label', label);

        if ('disabled' in target) {
            target.disabled = true;
        }

        if (isButtonInput(target)) {
            const originalValue = target.value;
            target.value = label;

            return {
                mode: 'button',
                count: 1,
                update(nextOptions) {
                    const nextLabel = normalizeText(nextOptions.label || nextOptions.message, DEFAULTS.button.label);
                    target.value = nextLabel;
                    target.setAttribute('aria-label', nextLabel);
                },
                teardown() {
                    target.classList.remove('is-loading-button');
                    target.value = originalValue;
                    restoreAttribute(target, 'aria-busy', restore.ariaBusy);
                    restoreAttribute(target, 'aria-disabled', restore.ariaDisabled);
                    restoreAttribute(target, 'aria-label', restore.ariaLabel);
                    target.style.minWidth = restore.minWidth;
                    if ('disabled' in target && restore.disabled !== null) {
                        target.disabled = restore.disabled;
                    }
                    if (restore.wasFocused && target.isConnected) {
                        target.focus({ preventScroll: true });
                    }
                },
            };
        }

        const originalNodes = Array.from(target.childNodes);
        target.replaceChildren(buildButtonContent(label));

        return {
            mode: 'button',
            count: 1,
            update(nextOptions) {
                const nextLabel = normalizeText(nextOptions.label || nextOptions.message, DEFAULTS.button.label);
                target.replaceChildren(buildButtonContent(nextLabel));
                target.setAttribute('aria-label', nextLabel);
            },
            teardown() {
                target.classList.remove('is-loading-button');
                target.replaceChildren(...originalNodes);
                restoreAttribute(target, 'aria-busy', restore.ariaBusy);
                restoreAttribute(target, 'aria-disabled', restore.ariaDisabled);
                restoreAttribute(target, 'aria-label', restore.ariaLabel);
                target.style.minWidth = restore.minWidth;
                if ('disabled' in target && restore.disabled !== null) {
                    target.disabled = restore.disabled;
                }
                if (restore.wasFocused && target.isConnected) {
                    target.focus({ preventScroll: true });
                }
            },
        };
    }

    function setTargetState(target, nextState) {
        const existing = targetStates.get(target);
        if (existing) {
            existing.teardown();
        }
        targetStates.set(target, nextState);
        return target;
    }

    function showTargetState(mode, options) {
        const target = resolveTarget(options.target);
        if (!target) {
            return null;
        }

        const existing = targetStates.get(target);
        if (existing && existing.mode === mode) {
            existing.count += 1;
            existing.update(options);
            return target;
        }

        if (mode === 'button') {
            return setTargetState(target, createButtonState(target, options));
        }

        return setTargetState(target, createSkeletonState(target, options));
    }

    function hideTargetState(target, force) {
        const state = targetStates.get(target);
        if (!state) {
            return false;
        }

        if (force) {
            state.count = 0;
        } else {
            state.count = Math.max(0, state.count - 1);
        }

        if (state.count > 0) {
            return true;
        }

        state.teardown();
        targetStates.delete(target);
        return true;
    }

    function show(options = {}) {
        const mode = normalizeMode(options.mode);
        if (mode === 'fullscreen') {
            return showFullscreen(options);
        }
        return showTargetState(mode, options);
    }

    function hide(target) {
        if (target == null) {
            return hideFullscreen(false);
        }

        if (target === FULLSCREEN_OVERLAY_ID) {
            return hideFullscreen(true);
        }

        const resolved = resolveTarget(target);
        if (resolved && resolved.id === FULLSCREEN_OVERLAY_ID) {
            return hideFullscreen(true);
        }

        if (!resolved) {
            return false;
        }

        return hideTargetState(resolved, false);
    }

    window.Loading = {
        show,
        hide,
    };
}(window, document));
