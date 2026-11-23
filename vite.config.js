import { defineConfig } from 'vite';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { nativePhonePreview } from './scripts/native-phone-preview.mjs';
import { widgetOriginalArt } from './scripts/widget-art.mjs';

const privateDirectory = /(?:^|\/)(?:\.local|\.codex-attachments|server|scripts|tests|archive)(?:\/|$)/i;

function privateSourceBoundary() {
  return {
    name: 'private-source-boundary',
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        let pathname = (request.url || '/').split('?')[0];
        try {
          for (let pass = 0; pass < 3; pass++) {
            const decoded = decodeURIComponent(pathname);
            if (decoded === pathname) break;
            pathname = decoded;
          }
        } catch {
          response.writeHead(400).end('Invalid request path.');
          return;
        }
        pathname = pathname.replaceAll('\\', '/');
        if (privateDirectory.test(pathname)) {
          response.writeHead(403, { 'Content-Type': 'text/plain; charset=utf-8', 'Cache-Control': 'no-store' });
          response.end('Private development files are not served.');
          return;
        }
        next();
      });
    },
  };
}

// A single new locally imported asset can be previewed without replacing the
// frozen Rust authority. The Rust service has the matching exact allowlist;
// no original pixels are committed or included in a distributable build.
function nativeChaoShadowPreview() {
  return {
    name: 'native-chao-shadow-preview',
    configureServer(server) {
      server.middlewares.use(async (request, response, next) => {
        const pathname = (request.url || '').split('?')[0];
        const water=/^\/api\/assets\/(water-(?:dark|hero|garden))\.glb$/.exec(pathname)?.[1];
        if (!water && pathname !== '/api/assets/chao-shadow.png') return next();
        if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405).end(); return; }
        try {
          const bytes = await readFile(new URL(water ? './.local/imports/'+water+'.glb' : './.local/imports/chao-shadow.png', import.meta.url));
          response.writeHead(200, { 'Content-Type': water ? 'model/gltf-binary' : 'image/png', 'Content-Length': bytes.length, 'Cache-Control': 'no-store' });
          response.end(request.method === 'HEAD' ? undefined : bytes);
        } catch { response.writeHead(404).end('Local native shadow is unavailable.'); }
      });
    },
  };
}

export default defineConfig({
  plugins: [privateSourceBoundary(), widgetOriginalArt(), nativeChaoShadowPreview(), nativePhonePreview(fileURLToPath(new URL('.', import.meta.url)))],
  server: {
    host: '127.0.0.1',
    port: Number(process.env.FRONTEND_PORT || 5173),
    strictPort: true,
    watch: { ignored: ['**/.local/**', '**/.codex-attachments/**', '**/server/**'] },
    fs: {
      strict: true,
      deny: [
        '.env', '.env.*', '*.{crt,pem,key,p12,pfx,cer,der}', '.npmrc', '.yarnrc.yml', '**/.git/**',
        '**/.local/**', '**/.codex-attachments/**', '**/server/**', '**/scripts/**', '**/tests/**', '**/archive/**',
        '**/*.{gba,sav,srm}',
      ],
    },
    proxy: {
      '/api': { target: `http://127.0.0.1:${process.env.GAME_PORT || 3001}` },
      '/ws': { target: `ws://127.0.0.1:${process.env.GAME_PORT || 3001}`, ws: true },
    },
  },
  build: { target: 'es2022', rolldownOptions: { input: { main: 'index.html', tiny: 'tiny.html', widget: 'widget.html', phone: 'phone.html' } } },
});
