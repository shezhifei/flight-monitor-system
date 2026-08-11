import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { expect, test } from '@playwright/test';
import {
  comparePngBuffers,
  comparePngBuffersOnSharedCanvas,
  VISUAL_THRESHOLDS,
} from '../../scripts/parity/pixel-compare.mjs';

/**
 * Task 31: Vue vs tracked legacy screenshot gates.
 *
 * Final plan thresholds:
 * - full-page ≤ 1%
 * - critical region ≤ 0.3%
 *
 * Progress ceilings enforce non-regression while CSS/layout converge.
 * Pages may not be marked verified until target thresholds pass.
 *
 * Vue baselines: e2e/parity/snapshots/vue/
 * Refresh: npm run parity:refresh-vue -- --page login,system_flags
 */

const LEGACY_ROOT = path.resolve(process.cwd(), 'e2e/parity/snapshots/legacy');
const VUE_ROOT = path.resolve(process.cwd(), 'e2e/parity/snapshots/vue');

/** Login: full-page at plan target; region residual ~0.9% form CJK AA. */
const LOGIN_CEILING = Object.freeze({
  fullPage: 0.01,
  region: VISUAL_THRESHOLDS.region,
});

/**
 * system_flags: full-page at plan target (≤1%). Workspace residual ~0.9% is
 * select/path glyph AA — keep a progress ceiling until ≤0.3%.
 */
const SYSTEM_FLAGS_CEILING = Object.freeze({
  fullPage: VISUAL_THRESHOLDS.fullPage,
  region: VISUAL_THRESHOLDS.region,
});

/** Dashboard structure is still substantially shorter than legacy. */
const DASHBOARD_CEILING = Object.freeze({
  fullPage: 0.74,
});

const SYSTEM_STATUS_CEILING = Object.freeze({
  fullPage: VISUAL_THRESHOLDS.fullPage,
  region: VISUAL_THRESHOLDS.region,
});

function isPng(bytes: Buffer): boolean {
  return bytes.length >= 24
    && bytes.subarray(0, 8).toString('hex') === '89504e470d0a1a0a'
    && bytes.readUInt32BE(16) > 0
    && bytes.readUInt32BE(20) > 0;
}

function readPng(root: string, page: string, file: string): Buffer {
  const absolute = path.join(root, page, file);
  expect(existsSync(absolute), `missing snapshot ${absolute}`).toBe(true);
  const bytes = readFileSync(absolute);
  expect(isPng(bytes), `invalid PNG ${absolute}`).toBe(true);
  return bytes;
}

async function assertVisual(
  page: import('@playwright/test').Page,
  options: {
    pageId: string;
    label: string;
    legacyFile: string;
    vueFile: string;
    target: number;
    ceiling: number;
  },
): Promise<void> {
  const legacy = readPng(LEGACY_ROOT, options.pageId, options.legacyFile);
  const vue = readPng(VUE_ROOT, options.pageId, options.vueFile);
  await page.goto('about:blank');
  const result = await comparePngBuffers(page, legacy, vue);
  expect(result.sizeMismatch, `${options.label}: size mismatch`).toBe(false);

  const pct = (result.ratio * 100).toFixed(3);
  const targetPct = (options.target * 100).toFixed(2);
  test.info().annotations.push({
    type: 'visual-diff',
    description: `${options.label}: ${pct}% differing (target ≤ ${targetPct}%, ceiling ≤ ${(options.ceiling * 100).toFixed(1)}%)`,
  });

  // Hard non-regression gate.
  expect(
    result.ratio,
    `${options.label}: ${pct}% exceeds progress ceiling ${(options.ceiling * 100).toFixed(1)}% (${result.differing}/${result.total})`,
  ).toBeLessThanOrEqual(options.ceiling);

  // Soft target gate — surfaces remaining debt without blocking while CSS converges.
  if (result.ratio > options.target) {
    test.info().annotations.push({
      type: 'visual-debt',
      description: `${options.label}: still above plan target ${targetPct}% (currently ${pct}%)`,
    });
  }
}

async function assertStructuralProgress(
  page: import('@playwright/test').Page,
  options: {
    pageId: string;
    label: string;
    legacyFile: string;
    vueFile: string;
    ceiling: number;
  },
): Promise<void> {
  const legacy = readPng(LEGACY_ROOT, options.pageId, options.legacyFile);
  const vue = readPng(VUE_ROOT, options.pageId, options.vueFile);
  await page.goto('about:blank');
  const result = await comparePngBuffersOnSharedCanvas(page, legacy, vue);
  const pct = (result.ratio * 100).toFixed(3);
  test.info().annotations.push({
    type: 'visual-debt',
    description: `${options.label}: ${pct}% differing on shared ${result.width}×${result.height} canvas; size mismatch=${result.sizeMismatch}`,
  });
  expect(
    result.ratio,
    `${options.label}: ${pct}% exceeds structural progress ceiling ${(options.ceiling * 100).toFixed(1)}%`,
  ).toBeLessThanOrEqual(options.ceiling);
}

