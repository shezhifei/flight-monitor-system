(function (global) {
    'use strict';

    // 共享配置已改为 ESM（vue-app 消费）；本文件仅被归档 legacy 页引用，
    // 浏览器里 require 本就不可用，这里 fail-closed 退化为纯转义。
    function escapeHtml(value) {
        return String(value || '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    function renderMarkdownSafe(markdown) {
        var source = String(markdown || '');
        var rendered = global.marked && typeof global.marked.parse === 'function'
            ? global.marked.parse(source)
            : escapeHtml(source).replace(/\r?\n/g, '<br>');

        if (global.DOMPurify && typeof global.DOMPurify.sanitize === 'function') {
            return global.DOMPurify.sanitize(rendered);
        }

        return escapeHtml(source).replace(/\r?\n/g, '<br>');
    }

    global.FMSSecurity = global.FMSSecurity || {};
    global.FMSSecurity.renderMarkdownSafe = renderMarkdownSafe;
})(window);
