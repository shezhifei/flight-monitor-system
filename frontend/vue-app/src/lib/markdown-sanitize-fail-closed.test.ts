// @vitest-environment jsdom

/**
 * Task 6 (P0): Markdown sanitize fail-closed
 *
 * When DOMPurify is unavailable, sanitizeHtml must NOT return raw HTML.
 * Instead, it must escape HTML entities to prevent XSS.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

interface SanitizeConfig {
  sanitizeHtml: (html: string) => string;
  setPurify: (purify: unknown) => void;
}

interface WindowWithDompurify extends Window {
  DOMPurify?: unknown;
}

describe('sanitizeHtml fail-closed', () => {
  let sanitizeConfig: SanitizeConfig;

  beforeEach(async () => {
    vi.resetModules();
    delete (window as WindowWithDompurify).DOMPurify;
    sanitizeConfig = await import('../../../shared/security/markdown-sanitize-config.js') as SanitizeConfig;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('escapes HTML when DOMPurify is unavailable instead of returning raw HTML', () => {
    // setPurify was NOT called, window.DOMPurify is undefined
    const maliciousHtml = '<script>alert("xss")</script>';
    const result = sanitizeConfig.sanitizeHtml(maliciousHtml);

    // Must NOT contain raw script tag
    expect(result).not.toContain('<script>');
    expect(result).not.toContain('alert("xss")');
    // Must contain escaped HTML entities
    expect(result).toContain('&lt;');
    expect(result).toContain('&gt;');
  });

  it('escapes HTML tags when DOMPurify is unavailable', () => {
    const html = '<img src="x" onerror="alert(1)">';
    const result = sanitizeConfig.sanitizeHtml(html);

    // Must NOT contain raw img tag (angle brackets must be escaped)
    expect(result).not.toContain('<img');
    // Must be escaped - the tag becomes text
    expect(result).toContain('&lt;img');
    // Angle brackets are escaped, so it's safe text, not executable HTML
    expect(result).not.toMatch(/<img\s/);
  });

  it('escapes dangerous content even when setPurify is called with null', () => {
    sanitizeConfig.setPurify(null);
    const html = '<iframe src="javascript:alert(1)"></iframe>';
    const result = sanitizeConfig.sanitizeHtml(html);

    // Angle brackets must be escaped so it's not executable HTML
    expect(result).not.toContain('<iframe');
    expect(result).toContain('&lt;iframe');
    // The entire thing is escaped text, not a real tag
    expect(result).not.toMatch(/<iframe\s/);
  });

  it('escapes angle brackets and ampersands', () => {
    const html = '<div data-x="1">&amp;</div>';
    const result = sanitizeConfig.sanitizeHtml(html);

    expect(result).not.toContain('<div');
    expect(result).toContain('&lt;div');
    expect(result).toContain('&amp;');
  });

  it('still sanitizes correctly when DOMPurify is available', async () => {
    // Re-import with DOMPurify available
    vi.resetModules();
    const dompurify = (await import('dompurify')).default;

    const config = await import('../../../shared/security/markdown-sanitize-config.js');
    config.setPurify(dompurify);

    // Pass HTML (not markdown) - sanitizeHtml sanitizes HTML, not markdown
    const html = '<script>alert("xss")</script><strong>safe</strong>';
    const result = config.sanitizeHtml(html);

    expect(result).not.toContain('<script');
    expect(result).toContain('<strong>safe</strong>');
  });
});
