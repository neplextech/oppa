import { spawn } from 'node:child_process';
import { constants } from 'node:fs';
import { access, mkdir, rename } from 'node:fs/promises';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const appDirectory = resolve(scriptDirectory, '..');
const iconsDirectory = join(appDirectory, 'src-tauri', 'icons');
const sourceIconPath = join(appDirectory, 'public', 'oppa-icon.png');
const iconIcoPath = join(iconsDirectory, 'icon.ico');
const iconIcnsPath = join(iconsDirectory, 'icon.icns');

const pngOutputs = [
  ['icon.png', 512],
  ['32x32.png', 32],
  ['64x64.png', 64],
  ['128x128.png', 128],
  ['128x128@2x.png', 256],
  ['Square30x30Logo.png', 30],
  ['Square44x44Logo.png', 44],
  ['Square71x71Logo.png', 71],
  ['Square89x89Logo.png', 89],
  ['Square107x107Logo.png', 107],
  ['Square142x142Logo.png', 142],
  ['Square150x150Logo.png', 150],
  ['Square284x284Logo.png', 284],
  ['Square310x310Logo.png', 310],
  ['StoreLogo.png', 50],
];

const iconsetOutputs = [
  ['icon_16x16.png', 16],
  ['icon_16x16@2x.png', 32],
  ['icon_32x32.png', 32],
  ['icon_32x32@2x.png', 64],
  ['icon_128x128.png', 128],
  ['icon_128x128@2x.png', 256],
  ['icon_256x256.png', 256],
  ['icon_256x256@2x.png', 512],
  ['icon_512x512.png', 512],
  ['icon_512x512@2x.png', 1024],
];

async function exists(path) {
  try {
    await access(path, constants.R_OK);
    return true;
  } catch {
    return false;
  }
}

function run(command, args, options = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, {
      stdio: 'inherit',
      ...options,
    });

    child.once('error', rejectPromise);
    child.once('exit', (code, signal) => {
      if (signal) {
        rejectPromise(new Error(`${command} was terminated by signal ${signal}`));
        return;
      }

      if (code !== 0) {
        rejectPromise(new Error(`${command} exited with code ${code ?? 1}`));
        return;
      }

      resolvePromise();
    });
  });
}

async function renderPng(inputPath, outputPath, size) {
  const tempOutputPath = join(dirname(outputPath), `${basename(outputPath)}.tmp.png`);

  await run('ffmpeg', [
    '-hide_banner',
    '-loglevel',
    'error',
    '-y',
    '-i',
    inputPath,
    '-map_metadata',
    '-1',
    '-vf',
    `scale=${size}:${size}:force_original_aspect_ratio=decrease:flags=lanczos,pad=${size}:${size}:(ow-iw)/2:(oh-ih)/2:color=0x00000000`,
    '-frames:v',
    '1',
    tempOutputPath,
  ]);

  await rename(tempOutputPath, outputPath);
}

async function renderIco(inputPath, outputPath, size) {
  const tempOutputPath = join(dirname(outputPath), `${basename(outputPath)}.tmp.ico`);

  await run('ffmpeg', [
    '-hide_banner',
    '-loglevel',
    'error',
    '-y',
    '-i',
    inputPath,
    '-map_metadata',
    '-1',
    '-vf',
    `scale=${size}:${size}:force_original_aspect_ratio=decrease:flags=lanczos,pad=${size}:${size}:(ow-iw)/2:(oh-ih)/2:color=0x00000000`,
    '-frames:v',
    '1',
    '-f',
    'ico',
    tempOutputPath,
  ]);

  await rename(tempOutputPath, outputPath);
}

async function main() {
  if (!(await exists(sourceIconPath))) {
    throw new Error(`Missing source icon at ${sourceIconPath}`);
  }

  if (process.platform !== 'darwin') {
    throw new Error('generate-icons.mjs requires macOS because it uses iconutil to build icon.icns');
  }

  const tempRoot = await mkdtemp(join(tmpdir(), 'oppa-icons-'));

  try {
    for (const [fileName, size] of pngOutputs) {
      await renderPng(sourceIconPath, join(iconsDirectory, fileName), size);
    }

    const iconsetDirectory = join(tempRoot, 'icon.iconset');
    await mkdir(iconsetDirectory, { recursive: true });

    for (const [fileName, size] of iconsetOutputs) {
      await renderPng(sourceIconPath, join(iconsetDirectory, fileName), size);
    }

    await rm(iconIcnsPath, { force: true });
    await run('iconutil', ['--convert', 'icns', '--output', iconIcnsPath, iconsetDirectory]);

    await rm(iconIcoPath, { force: true });
    await renderIco(sourceIconPath, iconIcoPath, 256);
  } finally {
    await rm(tempRoot, { recursive: true, force: true });
  }
}

await main();
