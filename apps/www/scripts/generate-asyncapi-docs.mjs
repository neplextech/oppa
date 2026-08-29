import { readdir, readFile, writeFile } from 'node:fs/promises';

import { generateFiles } from '@fumadocs/asyncapi';
import { createAsyncAPI } from '@fumadocs/asyncapi/server';
import { format } from 'oxfmt';

const asyncapi = createAsyncAPI({
  input: {
    'openprinter-cloud': './asyncapi/openprinter-cloud.yml',
  },
});

await generateFiles({
  input: asyncapi,
  output: './content/docs/agent-gateway',
  per: 'operation',
  includeDescription: true,
  meta: false,
});

const outputEntries = await readdir('./content/docs/agent-gateway', { withFileTypes: true });
const formatOptions = {
  arrowParens: 'always',
  bracketSpacing: true,
  endOfLine: 'lf',
  insertFinalNewline: true,
  printWidth: 70,
  proseWrap: 'always',
  semi: true,
  singleQuote: true,
  trailingComma: 'all',
};
await Promise.all(
  outputEntries
    .filter((entry) => entry.isFile() && entry.name.endsWith('.mdx'))
    .map(async (entry) => {
      const path = `./content/docs/agent-gateway/${entry.name}`;
      const result = await format(path, await readFile(path, 'utf8'), formatOptions);
      if (result.errors.length > 0) {
        throw new Error(`Could not format generated AsyncAPI page ${path}.`);
      }
      await writeFile(path, result.code, 'utf8');
    }),
);
