/* global document, innerWidth, innerHeight, getComputedStyle */
// Optional acceptance runner; Playwright remains an external review tool, not an app dependency.
const { chromium } = await import(process.env.OPPA_PLAYWRIGHT_MODULE ?? 'playwright');
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on('pageerror', (error) => errors.push(error.message));
const base = process.env.OPPA_DESKTOP_TEST_URL ?? 'http://127.0.0.1:1420/test/desktop.html';
const folder = 'apps/oppa/design-review/desktop';
await mkdir(folder, { recursive: true });
const cases = [
  ['overview', '/overview'],
  ['printers', '/printers?selected='],
  ['printer-inspector', '/printers?selected=printer_system_frontdesk'],
  ['jobs', '/jobs?selected='],
  ['job-inspector', '/jobs?selected=job_demo_1039'],
  ['thermal', '/virtual?selected=printer_virtual_receipts'],
  ['office', '/virtual?selected=office-preview'],
  ['settings', '/settings'],
  ['connection', '/settings?category=Connection'],
  ['appearance', '/settings?category=Appearance'],
  ['logs', '/diagnostics'],
];
const measurements = [];
for (const [width, height] of [
  [800, 600],
  [1024, 768],
  [1280, 800],
  [1600, 1000],
]) {
  await page.setViewportSize({ width, height });
  for (const theme of ['light', 'dark'])
    for (const [name, route] of cases) {
      await page.goto(`${base}?theme=${theme}#${route}`);
      await page.locator('h1').waitFor();
      if (name === 'office') {
        await page.getByRole('img', { name: 'Captured document page 1' }).waitFor();
        await page.waitForFunction(() =>
          Array.from(document.images).every((image) => image.complete && image.naturalWidth > 0),
        );
      }
      const layout = await page.evaluate(() => ({
        width: innerWidth,
        height: innerHeight,
        overflow:
          document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight,
        main: document.querySelector('main').getBoundingClientRect().width,
        panels: [...document.querySelectorAll('.inspector-content')].map((node) => ({
          shown: getComputedStyle(node).display !== 'none',
          right: node.getBoundingClientRect().right,
        })),
        header: document.querySelector('header').getBoundingClientRect().height,
      }));
      assert.equal(layout.overflow, false, `${name} ${width} ${theme} document overflow`);
      assert.ok(layout.main > 0);
      assert.ok(layout.panels.every((panel) => panel.right <= width + 1));
      measurements.push({ name, theme, ...layout });
      if (width === 1280 || width === 800)
        await page.screenshot({ animations: 'disabled', path: `${folder}/${name}-${theme}-${width}x${height}.png` });
    }
}
await page.setViewportSize({ width: 1280, height: 800 });
for (const theme of ['light', 'dark']) {
  await page.goto(`${base}?theme=${theme}&collapsed#/overview`);
  await page.getByRole('heading', { name: 'Overview', exact: true }).waitFor();
  await page.screenshot({ animations: 'disabled', path: `${folder}/collapsed-${theme}.png` });
  await page.goto(`${base}?theme=${theme}#/printers?selected=`);
  await page.getByRole('button', { name: 'Add printer', exact: true }).click();
  await page.getByRole('dialog').waitFor();
  await page.screenshot({ animations: 'disabled', path: `${folder}/add-printer-${theme}.png` });
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('button', { name: 'Quick actions', exact: true }).click();
  await page.getByRole('combobox', { name: 'Search commands' }).waitFor();
  await page.screenshot({ animations: 'disabled', path: `${folder}/commands-${theme}.png` });
  await page.keyboard.press('Escape');
  await page.goto(`${base}?theme=${theme}#/virtual`);
  await page.getByRole('button', { name: 'Simulation', exact: true }).click();
  await page.getByRole('dialog').waitFor();
  await page.screenshot({ animations: 'disabled', path: `${folder}/simulation-${theme}.png` });
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.goto(`${base}?theme=${theme}#/jobs?selected=`);
  await page.getByRole('button', { name: 'Job history actions' }).click();
  await page.getByRole('menuitem', { name: 'Clear records…' }).click();
  await page.getByRole('dialog').waitFor();
  await page.screenshot({ animations: 'disabled', path: `${folder}/clear-records-${theme}.png` });
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
}
await page.goto(`${base}?theme=dark#/printers?selected=`);
await page.getByRole('listitem').first().focus();
await page.keyboard.press('ArrowDown');
assert.ok(page.url().includes('selected=printer_system_frontdesk'));
const divider = page.getByRole('separator', { name: 'Resize inspector' });
const before = await divider.getAttribute('aria-valuenow');
await divider.focus();
await page.keyboard.press('ArrowLeft');
assert.notEqual(await divider.getAttribute('aria-valuenow'), before);
await page.getByRole('button', { name: 'Close printer inspector' }).click();
await page.getByRole('searchbox').count();
await page.goto(`${base}?theme=light&empty#/printers`);
await page.getByRole('heading', { name: 'Printers', exact: true }).waitFor();
await page.screenshot({ animations: 'disabled', path: `${folder}/empty-printers.png` });
await page.goto(`${base}?theme=dark&disconnected#/overview`);
await page.getByRole('heading', { name: 'Connecting to your server', exact: true }).waitFor();
await page.screenshot({ animations: 'disabled', path: `${folder}/disconnected.png` });
await page.goto(`${base}?theme=dark&unavailable#/overview`);
await page.getByRole('heading', { name: 'Some printers are unavailable', exact: true }).waitFor();
await page.screenshot({ animations: 'disabled', path: `${folder}/unavailable.png` });
await page.goto(`${base}?theme=dark&long#/diagnostics`);
await page.getByLabel('Log severity').selectOption('error');
await page.locator('.log-entry summary').first().click();
await page.screenshot({ animations: 'disabled', path: `${folder}/expanded-log.png` });
await page.goto(`${base}?theme=light#/virtual?selected=office-preview`);
await page.getByRole('button', { name: 'Next page', exact: true }).click();
await page.getByRole('img', { name: 'Captured document page 2' }).waitFor();
await page.getByRole('button', { name: 'Toggle output inspector' }).click();
await page.screenshot({ animations: 'disabled', path: `${folder}/office-inspector.png` });
await page.goto(`${base}?theme=dark&collapsed#/settings`);
await page.getByRole('button', { name: 'Settings', exact: true }).waitFor();
const navOrder = await page
  .locator('nav[aria-label="Primary navigation"] button')
  .evaluateAll((buttons) => buttons.map((button) => button.getAttribute('aria-label')));
