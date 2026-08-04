import { spawn } from 'node:child_process';
import { constants } from 'node:fs';
import { access, readFile, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const appDirectory = resolve(scriptDirectory, '..');
const repositoryRoot = resolve(appDirectory, '../..');
const tauriDirectory = join(appDirectory, 'src-tauri');
const baseConfigPath = join(tauriDirectory, 'tauri.conf.json');
const devConfigPath = join(tauriDirectory, 'tauri.conf.dev.json');
const generatedConfigPath = join(tauriDirectory, '.product-tauri.conf.json');

const mode = process.argv[2];
if (mode !== 'dev' && mode !== 'build') {
  throw new Error('Usage: node scripts/run-tauri.mjs <dev|build> [tauri arguments...]');
}

async function exists(path) {
  try {
    await access(path, constants.R_OK);
    return true;
  } catch {
    return false;
  }
}

async function resolveProductDirectory() {
  const configured = process.env.OPPA_PRODUCT_DIR;
  if (configured) {
    const dir = isAbsolute(configured) ? configured : resolve(process.cwd(), configured);
    return dir;
  }
  // Auto-select: in dev mode prefer products/oppa-dev for full isolation from the release build.
  const preferences = mode === 'dev' ? ['products/oppa-dev', 'products/default'] : ['products/default'];
  for (const relative of preferences) {
    const dir = resolve(repositoryRoot, relative);
    if (await exists(join(dir, 'product.json'))) {
      return dir;
    }
  }
  return resolve(repositoryRoot, 'products/default');
}

function requireProductString(product, key) {
  const value = product[key];
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`Product configuration field ${key} must be a non-empty string`);
  }
  return value;
}

function deepMerge(target, source) {
  for (const [key, value] of Object.entries(source)) {
    if (
      value !== null &&
      typeof value === 'object' &&
      !Array.isArray(value) &&
      target[key] !== null &&
      typeof target[key] === 'object' &&
      !Array.isArray(target[key])
    ) {
      deepMerge(target[key], value);
    } else {
      target[key] = value;
    }
  }
}

const defaultProductDir = resolve(repositoryRoot, 'products/default');
const productDirectory = await resolveProductDirectory();
const productPath = join(productDirectory, 'product.json');
const product = JSON.parse(await readFile(productPath, 'utf8'));
const config = JSON.parse(await readFile(baseConfigPath, 'utf8'));
const productName = requireProductString(product, 'productName');
const applicationId = requireProductString(product, 'applicationId');

config.productName = productName;
// Use a distinct identifier in dev when the default product is active; the dev product already
// carries a dev-specific applicationId so no suffix is needed.
const usingDefaultProduct = productDirectory === defaultProductDir;
const resolvedIdentifier = (mode === 'dev' && usingDefaultProduct) ? `${applicationId}.dev` : applicationId;
config.identifier = resolvedIdentifier;
// Expose the effective app identifier to Vite so the frontend can display it.
process.env.VITE_APP_IDENTIFIER = resolvedIdentifier;
if (Array.isArray(config.app?.windows)) {
  config.app.windows = config.app.windows.map((window) => ({ ...window, title: productName }));
}

if (Array.isArray(config.bundle?.icon)) {
  config.bundle.icon = await Promise.all(
    config.bundle.icon.map(async (defaultIcon) => {
      const productIcon = join(productDirectory, 'assets', defaultIcon.split(/[\\/]/u).at(-1));
      return (await exists(productIcon)) ? productIcon : defaultIcon;
    }),
  );
}

// Apply deepLinkScheme from product config so the OS registers the correct URL scheme.
// This is the single source of truth for the scheme; tauri.conf.json's scheme is the fallback.
const deepLinkScheme = typeof product.deepLinkScheme === 'string' && product.deepLinkScheme.trim() !== ''
  ? product.deepLinkScheme.trim()
  : null;
if (deepLinkScheme !== null) {
  config.plugins ??= {};
  config.plugins['deep-link'] ??= {};
  config.plugins['deep-link'].desktop = [{ schemes: [deepLinkScheme] }];
}

// Merge dev-only Tauri overrides (updater endpoints, bundle settings) on top of the product config.
if (mode === 'dev' && (await exists(devConfigPath))) {
  const devConfig = JSON.parse(await readFile(devConfigPath, 'utf8'));
  deepMerge(config, devConfig);
}

await writeFile(generatedConfigPath, `${JSON.stringify(config, null, 2)}\n`, 'utf8');

const pnpm = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const child = spawn(
  pnpm,
  ['--filter', 'oppa', 'exec', 'tauri', mode, '--config', generatedConfigPath, ...process.argv.slice(3)],
  {
    cwd: repositoryRoot,
    env: { ...process.env, OPPA_PRODUCT_DIR: productDirectory },
    stdio: 'inherit',
  },
);

child.once('error', (error) => {
  throw error;
});
child.once('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exitCode = code ?? 1;
  }
});
