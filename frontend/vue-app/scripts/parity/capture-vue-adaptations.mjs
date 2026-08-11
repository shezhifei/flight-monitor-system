/**
 * Explicit DOM adaptations used only while capturing the canonical Vue pages.
 *
 * Legacy fixture selectors remain immutable evidence. When Vue owns a
 * semantically equivalent region under a different selector, record the
 * mapping here so the difference is visible and unit tested.
 */
const PAGE_ADAPTATIONS = Object.freeze({
  dashboard: Object.freeze({
    panelSelectors: Object.freeze(['.wb-shell']),
    regionSelectors: Object.freeze({
      'module-grid': '.modules-grid',
    }),
    optionalInteractions: Object.freeze(['handover-drawer']),
  }),
  system_status: Object.freeze({
    sse: Object.freeze({
      pathname: '/api/v2/sse/stream',
      query: Object.freeze({ topics: Object.freeze(['error_events']) }),
    }),
  }),
});

function uniqueSelectors(selectors) {
  return [...new Set(selectors.filter((selector) => (
    typeof selector === 'string' && selector.trim() !== ''
  )))];
}

export function getVuePanelSelectors(pageId, legacySelectors = []) {
  const adaptation = PAGE_ADAPTATIONS[pageId];
  return uniqueSelectors([
    ...legacySelectors,
    ...(adaptation?.panelSelectors ?? []),
    '#app',
    'body',
  ]);
}

export function getVueRegionSelector(pageId, region) {
  return PAGE_ADAPTATIONS[pageId]?.regionSelectors?.[region.id] ?? region.selector;
}

export function isOptionalVueInteraction(pageId, interactionId) {
  return PAGE_ADAPTATIONS[pageId]?.optionalInteractions?.includes(interactionId) ?? false;
}

export function getNetworkIdleTimeoutMs(pageId) {
  return pageId === 'dashboard' ? 1_500 : 10_000;
}

export function getVueSseStreams(pageId, legacyStreams = []) {
  const adaptation = PAGE_ADAPTATIONS[pageId]?.sse;
  return legacyStreams.map((stream) => ({
    ...stream,
    pathname: adaptation?.pathname ?? stream.pathname,
    query: adaptation?.query ?? stream.query ?? {},
  }));
}

export function encodeSseEvents(events = []) {
  return events.map((event) => [
    event.id ? `id: ${event.id}` : null,
    event.event ? `event: ${event.event}` : null,
    `data: ${JSON.stringify(event.data ?? null)}`,
    '',
    '',
  ].filter((line) => line !== null).join('\n')).join('');
}
