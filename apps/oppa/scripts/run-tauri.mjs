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
  const configured = process.env.OPPA_PRODUCT_DIR ?? 'products/default';
  if (isAbsolute(configured)) {
    return configured;
  }

  const candidates = [resolve(process.cwd(), configured), resolve(repositoryRoot, configured)];
  for (const candidate of candidates) {
    if (await exists(join(candidate, 'product.json'))) {
      return candidate;
    }
  }
  return candidates[0];
}

function requireProductString(product, key) {
  const value = product[key];
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`Product configuration field ${key} must be a non-empty string`);
  }
  return value;
}

const productDirectory = await resolveProductDirectory();
const productPath = join(productDirectory, 'product.json');
const product = JSON.parse(await readFile(productPath, 'utf8'));
const config = JSON.parse(await readFile(baseConfigPath, 'utf8'));
const productName = requireProductString(product, 'productName');
const applicationId = requireProductString(product, 'applicationId');

config.productName = productName;
// Use a distinct identifier in dev so tauri dev data is isolated from the release build.
config.identifier = mode === 'dev' ? `${applicationId}.dev` : applicationId;
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
