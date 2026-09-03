/**
 * Unified Breadcrumb Component — Flight Monitor System
 *
 * Usage:
 *   window.Breadcrumb.render('#breadcrumb-host', [
 *     { label: '工作台', href: '/frontend/html/dashboard.html' },
 *     { label: '航班监控', current: true }
 *   ]);
 */
(function () {
    'use strict';

    var SVG_SEPARATOR = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 18l6-6-6-6"/></svg>';

    /**
     * Render breadcrumb into target container.
     * @param {string|HTMLElement} container
     * @param {Array<{label:string, href?:string, current?:boolean}>} items
     */
    function render(container, items) {
        var el = typeof container === 'string' ? document.querySelector(container) : container;
        if (!el) {
            console.warn('[Breadcrumb] Container not found:', container);
            return null;
        }

        var normalizedItems = Array.isArray(items) ? items : [];
        if (!normalizedItems.length) {
            el.innerHTML = '';
            return null;
        }

        var html = [
            '<nav class="unified-breadcrumb" aria-label="面包屑导航">',
            '<ol class="unified-breadcrumb__list">'
        ];

        for (var i = 0; i < normalizedItems.length; i++) {
            var item = normalizedItems[i] || {};
            var label = escapeHtml(String(item.label || '').trim() || '未命名页面');
            var href = typeof item.href === 'string' ? item.href.trim() : '';
            var isLast = i === normalizedItems.length - 1;
            var isCurrent = Boolean(item.current) || isLast;

            html.push('<li class="unified-breadcrumb__item">');
            if (!isCurrent && href) {
                html.push('<a class="unified-breadcrumb__link" href="' + escapeAttr(href) + '">' + label + '</a>');
            } else {
                html.push('<span class="unified-breadcrumb__current"' + (isCurrent ? ' aria-current="page"' : '') + '>' + label + '</span>');
            }
            html.push('</li>');

            if (!isLast) {
                html.push('<li class="unified-breadcrumb__separator" aria-hidden="true">' + SVG_SEPARATOR + '</li>');
            }
        }

        html.push('</ol>');
        html.push('</nav>');

        el.innerHTML = html.join('');
        return el.querySelector('.unified-breadcrumb');
    }

    function escapeHtml(str) {
        var div = document.createElement('div');
        div.appendChild(document.createTextNode(String(str)));
        return div.innerHTML;
    }

    function escapeAttr(str) {
        return String(str)
            .replace(/&/g, '&amp;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');
    }

    if (typeof window.Breadcrumb === 'undefined') {
        window.Breadcrumb = {};
    }
    window.Breadcrumb.render = render;
})();
