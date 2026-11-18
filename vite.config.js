import { defineConfig } from 'vite';

const privateDirectory = /(?:^|\/)(?:\.local|\.codex-attachments|server|scripts|tests)(?:\/|$)/i;

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

export default defineConfig({
  plugins: [privateSourceBoundary()],
  server: {
    host: '127.0.0.1',
    port: Number(process.env.FRONTEND_PORT || 5173),
    strictPort: true,
    watch: { ignored: ['**/.local/**', '**/.codex-attachments/**', '**/server/**'] },
    fs: {
      strict: true,
      deny: [
        '.env', '.env.*', '*.{crt,pem,key,p12,pfx,cer,der}', '.npmrc', '.yarnrc.yml', '**/.git/**',
        '**/.local/**', '**/.codex-attachments/**', '**/server/**', '**/scripts/**', '**/tests/**',
        '**/*.{gba,sav,srm}',
      ],
    },
    proxy: {
      '/api': { target: `http://127.0.0.1:${process.env.GAME_PORT || 3001}` },
      '/ws': { target: `ws://127.0.0.1:${process.env.GAME_PORT || 3001}`, ws: true },
    },
  },
  build: { target: 'es2022', rolldownOptions: { input: { main: 'index.html', tiny: 'tiny.html', widget: 'widget.html' } } },
});
