'use strict';

var SAFE_URI_PATTERN = /^(?:(?:https?|mailto|tel):|\/(?!\/)|#|\.{1,2}\/|[a-z0-9._~-][^:/?#]*(?:[/?#]|$))/i;
var URL_ATTRS = ['href', 'src', 'xlink:href'];

var PURIFY_CONFIG = {
    USE_PROFILES: { html: true },
    ALLOW_DATA_ATTR: false,
    FORBID_TAGS: ['style', 'script', 'iframe', 'object', 'embed', 'form', 'input', 'button'],
    FORBID_ATTR: ['style', 'srcset'],
    ALLOWED_URI_REGEXP: SAFE_URI_PATTERN
};

var ALLOWED_PROTOCOLS = ['https', 'http', 'mailto', 'tel'];

var _purifyInstance = null;
var _hookRegistered = false;

function setPurify(purify) {
    _purifyInstance = purify;
}

function _getPurify() {
    if (_purifyInstance) return _purifyInstance;
    if (typeof window !== 'undefined' && window.DOMPurify) return window.DOMPurify;
    if (typeof global !== 'undefined' && global.DOMPurify) return global.DOMPurify;
    return null;
}

function _isSafeUri(value) {
    return SAFE_URI_PATTERN.test(value.trim());
}

function _escapeHtml(value) {
    return String(value || '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function sanitizeHtml(html) {
    var purify = _getPurify();
    if (!purify || typeof purify.sanitize !== 'function') {
        // Fail-closed: escape HTML instead of returning raw unsanitized content
        return _escapeHtml(html);
    }

    if (!_hookRegistered) {
        purify.addHook('afterSanitizeAttributes', function (node) {
            if (!node || typeof node.getAttribute !== 'function') return;
            for (var i = 0; i < URL_ATTRS.length; i++) {
                var value = node.getAttribute(URL_ATTRS[i]);
                if (value && !_isSafeUri(value)) {
                    node.removeAttribute(URL_ATTRS[i]);
                }
            }
        });
        _hookRegistered = true;
    }

    return purify.sanitize(html, PURIFY_CONFIG);
}

module.exports = {
    PURIFY_CONFIG: PURIFY_CONFIG,
    ALLOWED_PROTOCOLS: ALLOWED_PROTOCOLS,
    setPurify: setPurify,
    sanitizeHtml: sanitizeHtml
};
