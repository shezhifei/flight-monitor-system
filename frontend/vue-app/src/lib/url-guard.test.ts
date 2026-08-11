import { describe, it, expect } from 'vitest';
import { assertSameOriginHttpUrl, assertStaticAssetUrl } from './url-guard';

describe('url-guard', () => {
  describe('assertSameOriginHttpUrl', () => {
    it('accepts same-origin relative URL', () => {
      const result = assertSameOriginHttpUrl('/api/v2/flights');
      expect(result.pathname).toBe('/api/v2/flights');
    });

    it('rejects cross-origin URL', () => {
      expect(() => assertSameOriginHttpUrl('https://evil.example/api')).toThrow(/same-origin/);
    });

    it('rejects non-http protocols', () => {
      expect(() => assertSameOriginHttpUrl('javascript:alert(1)')).toThrow(/HTTP/);
    });

    it('includes context in error message', () => {
      expect(() => assertSameOriginHttpUrl('https://evil.com/', 'test context')).toThrow(/test context/);
    });
  });

  describe('assertStaticAssetUrl', () => {
    it('accepts path under allowed prefix', () => {
      const result = assertStaticAssetUrl('/frontend/static/ai/main.js', '/frontend/static/ai/');
      expect(result).toBe('/frontend/static/ai/main.js');
    });

    it('rejects path outside allowed prefix', () => {
      expect(() => assertStaticAssetUrl('/etc/passwd', '/frontend/static/ai/')).toThrow();
    });
  });
});
