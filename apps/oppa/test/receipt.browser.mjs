/* global document, innerWidth, innerHeight, getComputedStyle */
// Optional visual acceptance runner. Uses real PNGs produced by the Rust regression fixture.
import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
const { chromium } = await import(process.env.OPPA_PLAYWRIGHT_MODULE ?? 'playwright');
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage();
const errors = [];
page.on('pageerror', (error) => errors.push(error.message));
const folder = 'apps/oppa/design-review/receipts';
await mkdir(folder, { recursive: true });
let checked = 0;
for (const [width, height] of [
  [800, 600],
  [1280, 800],
]) {
  await page.setViewportSize({ width, height });
  for (const theme of ['light', 'dark'])
    for (const receipt of ['short58', 'short80', 'long80', 'symbols58']) {
      await page.goto(
        `http://127.0.0.1:1420/test/desktop.html?theme=${theme}&receipt=${receipt}#/virtual?selected=printer_virtual_receipts`,
      );
      const image = page.getByRole('img', { name: 'Captured document page 1', exact: true });
      await image.waitFor();
      await page.waitForFunction(() => [...document.images].every((image) => image.complete && image.naturalWidth > 0));
      assert.equal(await page.getByLabel('Preview fit').inputValue(), 'actual');
      const source = await image.getAttribute('src');
      for (const presentation of ['receipt', 'raw']) {
        await page.getByLabel('Thermal presentation').selectOption(presentation);
        for (const fit of ['actual', 'window', 'width', 'manual']) {
          await page.getByLabel('Preview fit').selectOption(fit);
          if (fit === 'manual') await page.getByRole('button', { name: 'Zoom in', exact: true }).click();
          await page.waitForTimeout(70);
          const state = await page.evaluate(() => {
            const canvas = document.querySelector('.document-canvas');
            const image = document.querySelector('.captured-raster');
            const stage = canvas.firstElementChild;
            const content = stage.firstElementChild;
            const transform = getComputedStyle(content).transform;
            return {
              zoom: Number(transform.match(/matrix\(([^,]+)/)[1]),
              bodyOverflow:
                document.documentElement.scrollWidth > innerWidth ||
                document.documentElement.scrollHeight > innerHeight,
              paper: document.querySelector('.thermal-paper') !== null,
              padding: document.querySelector('.thermal-paper')
                ? ['paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft'].map(
                    (key) => getComputedStyle(document.querySelector('.thermal-paper'))[key],
                  )
                : [],
              imageWidth: image.getBoundingClientRect().width,
              canvasWidth: canvas.clientWidth,
              scrollHeight: canvas.scrollHeight,
              canvasHeight: canvas.clientHeight,
            };
          });
          assert.equal(state.bodyOverflow, false);
          assert.equal(state.paper, presentation === 'receipt');
          assert.equal(await image.getAttribute('src'), source);
          if (presentation === 'receipt') assert.deepEqual(state.padding, ['16px', '16px', '16px', '16px']);
          if (fit === 'actual') {
            assert.equal(state.zoom, 1);
            const mm = receipt.includes('58') ? 58 : 80;
            assert.ok(Math.abs(state.imageWidth - (mm * 96) / 25.4) < 0.1);
          }
          if (fit === 'window') {
            assert.ok(state.zoom <= 1);
            assert.ok(state.scrollHeight <= state.canvasHeight + 1);
          }
          if (fit === 'width') assert.ok(state.imageWidth <= state.canvasWidth - 47);
          if (receipt === 'long80' && fit === 'actual') assert.ok(state.scrollHeight > state.canvasHeight);
          checked++;
          if (
            (fit === 'actual' && receipt.startsWith('short')) ||
            (fit === 'window' && !receipt.startsWith('short')) ||
            (fit === 'width' && receipt === 'symbols58' && width === 1280) ||
            (fit === 'manual' && receipt === 'short80' && width === 1280)
          )
            await page.screenshot({
              path: `${folder}/${receipt}-${theme}-${presentation}-${fit}-${width}.png`,
              animations: 'disabled',
            });
        }
      }
      // Export must preserve exactly the captured raster, independent of paper decoration and zoom.
      const downloadPromise = page.waitForEvent('download');
      await page.getByRole('button', { name: 'Export captured output' }).click();
      const download = await downloadPromise;
      const exported = await readFile(await download.path());
      const captured = await readFile(`apps/oppa/test/receipts/${receipt}.png`);
      assert.deepEqual(exported, captured);
    }
}
// Office page fitting and navigation remain available without receipt decoration.
await page.goto('http://127.0.0.1:1420/test/desktop.html?theme=light#/virtual?selected=office-preview');
await page.getByRole('img', { name: 'Captured document page 1', exact: true }).waitFor();
assert.equal(await page.getByLabel('Preview fit').inputValue(), 'window');
assert.equal(await page.getByLabel('Thermal presentation').count(), 0);
await page.getByRole('button', { name: 'Next page', exact: true }).click();
await page.getByRole('img', { name: 'Captured document page 2', exact: true }).waitFor();
assert.deepEqual(errors, []);
await browser.close();
console.log(`Passed ${checked} receipt layouts; PNG exports unchanged; office navigation verified.`);
