# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: k2-hidpi.spec.js >> Retina pointer coordinates activate Open MIDI
- Location: k2-hidpi.spec.js:9:1

# Error details

```
Error: page.goto: net::ERR_CONNECTION_REFUSED at http://127.0.0.1:8080/?v=1787702668066
Call log:
  - navigating to "http://127.0.0.1:8080/?v=1787702668066", waiting until "load"

```

# Page snapshot

```yaml
- generic [ref=e3]:
  - generic [ref=e6]:
    - heading "Hmmm… can't reach this page" [level=1] [ref=e7]
    - paragraph [ref=e8]:
      - strong [ref=e9]: 127.0.0.1
      - text: refused to connect.
    - generic [ref=e10]:
      - paragraph [ref=e11]: "Try:"
      - list [ref=e12]:
        - listitem [ref=e13]: •Checking the connection
        - listitem [ref=e14]:
          - text: •
          - link "Checking the proxy and the firewall" [ref=e15] [cursor=pointer]:
            - /url: "#buttons"
    - generic [ref=e16]: ERR_CONNECTION_REFUSED
  - generic [ref=e17]:
    - button "Refresh" [ref=e19] [cursor=pointer]
    - button "Details" [ref=e20] [cursor=pointer]
```

# Test source

```ts
  1  | const { test, expect } = require('playwright/test');
  2  | 
  3  | test.use({
  4  |   channel: 'msedge',
  5  |   viewport: { width: 3135, height: 1464 },
  6  |   deviceScaleFactor: 1,
  7  | });
  8  | 
  9  | test('Retina pointer coordinates activate Open MIDI', async ({ page }) => {
  10 |   const failures = [];
  11 |   page.on('pageerror', error => failures.push(error.message));
  12 |   page.on('console', message => {
  13 |     if (message.type() === 'error') failures.push(message.text());
  14 |   });
> 15 |   await page.goto(`http://127.0.0.1:8080/?v=${Date.now()}`);
     |              ^ Error: page.goto: net::ERR_CONNECTION_REFUSED at http://127.0.0.1:8080/?v=1787702668066
  16 |   await page.waitForSelector('canvas');
  17 |   await page.waitForTimeout(500);
  18 |   await page.screenshot({ path: '/tmp/k2-hidpi-before.png' });
  19 | 
  20 |   const metrics = await page.locator('canvas').evaluate(canvas => ({
  21 |     css: canvas.getBoundingClientRect().toJSON(),
  22 |     width: canvas.width,
  23 |     height: canvas.height,
  24 |     dpr: devicePixelRatio,
  25 |   }));
  26 | 
  27 |   const chooserPromise = page.waitForEvent('filechooser', { timeout: 3_000 });
  28 |   await page.mouse.click(3070, 45);
  29 |   const chooser = await chooserPromise;
  30 |   await chooser.setFiles({
  31 |     name: 'retina.mid',
  32 |     mimeType: 'audio/midi',
  33 |     buffer: Buffer.from([
  34 |       0x4d, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 0, 0, 1, 0, 0x60,
  35 |       0x4d, 0x54, 0x72, 0x6b, 0, 0, 0, 0x0c,
  36 |       0, 0x90, 0x3c, 0x64, 0x60, 0x80, 0x3c, 0,
  37 |       0, 0xff, 0x2f, 0,
  38 |     ]),
  39 |   });
  40 |   await page.waitForTimeout(500);
  41 |   await page.screenshot({ path: '/tmp/k2-hidpi-loaded.png' });
  42 |   expect(metrics.dpr).toBe(1);
  43 |   expect(metrics.width).toBe(metrics.css.width);
  44 |   expect(failures).toEqual([]);
  45 | });
  46 | 
```