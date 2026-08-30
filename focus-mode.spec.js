const { test, expect } = require('/Users/nathanbradshaw/.npm/_npx/e41f203b7505f1fb/node_modules/playwright/test');

test('F8 toggles keyboard focus mode', async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.goto('http://127.0.0.1:8081');
  await page.waitForTimeout(2000);
  await page.locator('canvas').first().click({ position: { x: 8, y: 8 } });
  await page.keyboard.press('F8');
  await page.waitForTimeout(1200);
  await page.screenshot({ path: '/tmp/k2-focus-f8.png' });
  await page.keyboard.press('F8');
  await page.waitForTimeout(500);
  await page.screenshot({ path: '/tmp/k2-focus-restored.png' });
  expect(await page.locator('canvas').count()).toBeGreaterThan(0);
});
