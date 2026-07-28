import type { IncomingMessage, ServerResponse } from 'node:http';

/** A structured HTTP failure safe to return from the example API. */
export class HttpError extends Error {
  public readonly code: string;
  public readonly status: number;

  public constructor(status: number, code: string, message: string) {
    super(message);
    this.name = 'HttpError';
    this.status = status;
    this.code = code;
  }
}

/** Read a bounded request body without assuming a web framework. */
export async function readBody(request: IncomingMessage, maximumBytes: number): Promise<Buffer> {
  const declaredLength = Number(request.headers['content-length'] ?? '0');

  if (Number.isFinite(declaredLength) && declaredLength > maximumBytes) {
    request.resume();
    throw new HttpError(413, 'body_too_large', `The request body must not exceed ${maximumBytes} bytes.`);
  }

  const chunks: Buffer[] = [];
  let length = 0;

  for await (const value of request as AsyncIterable<Uint8Array>) {
    const chunk = Buffer.from(value);
    length += chunk.byteLength;

    if (length > maximumBytes) {
      throw new HttpError(413, 'body_too_large', `The request body must not exceed ${maximumBytes} bytes.`);
    }

    chunks.push(chunk);
  }

  return Buffer.concat(chunks, length);
}

/** Parse a bounded JSON object request. */
export async function readJson(request: IncomingMessage, maximumBytes: number): Promise<unknown> {
  requireContentType(request, 'application/json');
  const body = await readBody(request, maximumBytes);

  try {
    return JSON.parse(body.toString('utf8')) as unknown;
  } catch {
    throw new HttpError(400, 'invalid_json', 'The request body must contain valid JSON.');
  }
}

/** Parse a bounded `application/x-www-form-urlencoded` request. */
export async function readForm(request: IncomingMessage, maximumBytes: number): Promise<URLSearchParams> {
  requireContentType(request, 'application/x-www-form-urlencoded');
  const body = await readBody(request, maximumBytes);
  return new URLSearchParams(body.toString('utf8'));
}

/** Send a JSON response with conservative security headers. */
export function sendJson(
  response: ServerResponse,
  status: number,
  value: unknown,
  headers?: Readonly<Record<string, string>>,
): void {
  const body = Buffer.from(`${JSON.stringify(value)}\n`, 'utf8');
  response.writeHead(status, {
    'cache-control': 'no-store',
    'content-length': String(body.byteLength),
    'content-type': 'application/json; charset=utf-8',
    'x-content-type-options': 'nosniff',
    ...headers,
  });
  response.end(body);
}

/** Send an HTML response used only by the local approval page. */
export function sendHtml(response: ServerResponse, status: number, html: string): void {
  const body = Buffer.from(html, 'utf8');
  response.writeHead(status, {
    'cache-control': 'no-store',
    'content-length': String(body.byteLength),
    'content-security-policy': "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'self'",
    'content-type': 'text/html; charset=utf-8',
    'referrer-policy': 'no-referrer',
    'x-content-type-options': 'nosniff',
    'x-frame-options': 'DENY',
  });
  response.end(body);
}

/** Escape interpolated text before including it in the approval document. */
export function escapeHtml(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (character) =>
      ({
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;',
      })[character] ?? character,
  );
}

function requireContentType(request: IncomingMessage, expected: string): void {
  const contentType = request.headers['content-type']?.split(';', 1)[0]?.trim().toLowerCase();

  if (contentType !== expected) {
    throw new HttpError(415, 'unsupported_media_type', `Use Content-Type: ${expected}.`);
  }
}
