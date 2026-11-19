import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const routes = new Map([
  ['/', { file: new URL('../preview/index.html', import.meta.url), type: 'text/html; charset=utf-8' }],
  ['/runtime.js', { file: new URL('../preview/runtime.js', import.meta.url), type: 'text/javascript; charset=utf-8' }],
  ['/runtime.css', { file: new URL('../preview/runtime.css', import.meta.url), type: 'text/css; charset=utf-8' }],
  ['/runtime.wasm', { file: new URL('../wasm/target/wasm32-unknown-unknown/release/asoe_runtime_wasm.wasm', import.meta.url), type: 'application/wasm' }],
]);

const headers = {
  'Cache-Control': 'no-store',
  'X-Content-Type-Options': 'nosniff',
  'Cross-Origin-Resource-Policy': 'same-origin',
  'Referrer-Policy': 'no-referrer',
  'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
  'Content-Security-Policy': "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
};

function send(request, response, status, body, type = 'text/plain; charset=utf-8', extra = {}) {
  response.writeHead(status, { ...headers, 'Content-Type': type, 'Content-Length': Buffer.byteLength(body), ...extra });
  response.end(request.method === 'HEAD' ? undefined : body);
}

/** The only files served are the four literal routes above. No directory,
 * ROM, save, source map, attachment, extracted output or input endpoint exists. */
export function createPreviewServer() {
  const server = createServer(async (request, response) => {
    const address = server.address();
    const origin = `http://127.0.0.1:${address.port}`;
    const documentNavigation = request.url === '/' && request.headers['sec-fetch-mode'] === 'navigate' && request.headers['sec-fetch-dest'] === 'document';
    const crossSiteSubresource = request.headers['sec-fetch-site'] === 'cross-site' && !documentNavigation;
    if (request.headers.host !== `127.0.0.1:${address.port}` || (request.headers.origin && request.headers.origin !== origin) || crossSiteSubresource) {
      send(request, response, 403, 'Loopback same-origin access only.');
      return;
    }
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      send(request, response, 405, 'Only GET and HEAD are supported.', undefined, { Allow: 'GET, HEAD', Connection: 'close' });
      return;
    }
    if (request.headers['transfer-encoding'] || (request.headers['content-length'] && request.headers['content-length'] !== '0')) {
      send(request, response, 400, 'Request bodies are unsupported.', undefined, { Connection: 'close' });
      return;
    }
    const route = routes.get(request.url);
    if (!route) {
      send(request, response, 404, 'Not found.');
      return;
    }
    try {
      const info = await stat(route.file);
      if (!info.isFile() || info.size > 16 * 1024 * 1024) throw new Error('Unsupported static file');
      const bytes = await readFile(route.file);
      if (bytes.length > 16 * 1024 * 1024) throw new Error('Static file grew beyond its limit');
      send(request, response, 200, bytes, route.type);
    } catch {
      send(request, response, 503, 'Build the local Rust/WASM runtime, then reload the preview.');
    }
  });
  server.requestTimeout = 30_000;
  server.headersTimeout = 10_000;
  server.keepAliveTimeout = 5_000;
  server.maxRequestsPerSocket = 100;
  return server;
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length !== 2 || args[0] !== '--port' || !/^\d+$/.test(args[1])) {
    throw new Error('Usage: node tools/serve-runtime.mjs --port PORT (use 0 for an available port)');
  }
  const port = Number(args[1]);
  if (!Number.isSafeInteger(port) || port < 0 || port > 65535) throw new Error('Port must be an integer from 0 through 65535.');
  const server = createPreviewServer();
  await new Promise((resolveListening, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', resolveListening);
  });
  console.log(`Sonic Advance local preview: http://127.0.0.1:${server.address().port}`);
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => server.close());
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
