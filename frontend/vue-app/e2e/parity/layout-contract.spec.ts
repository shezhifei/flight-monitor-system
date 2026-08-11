import { expect, test, type Page } from '@playwright/test';
import { installSessionRoutes, PARITY_ADMIN } from './helpers/authRoutes';

/**
 * Task 30 scaffold: computed layout contracts for critical regions.
 * Thresholds are browser-rounding only (no broad percentage excuses).
 */

async function box(page: Page, selector: string) {
  const locator = page.locator(selector).first();
  await expect(locator).toBeVisible();
  const handle = await locator.elementHandle();
  if (!handle) throw new Error(`missing ${selector}`);
  return handle.evaluate((el) => {
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return {
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
      display: style.display,
      position: style.position,
      overflow: style.overflow,
      zIndex: style.zIndex,
      fontFamily: style.fontFamily,
      fontSize: style.fontSize,
      fontWeight: style.fontWeight,
      lineHeight: style.lineHeight,
    };
  });
}

test.describe('layout contracts', () => {
  test('login-default: login card has stable desktop geometry anchors', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/frontend/login.html');
    const card = await box(page, '.login-card, #loginForm');
    expect(card.width).toBeGreaterThan(280);
    expect(card.width).toBeLessThan(560);
    expect(card.height).toBeGreaterThan(300);
    expect(card.display).not.toBe('none');
    // Card should sit in the right half of the desktop layout (legacy behavior).
    expect(card.x).toBeGreaterThan(400);
  });

  test('system_flags-success: flags workspace is above-the-fold on desktop', async ({ page }) => {
    await installSessionRoutes(page, PARITY_ADMIN);
    await page.route('**/api/v2/system/flags', (route) => route.fulfill({
      status: 200,
      json: {
        success: true,
        data: {
          flags: [{
            path: 'feature_flags.demo.enabled',
            value: true,
            type: 'boolean',
            category: 'feature_flags',
            label: 'Demo',
            description: 'demo',
            masked: false,
          }],
        },
      },
    }));
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/frontend/system_flags.html');
    const main = await box(page, '.main-content, #content-area, main');
    expect(main.y).toBeLessThan(120);
    expect(main.width).toBeGreaterThan(600);
    expect(main.display).not.toBe('none');
  });
});
