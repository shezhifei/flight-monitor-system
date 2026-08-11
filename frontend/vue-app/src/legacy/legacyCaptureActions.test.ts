import { describe, expect, it, vi } from 'vitest';

import {
  CaptureActionValidationError,
  normalizeCaptureDefinition,
  runCaptureActions,
} from '../../scripts/parity/capture-actions.mjs';

describe('legacy capture action definitions', () => {
  it('normalizes deterministic regions and interaction states', () => {
    expect(normalizeCaptureDefinition({
      full_page: false,
      expected_panels: ['#app'],
      regions: [{ id: 'summary', selector: '#summary' }],
      interactions: [{
        id: 'create-dialog',
        actions: [{ type: 'click', selector: '#create' }],
        expected_panels: ['#dialog.show'],
        regions: [{ id: 'create-dialog', selector: '#dialog .modal' }],
      }],
    }, 'fixture.capture')).toEqual({
      theme: 'light',
      expectedPanels: ['#app'],
      regions: [{ id: 'summary', selector: '#summary' }],
      captureFullPage: false,
      interactions: [{
        id: 'create-dialog',
        actions: [{ type: 'click', selector: '#create' }],
        expectedPanels: ['#dialog.show'],
        regions: [{ id: 'create-dialog', selector: '#dialog .modal' }],
        captureFullPage: false,
      }],
      blockedInteractions: [],
    });
  });

  it.each([
    [{ interactions: [{ id: '../escape', actions: [{ type: 'click', selector: '#x' }] }] }],
    [{ interactions: [{ id: 'dialog', actions: [{ type: 'evaluate', selector: 'body' }] }] }],
    [{ regions: [{ id: 'same', selector: '#a' }, { id: 'same', selector: '#b' }] }],
    [{
      interactions: [{ id: 'dialog', actions: [{ type: 'click', selector: '#x' }] }],
      blocked_interactions: [{ id: 'dialog', reason: 'known gap', source: 'legacy.js' }],
    }],
  ])('rejects unsafe or ambiguous definitions: %j', (definition) => {
    expect(() => normalizeCaptureDefinition(definition)).toThrow(CaptureActionValidationError);
  });

  it('runs only the normalized locator operations in order', async () => {
    const click = vi.fn().mockResolvedValue(undefined);
    const fill = vi.fn().mockResolvedValue(undefined);
    const waitFor = vi.fn().mockResolvedValue(undefined);
    const first = vi.fn(() => ({ click, fill, waitFor }));
    const locator = vi.fn((_selector: string) => ({ first }));

    await runCaptureActions({ locator } as never, [
      { type: 'fill', selector: '#name', value: '固定值' },
      { type: 'click', selector: '#open' },
    ]);

    expect(locator.mock.calls.map(([selector]) => selector)).toEqual(['#name', '#open']);
    expect(fill).toHaveBeenCalledWith('固定值');
    expect(click).toHaveBeenCalledOnce();
    expect(waitFor).toHaveBeenCalledTimes(2);
  });
});
