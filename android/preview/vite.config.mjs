import { mergeConfig } from 'vite';
import base from '../../vite.config.js';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';

// A single generated original image is public presentation data. Never expose
// the private imports directory, the ROM, a manifest path or arbitrary filenames.
export const GARDEN_FILE = 'tiny-garden-a2ne.png';
export const GARDEN_SHA256 = '75eb4d2839418fc730773377a1135776a0c361cbb1b6967af8134d8d4b5469ae';
const route = `/__widget-art/${GARDEN_FILE}`;

async function originalGarden() {
  if (!process.env.TINY_GARDEN_ART_DIR) return null;
  const bytes = await readFile(resolve(process.env.TINY_GARDEN_ART_DIR, GARDEN_FILE));
  if (bytes.length > 256 * 1024 || createHash('sha256').update(bytes).digest('hex') !== GARDEN_SHA256) {
    throw new Error('Original A2NE garden image fingerprint mismatch. Regenerate with android/tools/export-tiny-garden.ps1.');
  }
  return bytes;
}

export default mergeConfig(base, {
  plugins: [{
    name: 'widget-original-garden',
    configureServer(server) {
      server.middlewares.use(async (request, response, next) => {
        if ((request.url || '').split('?')[0] !== route) return next();
        try {
          const bytes = await originalGarden();
          if (!bytes) return response.writeHead(404).end('Original garden art is not configured.');
          response.writeHead(200, { 'Content-Type': 'image/png', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
          response.end(bytes);
        } catch {
          response.writeHead(404).end('Verified original garden art is unavailable.');
        }
      });
    },
    async generateBundle() {
      const bytes = await originalGarden();
      if (bytes) this.emitFile({ type: 'asset', fileName: `__widget-art/${GARDEN_FILE}`, source: bytes });
    },
  }],
});
