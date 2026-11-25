/* global document, innerWidth, innerHeight */
// Optional browser acceptance check: provide an installed Playwright module through OPPA_PLAYWRIGHT_MODULE.
const { chromium } = await import(process.env.OPPA_PLAYWRIGHT_MODULE ?? 'playwright');
import assert from 'node:assert/strict';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage();
const errors = [];
page.on('pageerror', (error) => errors.push(error.message));
const base = process.env.OPPA_TEST_URL ?? 'http://127.0.0.1:1420/test/onboarding.html';
const observations = [];
for (const [width, height] of [
  [800, 600],
  [1024, 768],
  [1280, 800],
  [1600, 1000],
]) {
  await page.setViewportSize({ width, height });
  for (const theme of ['light', 'dark'])
    for (const step of ['connect', 'pair', 'printers']) {
      await page.goto(`${base}?step=${step}&theme=${theme}`);
      await page
        .getByRole('heading', {
          name: step === 'connect' ? 'Connect to a server' : step === 'pair' ? 'Pair this computer' : 'Choose printers',
          exact: true,
        })
        .waitFor();
      const layout = await page.evaluate(() => ({
        width: innerWidth,
        height: innerHeight,
        column: document.querySelector('.onboarding-column').getBoundingClientRect().width,
        pageScroll: document.documentElement.scrollHeight > innerHeight,
        overflow: document.documentElement.scrollWidth > innerWidth,
        bottom: document.querySelector('.onboarding-column').getBoundingClientRect().bottom,
        footer: document.querySelector('footer').getBoundingClientRect().top,
      }));
      assert.equal(layout.column, 360);
      assert.equal(layout.pageScroll, false);
      assert.equal(layout.overflow, false);
      assert.ok(layout.bottom <= layout.footer);
      observations.push({ step, theme, ...layout });
    }
}
await page.setViewportSize({ width: 800, height: 600 });
await page.goto(`${base}?step=connect&theme=light`);
await page.getByRole('button', { name: 'Continue', exact: true }).click();
await page.getByRole('heading', { name: 'Pair this computer', exact: true }).waitFor();
await page.getByLabel('Pairing code', { exact: true }).fill(' abcd- efgh ');
assert.equal(await page.getByLabel('Pairing code', { exact: true }).inputValue(), 'ABCD-EFGH');
await page.getByRole('button', { name: 'Back', exact: true }).click();
await page.getByRole('button', { name: 'Continue', exact: true }).click();
assert.equal(await page.getByLabel('Pairing code', { exact: true }).inputValue(), 'ABCD-EFGH');
await page.getByRole('button', { name: 'Pair computer', exact: true }).click();
await page.getByRole('heading', { name: 'Choose printers', exact: true }).waitFor();
assert.equal(await page.getByRole('switch', { checked: true }).count(), 0);
await page.getByRole('switch').first().click();
assert.equal(await page.getByRole('switch', { checked: true }).count(), 1);
await page.getByRole('button', { name: 'Finish setup', exact: true }).click();
await page.getByRole('heading', { name: 'Overview', exact: true }).waitFor();
assert.equal(await page.evaluate(() => localStorage.getItem('oppa-onboarding-active')), null);
await page.goto(`${base}?step=connect&theme=dark&deep-link`);
await page.getByRole('heading', { name: 'Pair this computer', exact: true }).waitFor();
assert.equal(await page.getByLabel('Pairing code', { exact: true }).inputValue(), 'LINK-CODE');
assert.ok(!(await page.evaluate(() => localStorage.getItem('oppa-setup-progress').includes('LINK-CODE'))));
await page.goto(`${base}?step=pair&theme=light&resume`);
await page.getByRole('heading', { name: 'Pair this computer', exact: true }).waitFor();
assert.equal(await page.getByLabel('Pairing code', { exact: true }).inputValue(), '');
await page.goto(`${base}?step=printers&theme=dark&empty`);
await page.getByText('No printers discovered.', { exact: false }).waitFor();
await page.screenshot({ path: 'apps/oppa/design-review/screenshots/empty-printers-800x600.png' });
await page.goto(`${base}?step=connect&theme=dark&existing&deep-link&state=connected`);
await page.getByRole('dialog').waitFor();
assert.ok(await page.getByText('Replace this computer’s pairing?', { exact: true }).isVisible());
await page.getByRole('button', { name: 'Cancel', exact: true }).click();
await page.getByRole('heading', { name: 'Overview', exact: true }).waitFor();
await page.goto(`${base}?step=connect&theme=light&state=credential_revoked&paired`);
await page.getByText('This computer’s access was revoked.', { exact: false }).waitFor();
await page.getByRole('button', { name: 'Continue', exact: true }).click();
await page.getByRole('dialog').waitFor();
await page.getByRole('dialog').getByRole('button', { name: 'Continue', exact: true }).click();
await page.getByRole('heading', { name: 'Pair this computer', exact: true }).waitFor();
await page.goto(`${base}?step=connect&theme=light&state=authentication_failed&paired`);
await page.getByRole('button', { name: 'Retry existing connection', exact: true }).click();
await page.getByRole('heading', { name: 'Choose printers', exact: true }).waitFor();
assert.deepEqual(errors, []);
console.log(
  JSON.stringify(
    {
      layouts: observations.length,
      interactionChecks:
        'connect, normalized paste, Back, pairing, explicit printer enable, finish, deep link, safe resume, empty state',
      errors,
    },
    null,
    2,
  ),
);
await browser.close();