assert.equal(navOrder[1], 'Overview');
assert.equal(navOrder[navOrder.length - 1], 'Settings');
await page.goto(`${base}?theme=light#/jobs?selected=job_demo_1039`);
assert.equal(await page.locator('h1').evaluate((node) => getComputedStyle(node).userSelect), 'none');
assert.equal(await page.locator('[role="alert"]').evaluate((node) => getComputedStyle(node).userSelect), 'text');
assert.equal(
  await page
    .locator('dd')
    .first()
    .evaluate((node) => getComputedStyle(node).userSelect),
  'text',
);
await page.goto(`${base}?theme=dark#/printers`);
assert.equal(await page.getByLabel('Search printers').evaluate((node) => getComputedStyle(node).userSelect), 'text');
await page.goto(`${base}?theme=light&no-developer#/virtual`);
assert.equal(await page.getByLabel('Document preview canvas').count(), 0);
await page.goto(`${base}?theme=light#/virtual?selected=office-preview`);
const downloadPromise = page.waitForEvent('download');
await page.getByRole('button', { name: 'Export captured output', exact: true }).click();
const download = await downloadPromise;
assert.ok(download.suggestedFilename().endsWith('.png'));
await page.goto(`${base}?theme=dark#/printers?selected=`);
await page.getByRole('button', { name: 'Add printer', exact: true }).click();
await page.getByLabel('Display name', { exact: true }).fill('Network test target');
await page.getByLabel('Host name or IP address', { exact: true }).fill('invalid/path');
await page.getByRole('checkbox').check();
await page.getByRole('dialog').getByRole('button', { name: 'Add printer', exact: true }).click();
await page.getByRole('alert').waitFor();
await page.getByLabel('Host name or IP address', { exact: true }).fill('192.0.2.50');
await page.getByRole('dialog').getByRole('button', { name: 'Add printer', exact: true }).click();
await page.getByText('Network test target', { exact: true }).waitFor();
assert.deepEqual(errors, []);
console.log(
  JSON.stringify(
    {
      layouts: measurements.length,
      browserErrors: errors,
      checks:
        'screens, collapsed navigation, dialogs, commands, arrows, resizing, failure/empty states, developer gate',
    },
    null,
    2,
  ),
);
await browser.close();
