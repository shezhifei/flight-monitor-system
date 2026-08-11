import type { Page } from '@playwright/test';

export interface CaptureRegion {
  id: string;
  selector: string;
}

export interface CaptureAction {
  type: 'check' | 'click' | 'fill' | 'press' | 'select-option' | 'uncheck' | 'wait-for';
  selector: string;
  value?: string;
  values?: string[];
  key?: string;
  state?: 'attached' | 'detached' | 'hidden' | 'visible';
}

export interface CaptureInteraction {
  id: string;
  actions: CaptureAction[];
  expectedPanels: string[];
  regions: CaptureRegion[];
  captureFullPage: boolean;
}

export interface NormalizedCaptureDefinition {
  theme: string;
  expectedPanels: string[];
  regions: CaptureRegion[];
  captureFullPage: boolean;
  interactions: CaptureInteraction[];
  blockedInteractions: Array<{ id: string; reason: string; source: string }>;
}

export class CaptureActionValidationError extends Error {}

export function normalizeCaptureDefinition(
  value: unknown,
  context?: string,
): NormalizedCaptureDefinition;

export function runCaptureActions(page: Page, actions: CaptureAction[]): Promise<void>;