test.describe('visual parity gates', () => {
  test('login-default: legacy desktop baseline PNG is present and valid', async () => {
    const bytes = readPng(LEGACY_ROOT, 'login', 'default--full-page--desktop.png');
    expect(createHash('sha256').update(bytes).digest('hex')).toHaveLength(64);
  });

  test('login-default: Vue full-page desktop vs legacy', async ({ page }) => {
    await assertVisual(page, {
      pageId: 'login',
      label: 'login-default full-page desktop',
      legacyFile: 'default--full-page--desktop.png',
      vueFile: 'default--full-page--desktop.png',
      target: VISUAL_THRESHOLDS.fullPage,
      ceiling: LOGIN_CEILING.fullPage,
    });
  });

  test('login-default: Vue login-card desktop vs legacy', async ({ page }) => {
    await assertVisual(page, {
      pageId: 'login',
      label: 'login-default login-card desktop',
      legacyFile: 'default--login-card--desktop.png',
      vueFile: 'default--login-card--desktop.png',
      target: VISUAL_THRESHOLDS.region,
      ceiling: LOGIN_CEILING.region,
    });
  });

  test('login-required-username-error: Vue login-card desktop vs legacy', async ({ page }) => {
    await assertVisual(page, {
      pageId: 'login',
      label: 'login-required-username-error login-card desktop',
      legacyFile: 'required-username-error--login-card--desktop.png',
      vueFile: 'required-username-error--login-card--desktop.png',
      target: VISUAL_THRESHOLDS.region,
      ceiling: LOGIN_CEILING.region,
    });
  });

  test('login-password-visible: Vue login-card desktop vs legacy', async ({ page }) => {
    await assertVisual(page, {
      pageId: 'login',
      label: 'login-password-visible login-card desktop',
      legacyFile: 'password-visible--login-card--desktop.png',
      vueFile: 'password-visible--login-card--desktop.png',
      target: VISUAL_THRESHOLDS.region,
      ceiling: LOGIN_CEILING.region,
    });
  });

  test('system_flags-success: Vue full-page desktop vs legacy', async ({ page }) => {
    await assertVisual(page, {
      pageId: 'system_flags',
      label: 'system_flags-success full-page desktop',
      legacyFile: 'system-flags-success--full-page--desktop.png',
      vueFile: 'system-flags-success--full-page--desktop.png',
      target: VISUAL_THRESHOLDS.fullPage,
      ceiling: SYSTEM_FLAGS_CEILING.fullPage,
    });
  });

  test('system_flags-success: Vue flags-workspace desktop vs legacy', async ({ page }) => {
    await assertVisual(page, {
      pageId: 'system_flags',
      label: 'system_flags-success flags-workspace desktop',
      legacyFile: 'system-flags-success--flags-workspace--desktop.png',
      vueFile: 'system-flags-success--flags-workspace--desktop.png',
      target: VISUAL_THRESHOLDS.region,
      ceiling: SYSTEM_FLAGS_CEILING.region,
    });
  });

  test('dashboard-success: Vue full-page desktop structural progress', async ({ page }) => {
    await assertStructuralProgress(page, {
      pageId: 'dashboard',
      label: 'dashboard-success full-page desktop',
      legacyFile: 'dashboard-success--full-page--desktop.png',
      vueFile: 'dashboard-success--full-page--desktop.png',
      ceiling: DASHBOARD_CEILING.fullPage,
    });
  });

  test('system_status-success: Vue full-page desktop vs legacy', async ({ page }) => {
    await assertVisual(page, {
      pageId: 'system_status',
      label: 'system_status-success full-page desktop',
      legacyFile: 'system-status-success--full-page--desktop.png',
      vueFile: 'system-status-success--full-page--desktop.png',
      target: VISUAL_THRESHOLDS.fullPage,
      ceiling: SYSTEM_STATUS_CEILING.fullPage,
    });
  });

  test('system_status-success: Vue realtime-log desktop vs legacy', async ({ page }) => {
    await assertVisual(page, {
      pageId: 'system_status',
      label: 'system_status-success realtime-log desktop',
      legacyFile: 'system-status-success--realtime-log--desktop.png',
      vueFile: 'system-status-success--realtime-log--desktop.png',
      target: VISUAL_THRESHOLDS.region,
      ceiling: SYSTEM_STATUS_CEILING.region,
    });
  });
});
