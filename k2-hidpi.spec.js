const { test, expect } = require('playwright/test');

test.use({
  channel: 'msedge',
  viewport: { width: 3135, height: 1464 },
  deviceScaleFactor: 1,
});

test('Retina pointer coordinates activate Open MIDI', async ({ page }) => {
  const failures = [];
  page.on('pageerror', error => failures.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error') failures.push(message.text());
  });
  await page.goto(`http://127.0.0.1:8080/?v=${Date.now()}`);
  await page.waitForSelector('canvas');
  await page.waitForTimeout(500);
  await page.screenshot({ path: '/tmp/k2-hidpi-before.png' });

  const metrics = await page.locator('canvas').evaluate(canvas => ({
    css: canvas.getBoundingClientRect().toJSON(),
    width: canvas.width,
    height: canvas.height,
    dpr: devicePixelRatio,
  }));

  const chooserPromise = page.waitForEvent('filechooser', { timeout: 3_000 });
  await page.mouse.click(3070, 45);
  const chooser = await chooserPromise;
  await chooser.setFiles({
    name: 'retina.mid',
    mimeType: 'audio/midi',
    buffer: Buffer.from([
      0x4d, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 0, 0x60,
      0x4d, 0x54, 0x72, 0x6b, 0, 0, 0, 0x0c,
      0, 0x90, 0x3c, 0x64, 0x60, 0x80, 0x3c, 0,
      0, 0xff, 0x2f, 0,
    ]),
  });
  await page.waitForTimeout(500);
  await page.screenshot({ path: '/tmp/k2-hidpi-loaded.png' });
  expect(metrics.dpr).toBe(1);
  expect(metrics.width).toBe(metrics.css.width);
  expect(failures).toEqual([]);
});
