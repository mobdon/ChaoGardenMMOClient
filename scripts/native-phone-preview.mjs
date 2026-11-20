import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, relative, isAbsolute } from 'node:path';

// Local presentation snapshot only. No credentials, save data or game routes
// are copied; Tiny Garden continues using the same-origin Rust authority.
export function nativePhonePreview(root) {
  const directory = resolve(root, process.env.TINY_PHONE_ISOLATED_BUILD === '1' ? '.local/native-phone-games' : '.local/native-phone');
  return {
    name: 'native-phone-preview',
    configureServer(server) {
      server.middlewares.use(async (request, response, next) => {
        const route = (request.url || '').split('?')[0];
        if (!route.startsWith('/phone-tiny/')) return next();
        if (!['GET', 'HEAD'].includes(request.method)) return response.writeHead(405).end();
        try {
          const manifest = JSON.parse(await readFile(resolve(directory, 'manifest.json'), 'utf8'));
          const entry = Object.hasOwn(manifest.routes, route) && manifest.routes[route];
          if (!entry) return response.writeHead(404).end('Unknown Tiny Garden presentation file.');
          const file = resolve(directory, entry.file);
          const rel = relative(directory, file);
          if (isAbsolute(rel) || rel.startsWith('..')) throw Error('Snapshot boundary');
          const bytes = await readFile(file);
          if (createHash('sha256').update(bytes).digest('hex') !== entry.sha256) throw Error('Snapshot changed');
          response.writeHead(200, { 'Content-Type': entry.mime, 'Content-Length': bytes.length,
            'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
          response.end(request.method === 'HEAD' ? undefined : bytes);
        } catch { response.writeHead(404).end('Build the local native Tiny Garden preview first.'); }
      });
    },
  };
}
