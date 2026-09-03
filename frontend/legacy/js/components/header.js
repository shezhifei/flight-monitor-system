/**
 * Unified Header Component — Flight Monitor System
 *
 * Usage:
 *   window.Header.render('#header-host', {
 *     title: '航班监控',
 *     subtitle: '实时状态',          // optional
 *     showBack: true,
 *     backHref: '/frontend/html/dashboard.html',
 *     user: { username, role },
 *     actions: [ { label, onClick, className } ],  // optional right-side buttons
 *     extraLeft: htmlString,                        // optional tabs/content after title
 *     extraRight: htmlString,                       // optional chips/content before user
 *     onLogout: function                            // optional, defaults to window.logout
 *   });
 */
(function () {
    'use strict';

    var SVG_PLANE = '<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M17.8 19.2L16 11l3.5-3.5C21 6 21.5 4 21 3c-1-.5-3 0-4.5 1.5L13 8 4.8 6.2c-.5-.1-.9.1-1.1.5l-.3.5c-.2.5-.1 1 .3 1.3L9 12l-2 3H4l-1 1 3 2 2 3 1-1v-3l3-2 3.5 5.3c.3.4.8.5 1.3.3l.5-.2c.4-.3.6-.7.5-1.2z"/></svg>';
    var SVG_BACK = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M19 12H5"/><path d="M12 19l-7-7 7-7"/></svg>';
    var SVG_LOGOUT = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>';

    /**
     * Render the unified header into a container element.
     * @param {string|HTMLElement} container - CSS selector or DOM element
     * @param {Object} options
     */
    function render(container, options) {
        options = options || {};
        var el = typeof container === 'string' ? document.querySelector(container) : container;
        if (!el) {
            console.warn('[Header] Container not found:', container);
            return;
        }

        var title = options.title || '';
        var subtitle = options.subtitle || '';
        var showBack = !!options.showBack;
        var backHref = options.backHref || '/frontend/html/dashboard.html';
        var user = options.user || null;
        var actions = options.actions || [];
        var extraLeft = options.extraLeft || '';
        var extraRight = options.extraRight || '';
        var onLogout = options.onLogout || (typeof window.logout === 'function' ? window.logout : null);

        // Build class list
        var classes = ['unified-header'];
        if (showBack) classes.push('unified-header--with-back');
        if (extraLeft) classes.push('unified-header--has-extra-left');
        if (extraRight) classes.push('unified-header--has-extra-right');
        if (actions.length) classes.push('unified-header--has-actions');

        // Brand logo
        var brandHtml = '<a class="unified-header__brand" href="/frontend/html/dashboard.html" title="返回工作台">' +
            '<span class="unified-header__brand-icon">' + SVG_PLANE + '</span>' +
            '<span class="unified-header__brand-text">航班监控</span>' +
            '</a>';

        // Back button
        var backHtml = showBack
            ? '<a class="unified-header__back" href="' + escapeAttr(backHref) + '" title="返回工作台">' + SVG_BACK + '</a>'
            : '';

        // Title area
        var titleHtml = '<div class="unified-header__title-group">' +
            '<h1 class="unified-header__title">' + escapeHtml(title) + '</h1>' +
            (subtitle ? '<span class="unified-header__subtitle">' + escapeHtml(subtitle) + '</span>' : '') +
            '</div>';

        // Left section: back + brand + title + extraLeft
        var leftHtml = '<div class="unified-header__left">' +
            backHtml +
            brandHtml +
            titleHtml +
            (extraLeft ? '<div class="unified-header__extra-left">' + extraLeft + '</div>' : '') +
            '</div>';

        // Right section: extraRight + actions + user + logout
        var rightParts = [];
        if (extraRight) {
            rightParts.push('<div class="unified-header__extra-right">' + extraRight + '</div>');
        }
        if (actions.length) {
            rightParts.push('<div class="unified-header__actions">');
            for (var i = 0; i < actions.length; i++) {
                var a = actions[i];
                var cls = 'unified-header__action-btn' + (a.className ? ' ' + a.className : '');
                rightParts.push('<button type="button" class="' + cls + '" data-action="' + escapeAttr(a.label || '') + '">' + escapeHtml(a.label) + '</button>');
            }
            rightParts.push('</div>');
        }
        if (user) {
            var displayName = user.effective_operator_label || user.username || user.name || '用户';
            var displayRole = user.role || user.roles ? (Array.isArray(user.roles) ? user.roles.join(', ') : user.roles) : '';
            rightParts.push(
                '<div class="unified-header__user-pill">' +
                '<span class="unified-header__user-name">' + escapeHtml(displayName) + '</span>' +
                (displayRole ? '<span class="unified-header__user-role">' + escapeHtml(displayRole) + '</span>' : '') +
                '</div>'
            );
        }
        if (onLogout) {
            rightParts.push(
                '<button type="button" class="unified-header__logout-btn" id="unifiedHeaderLogout">' +
                SVG_LOGOUT +
                '<span>退出</span>' +
                '</button>'
            );
        }

        var rightHtml = '<div class="unified-header__right">' + rightParts.join('') + '</div>';

        el.innerHTML = '<header class="' + classes.join(' ') + '" role="banner">' +
            leftHtml +
            rightHtml +
            '</header>';

        // Bind logout
        var logoutBtn = el.querySelector('#unifiedHeaderLogout');
        if (logoutBtn && onLogout) {
            logoutBtn.addEventListener('click', function (e) {
                e.preventDefault();
                onLogout();
            });
        }

        // Bind action buttons
        var actionBtns = el.querySelectorAll('.unified-header__action-btn');
        for (var j = 0; j < actionBtns.length; j++) {
            (function (btn, idx) {
                btn.addEventListener('click', function () {
                    if (actions[idx] && typeof actions[idx].onClick === 'function') {
                        actions[idx].onClick();
                    }
                });
            })(actionBtns[j], j);
        }

        return el.querySelector('header');
    }

    /** Escape HTML entities */
    function escapeHtml(str) {
        var div = document.createElement('div');
        div.appendChild(document.createTextNode(str));
        return div.innerHTML;
    }

    /** Escape attribute value */
    function escapeAttr(str) {
        return String(str).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/'/g, '&#39;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    // Expose globally
    if (typeof window.Header === 'undefined') {
        window.Header = {};
    }
    window.Header.render = render;
})();
