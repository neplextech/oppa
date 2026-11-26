/* global document, window */
// Run with the documentation dev server and an external Playwright install.
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const { chromium } = await import(process.env.OPPA_PLAYWRIGHT_MODULE ?? 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../public/previews');
const base = process.env.OPPA_PREVIEW_BASE_URL ?? 'http://127.0.0.1:3000';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const manifest = [];
try {
  const page = await browser.newPage({ deviceScaleFactor: 1 });
  for (const [format, count, width, height] of [
    ['play-store', 6, 1080, 1920],
    ['app-store', 5, 1290, 2796],
    ['feature', 1, 1024, 500],
  ]) {
    await mkdir(join(root, format === 'feature' ? '.' : format), { recursive: true });
    await page.setViewportSize({ width, height });
    for (let slide = 0; slide < count; slide++) {
      await page.goto(`${base}/previews/artwork.html?format=${format}&slide=${slide}`);
      await page.evaluate(async () => {
        await document.fonts.ready;
        await Promise.all([...document.images].map((image) => image.decode()));
      });
      const { slug } = await page.evaluate(() => window.previewAsset);
      const file = format === 'feature' ? 'feature-graphic.png' : `${format}/${slug}.png`;
      await page.screenshot({ path: join(root, file), animations: 'disabled', omitBackground: false });
      manifest.push({ file, width, height, draft: format === 'app-store' });
      console.log(`Exported ${file} (${width} × ${height})`);
    }
  }
  await writeFile(join(root, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
} finally {
  await browser.close();
}
