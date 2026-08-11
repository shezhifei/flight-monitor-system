(function (global) {
    'use strict';

    var sanitizeConfig = require('../../shared/security/markdown-sanitize-config.js');
    var sanitizeHtml = sanitizeConfig.sanitizeHtml;

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
            return sanitizeHtml(rendered);
        }

        return escapeHtml(source).replace(/\r?\n/g, '<br>');
    }

    global.FMSSecurity = global.FMSSecurity || {};
    global.FMSSecurity.renderMarkdownSafe = renderMarkdownSafe;
})(window);
