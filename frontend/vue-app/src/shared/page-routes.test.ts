import { describe, expect, it } from 'vitest';
import { ontologyCenterUrl, PAGE_ROUTES, pageUrl } from './page-routes';

describe('page-routes', () => {
  it('pageUrl returns canonical paths', () => {
    expect(pageUrl('ontology_center')).toBe(PAGE_ROUTES.ontology_center);
    expect(pageUrl('flight_monitor')).toBe('/frontend/flight_monitor.html');
  });

  it('ontologyCenterUrl builds deep-link query params', () => {
    expect(ontologyCenterUrl()).toBe('/frontend/ontology_center.html');
    expect(ontologyCenterUrl({ flightId: 'FL_1' })).toBe(
      '/frontend/ontology_center.html?flight=FL_1',
    );
    expect(ontologyCenterUrl({ registration: 'B-1234', tab: 'resources' })).toBe(
      '/frontend/ontology_center.html?registration=B-1234&tab=resources',
    );
    expect(
      ontologyCenterUrl({
        flightId: 'FL_1',
        registration: 'B-1',
        tab: 'suggestions',
      }),
    ).toBe('/frontend/ontology_center.html?flight=FL_1&registration=B-1&tab=suggestions');
  });

  it('ontologyCenterUrl trims empty values', () => {
    expect(ontologyCenterUrl({ flightId: '  ', registration: null })).toBe(
      '/frontend/ontology_center.html',
    );
  });
});
