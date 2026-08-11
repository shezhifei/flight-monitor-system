import type { Page } from '@playwright/test';

export interface PixelCompareResult {
  ratio: number;
  differing: number;
  total: number;
  width: number;
  height: number;
  sizeMismatch: boolean;
}

export declare function comparePngBuffers(
  page: Page,
  leftPng: Buffer,
  rightPng: Buffer,
): Promise<PixelCompareResult>;

export declare function comparePngBuffersOnSharedCanvas(
  page: Page,
  leftPng: Buffer,
  rightPng: Buffer,
): Promise<PixelCompareResult>;

export declare const VISUAL_THRESHOLDS: {
  readonly region: number;
  readonly fullPage: number;
};
