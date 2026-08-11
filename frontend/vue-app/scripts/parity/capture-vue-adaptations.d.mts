import type { CaptureInteraction, CaptureRegion } from './capture-actions.mjs';

export interface SseFixtureEvent {
  id?: string;
  event?: string;
  data?: unknown;
}

export interface SseFixtureStream {
  id: string;
  pathname: string;
  query?: Record<string, string[]>;
  events?: SseFixtureEvent[];
}

export declare function getVuePanelSelectors(
  pageId: string,
  legacySelectors?: string[],
): string[];

export declare function getVueRegionSelector(pageId: string, region: CaptureRegion): string;

export declare function isOptionalVueInteraction(
  pageId: string,
  interactionId: CaptureInteraction['id'],
): boolean;

export declare function getNetworkIdleTimeoutMs(pageId: string): number;

export declare function getVueSseStreams(
  pageId: string,
  legacyStreams?: SseFixtureStream[],
): SseFixtureStream[];

export declare function encodeSseEvents(events?: SseFixtureEvent[]): string;
