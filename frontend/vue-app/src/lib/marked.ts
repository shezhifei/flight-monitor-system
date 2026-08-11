import { Marked, type MarkedOptions } from 'marked';
import DOMPurify from 'dompurify';
import { setPurify, sanitizeHtml } from '../../../shared/security/markdown-sanitize-config.js';

setPurify(DOMPurify);

export type MarkdownOptions = Omit<MarkedOptions<string, string>, 'async'>;

const defaultMarkdownOptions: MarkdownOptions = {
  gfm: true,
};

export function createMarked(options: MarkdownOptions = {}): Marked<string, string> {
  const instance = new Marked<string, string>();

  instance.setOptions({
    ...defaultMarkdownOptions,
    ...options,
  });

  return instance;
}

export function renderMarkdown(markdown: string, options: MarkdownOptions = {}): string {
  return safeMarkdown(markdown, options);
}

export function safeMarkdown(markdown: string, options: MarkdownOptions = {}): string {
  const rendered = createMarked(options).parse(markdown || '', { async: false });

  return sanitizeHtml(rendered);
}
