// Xcode invokes this bridge directly so pnpm cannot reinstall dependencies in
// a build phase whose environment differs from the CI install step.
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptPath = fileURLToPath(import.meta.url);

/** Replaces the generated pnpm build command while preserving Xcode arguments. */
export function patchIosBuildPhase(source, path, pbxproj) {
  const quotedPath = `'${path.replaceAll("'", "'\\''")}'`;
  const command = `node ${quotedPath}`;
  if (pbxproj) {
    return source.replace(/shellScript = ("(?:[^"\\]|\\.)*");/g, (match, encoded) => {
      const script = JSON.parse(encoded);
      const updated = script.replace('pnpm tauri ios xcode-script', command);
      return updated === script ? match : `shellScript = ${JSON.stringify(updated)};`;
    });
  }
  return source.replace('script: pnpm tauri ios xcode-script', `script: ${command}`);
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const require = createRequire(import.meta.url);
  const cli = require.resolve('@tauri-apps/cli/tauri.js');
  const child = spawn(process.execPath, [cli, 'ios', 'xcode-script', ...process.argv.slice(2)], {
    stdio: 'inherit',
    env: {
      ...process.env,
      // Xcode prepends its toolchain to PATH, so restore the swift-rs shim.
      PATH: `${join(dirname(scriptPath), 'apple-toolchain')}:${process.env.PATH ?? ''}`,
    },
  });
  child.once('error', (error) => {
    throw error;
  });
  child.once('exit', (code, signal) => {
    if (signal) process.kill(process.pid, signal);
    else process.exitCode = code ?? 1;
  });
}
