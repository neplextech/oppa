import { readFileSync } from 'node:fs';

/**
 * Reads the OAuth client identifier from the product definition built by the
 * matching local OPPA example.
 */
export function loadExampleClientId(): string {
  const source = readFileSync(new URL('../product/product.json', import.meta.url), 'utf8');
  const product = JSON.parse(source) as unknown;
  if (
    typeof product !== 'object' ||
    product === null ||
    !('protocol' in product) ||
    typeof product.protocol !== 'object' ||
    product.protocol === null ||
    !('clientId' in product.protocol) ||
    typeof product.protocol.clientId !== 'string' ||
    product.protocol.clientId.trim() !== product.protocol.clientId ||
    product.protocol.clientId.length === 0 ||
    product.protocol.clientId.length > 256
  ) {
    throw new Error('The example product must define a non-empty protocol.clientId of at most 256 characters.');
  }
  return product.protocol.clientId;
}

/** OAuth client identifier shared by the example provider and product build. */
export const EXAMPLE_CLIENT_ID = loadExampleClientId();
