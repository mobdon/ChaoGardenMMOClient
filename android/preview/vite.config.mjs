import { mergeConfig } from 'vite';
import base from '../../vite.config.js';
import { readFile } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';

// Only named, hash-pinned presentation assets are public. Never expose the
// private imports directory, ROMs, manifests or arbitrary filenames.
export const GARDEN_FILE = 'tiny-garden-a2ne.png';
export const GARDEN_SHA256 = '75eb4d2839418fc730773377a1135776a0c361cbb1b6967af8134d8d4b5469ae';
const catalog = JSON.parse(readFileSync(new URL('../docs/ORIGINAL-ASSETS.json', import.meta.url), 'utf8'));
export const CHAO_ASSETS = catalog.assets.filter(asset =>
  ['adapted-phone-atlas', 'original-dark-head-effect'].includes(asset.kind));
const emeralds = JSON.parse(readFileSync(new URL('../docs/WIDGET-EMERALDS.json', import.meta.url), 'utf8'));
export const EMERALD_ASSETS = emeralds.sprites;
const menus = JSON.parse(readFileSync(new URL('../docs/TINY-MENU-ASSETS.json', import.meta.url), 'utf8'));
export const MENU_ASSETS = menus.outputs.filter(asset => asset.previewRoute);
const assets = new Map([
  [`/__widget-art/${GARDEN_FILE}`, { filename: GARDEN_FILE, sha256: GARDEN_SHA256, limit: 256 * 1024 }],
  ...CHAO_ASSETS.map(asset => [asset.path, { filename: asset.path.split('/').at(-1), sha256: asset.sha256, limit: 4 * 1024 * 1024 }]),
  ...EMERALD_ASSETS.map(asset => [`/__widget-art/${asset.filename}`, { filename: asset.filename, sha256: asset.sha256, limit: 256 * 1024 }]),
  ...MENU_ASSETS.map(asset => [asset.previewRoute, { filename: asset.filename, sha256: asset.pngSha256, limit: 256 * 1024 }]),
]);

async function originalAsset(descriptor) {
  if (!process.env.TINY_GARDEN_ART_DIR) return null;
  let bytes;
  try { bytes = await readFile(resolve(process.env.TINY_GARDEN_ART_DIR, descriptor.filename)); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
  if (bytes.length > descriptor.limit || createHash('sha256').update(bytes).digest('hex') !== descriptor.sha256) {
    throw new Error(`Original widget artwork fingerprint mismatch: ${descriptor.filename}`);
  }
  return bytes;
}

export default mergeConfig(base, {
  plugins: [{
    name: 'widget-original-garden',
    configureServer(server) {
      server.middlewares.use(async (request, response, next) => {
        const route = (request.url || '').split('?')[0];
        const descriptor = assets.get(route);
        if (!descriptor) return next();
        try {
          const bytes = await originalAsset(descriptor);
          // Existing Rust asset routes remain the fallback for source-only setups.
          if (!bytes && route.startsWith('/api/assets/')) return next();
          if (!bytes) return response.writeHead(404).end('Original widget art is not configured.');
          response.writeHead(200, { 'Content-Type': 'image/png', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
          response.end(bytes);
        } catch {
          response.writeHead(404).end('Verified original widget art is unavailable.');
        }
      });
    },
    async generateBundle() {
      for (const [route, descriptor] of assets) {
        const bytes = await originalAsset(descriptor);
        if (bytes) this.emitFile({ type: 'asset', fileName: route.slice(1), source: bytes });
      }
    },
  }],
});
