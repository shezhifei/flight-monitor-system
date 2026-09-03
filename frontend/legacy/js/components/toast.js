(function attachToastComponent(window, document) {
    'use strict';

    if (!window || !document || window.Toast) {
        return;
    }

    const TOAST_REGION_ID = 'toastRegion';
    const DEFAULT_DURATION_MS = 4200;
    const EXIT_DURATION_MS = 180;
    const TYPE_META = {
        success: {
            title: '成功',
            role: 'status',
        },
        error: {
            title: '错误',
            role: 'alert',
        },
        warning: {
            title: '提示',
            role: 'alert',
        },
        info: {
            title: '通知',
            role: 'status',
        },
    };

    let toastSequence = 0;

    function getRegion() {
        let region = document.getElementById(TOAST_REGION_ID);

        if (!region) {
            region = document.createElement('section');
            region.id = TOAST_REGION_ID;
            region.className = 'toast-region';
            region.setAttribute('aria-label', '系统通知');
            region.setAttribute('aria-live', 'polite');
            region.setAttribute('aria-relevant', 'additions removals');
            document.body.appendChild(region);
        }

        return region;
    }

    function normalizeType(type) {
        const normalized = String(type || '').trim().toLowerCase();
        return TYPE_META[normalized] ? normalized : 'info';
    }

    function normalizeMessage(message) {
        return String(message == null ? '' : message).trim();
    }

    function normalizeDuration(duration) {
        const numericDuration = Number(duration);

        if (!Number.isFinite(numericDuration)) {
            return DEFAULT_DURATION_MS;
        }

        return Math.max(0, numericDuration);
    }

    function buildToast(type, message, options) {
        const meta = TYPE_META[type];
        const toast = document.createElement('article');
        const content = document.createElement('div');
        const header = document.createElement('div');
        const title = document.createElement('p');
        const messageNode = document.createElement('p');
        const closeButton = document.createElement('button');

        toastSequence += 1;
        toast.className = `toast toast--${type}`;
        toast.dataset.toastId = `toast-${Date.now()}-${toastSequence}`;
        toast.setAttribute('role', meta.role);
        toast.setAttribute('aria-atomic', 'true');
        toast.tabIndex = -1;

        content.className = 'toast__content';
        header.className = 'toast__header';
        title.className = 'toast__title';
        title.textContent = options.title ? String(options.title).trim() || meta.title : meta.title;

        messageNode.className = 'toast__message';
        messageNode.textContent = message;

        closeButton.type = 'button';
        closeButton.className = 'toast__close';
        closeButton.setAttribute('aria-label', '关闭通知');
        closeButton.textContent = '×';

        header.append(title, closeButton);
        content.append(header, messageNode);
        toast.appendChild(content);

        return {
            toast,
            closeButton,
        };
    }

    function clearDismissTimer(toast) {
        const timerId = Number(toast.dataset.timerId || '0');

        if (timerId) {
            window.clearTimeout(timerId);
            delete toast.dataset.timerId;
        }
    }

    function dismissToast(toast) {
        if (!toast || toast.dataset.closing === 'true') {
            return;
        }

        toast.dataset.closing = 'true';
        clearDismissTimer(toast);
        toast.classList.add('toast--closing');

        window.setTimeout(() => {
            if (toast.parentNode) {
                toast.parentNode.removeChild(toast);
            }
        }, EXIT_DURATION_MS);
    }

    function startDismissTimer(toast, durationMs) {
        clearDismissTimer(toast);

        if (toast.dataset.persistent === 'true' || durationMs <= 0) {
            return;
        }

        toast.dataset.remainingMs = String(durationMs);
        toast.dataset.timerStartedAt = String(Date.now());
        toast.dataset.timerId = String(window.setTimeout(() => dismissToast(toast), durationMs));
    }

    function pauseDismissTimer(toast) {
        if (!toast || toast.dataset.persistent === 'true') {
            return;
        }

        const startedAt = Number(toast.dataset.timerStartedAt || '0');
        const remainingMs = Number(toast.dataset.remainingMs || '0');

        if (!startedAt || !remainingMs) {
            return;
        }

        const elapsedMs = Math.max(0, Date.now() - startedAt);
        const nextRemainingMs = Math.max(0, remainingMs - elapsedMs);

        clearDismissTimer(toast);
        toast.dataset.remainingMs = String(nextRemainingMs);
    }

    function resumeDismissTimer(toast) {
        if (!toast || toast.dataset.persistent === 'true') {
            return;
        }

        const remainingMs = Number(toast.dataset.remainingMs || '0');

        if (!remainingMs) {
            dismissToast(toast);
            return;
        }

        startDismissTimer(toast, remainingMs);
    }

    function show(type, message, options = {}) {
        const normalizedMessage = normalizeMessage(message);

        if (!normalizedMessage) {
            return null;
        }

        const normalizedType = normalizeType(type);
        const persistent = Boolean(options.persistent);
        const durationMs = persistent ? 0 : normalizeDuration(options.duration);
        const region = getRegion();
        const { toast, closeButton } = buildToast(normalizedType, normalizedMessage, options);

        toast.dataset.persistent = persistent ? 'true' : 'false';
        closeButton.addEventListener('click', () => dismissToast(toast));

        toast.addEventListener('mouseenter', () => pauseDismissTimer(toast));
        toast.addEventListener('mouseleave', () => resumeDismissTimer(toast));
        toast.addEventListener('focusin', () => pauseDismissTimer(toast));
        toast.addEventListener('focusout', (event) => {
            if (!toast.contains(event.relatedTarget)) {
                resumeDismissTimer(toast);
            }
        });

        region.prepend(toast);
        startDismissTimer(toast, durationMs);

        return toast.dataset.toastId;
    }

    window.Toast = {
        show,
    };
}(window, document));
