import { describe, expect, it } from 'vitest';

import {
  encodeSseEvents,
  getNetworkIdleTimeoutMs,
  getVuePanelSelectors,
  getVueRegionSelector,
  getVueSseStreams,
  isOptionalVueInteraction,
} from '../../scripts/parity/capture-vue-adaptations.mjs';

describe('Vue parity capture adaptations', () => {
  it('adds Vue dashboard shells after legacy panel selectors', () => {
    expect(getVuePanelSelectors('dashboard', ['#moduleGrid', '.dashboard-handover-card'])).toEqual([
      '#moduleGrid',
      '.dashboard-handover-card',
      '.wb-shell',
      '#app',
      'body',
    ]);
  });

  it('maps the dashboard module-grid region without changing the legacy fixture', () => {
    expect(getVueRegionSelector('dashboard', {
      id: 'module-grid',
      selector: '#moduleGrid',
    })).toBe('.modules-grid');
  });

  it('soft-skips only the known dashboard handover interaction', () => {
    expect(isOptionalVueInteraction('dashboard', 'handover-drawer')).toBe(true);
    expect(isOptionalVueInteraction('dashboard', 'unexpected-action')).toBe(false);
    expect(isOptionalVueInteraction('system_flags', 'handover-drawer')).toBe(false);
  });

  it('uses a short dashboard idle timeout so SSE reconnects cannot stall capture', () => {
    expect(getNetworkIdleTimeoutMs('dashboard')).toBe(1_500);
    expect(getNetworkIdleTimeoutMs('system_flags')).toBe(10_000);
  });

  it('maps the legacy system-status stream onto the canonical topic endpoint', () => {
    expect(getVueSseStreams('system_status', [{
      id: 'status-stream',
      pathname: '/api/v2/health/stream/status',
      query: { token: ['legacy-token'] },
      events: [],
    }])).toEqual([{
      id: 'status-stream',
      pathname: '/api/v2/sse/stream',
      query: { topics: ['error_events'] },
      events: [],
    }]);
  });

  it('encodes named fixture events as valid finite SSE frames', () => {
    expect(encodeSseEvents([{
      id: 'event-1',
      event: 'error_log',
      data: { message: 'deterministic' },
    }])).toBe('id: event-1\nevent: error_log\ndata: {"message":"deterministic"}\n\n');
  });
});
