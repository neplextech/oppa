import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { OpenPrinterAuthenticationSchema, OpenPrinterProtocolSchema } from '../dist/index.js';

const outputUrl = new URL('../../../protocol/schema/openprinter.schema.json', import.meta.url);

function canonicalize(value) {
  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])]),
    );
  }
  return value;
}

const output = `${JSON.stringify(
  canonicalize({
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    title: 'OpenPrinter v1 contracts',
    anyOf: [OpenPrinterProtocolSchema, OpenPrinterAuthenticationSchema],
  }),
  null,
  2,
)}\n`;

if (process.argv.includes('--check')) {
  let committed;
  try {
    committed = await readFile(outputUrl, 'utf8');
  } catch {
    console.error(`Missing generated schema: ${fileURLToPath(outputUrl)}. Run pnpm schema:generate.`);
    process.exitCode = 1;
  }

  if (committed !== undefined && committed !== output) {
    console.error(`Generated schema is stale: ${fileURLToPath(outputUrl)}. Run pnpm schema:generate.`);
    process.exitCode = 1;
  }
} else {
  await writeFile(outputUrl, output);
  console.log(`Wrote ${fileURLToPath(outputUrl)}`);
}
