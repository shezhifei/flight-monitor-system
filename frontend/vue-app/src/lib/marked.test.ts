// @vitest-environment jsdom

import { describe, expect, it } from 'vitest';
import { renderMarkdown } from './marked';

describe('renderMarkdown', () => {
  it('keeps markdown formatting while removing script tags', () => {
    const html = renderMarkdown('**safe**<script>alert("xss")</script>');

    expect(html).toContain('<strong>safe</strong>');
    expect(html).not.toContain('<script');
    expect(html).not.toContain('alert("xss")');
  });

  it('removes image event handler attributes', () => {
    const html = renderMarkdown('<img src="x" onerror="alert(1)">');

    expect(html).toContain('<img');
    expect(html).not.toContain('onerror');
    expect(html).not.toContain('alert(1)');
  });

  it('removes javascript URLs from links', () => {
    const html = renderMarkdown('[open](javascript:alert(1))');

    expect(html).toContain('open');
    expect(html).not.toContain('javascript:');
    expect(html).not.toContain('alert(1)');
  });

  it('removes style elements, inline styles, and data attributes', () => {
    const html = renderMarkdown('<style>body{display:none}</style><span style="color:red" data-secret="x">safe</span>');

    expect(html).toContain('safe');
    expect(html).not.toContain('<style');
    expect(html).not.toContain('display:none');
    expect(html).not.toContain('style=');
    expect(html).not.toContain('data-secret');
  });

  it('removes protocol-relative and data URLs', () => {
    const html = renderMarkdown('[cdn](//evil.example/x)\n\n![pixel](data:image/svg+xml;base64,PHN2ZyBvbmxvYWQ9YWxlcnQoMSk+)');

    expect(html).toContain('cdn');
    expect(html).toContain('pixel');
    expect(html).not.toContain('//evil.example');
    expect(html).not.toContain('data:image');
    expect(html).not.toContain('onload');
  });
});
