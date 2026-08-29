import { createAsyncAPI } from '@fumadocs/asyncapi/server';

/** The checked-in, private-system AsyncAPI snapshot rendered by Fumadocs. */
export const asyncapi = createAsyncAPI({
  input: {
    'openprinter-cloud': './asyncapi/openprinter-cloud.yml',
  },
});
