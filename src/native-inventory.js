// The carried-items window, drawn only from the game's own pieces: the Chao
// Select window (add_select_race_en 104,4 40x60: 16 px bands, 8 px stripe
// rows, 2 px sides), its flat cursor (127,127,255) and black message bar, the
// al_panel item icons, and the 22px ad_22ascii font. The game has no inventory
// screen; this layout is the Chao Select screen's, measured at 640x480.
export const WINDOW = { width: 240, height: 202 }; // smallest window; it grows a row at a time
const PIECE = { x: 104, y: 4, w: 40, h: 60, band: 16, side: 2, corner: 12, stripe: 8 }; // corner: the bands' rounded ends, kept unstretched
const GRID = { x: 12, y: 24, pitch: 56, columns: 4, cursor: [48, 46] };
const BAR = 32; // message bar height below the window (screen y 413-444)

const FONT = { cell: 22, columns: 23, first: 33, scale: 16 / 22, threshold: 165 };

/** Slot n's top-left inside the window: four across, as Chao Select spaces its Chao. */
export function slotAt(n) {
  return { x: GRID.x + (n % GRID.columns) * GRID.pitch, y: GRID.y + Math.floor(n / GRID.columns) * GRID.pitch };
}
/** The canvas for `count` items: the window grows so every item has a slot, then the message bar. */
export function canvasSize(count) {
  const rows = Math.ceil(count / GRID.columns);
  const height = Math.max(WINDOW.height, GRID.y + rows * GRID.pitch + PIECE.band);
  return { width: WINDOW.width, height: height + BAR, window: height };
}
export const SLOT = { width: GRID.cursor[0], height: GRID.cursor[1] };

/**
 * Each glyph's [left, width] from the font image itself: a column is ink when
 * it holds a pixel with alpha 255 and red at or above the threshold (the rule
 * that reproduces every hand-measured stat label glyph).
 */
export function measureFont({ data, width }) {
  const metrics = {};
  for (let code = FONT.first; code < 127; code++) {
    const index = code - FONT.first, cx = (index % FONT.columns) * FONT.cell, cy = Math.floor(index / FONT.columns) * FONT.cell;
    let left = -1, right = -1;
    for (let x = 0; x < FONT.cell; x++) {
      for (let y = 0; y < FONT.cell; y++) {
        const p = ((cy + y) * width + cx + x) * 4;
        if (data[p + 3] === 255 && data[p] >= FONT.threshold) { if (left < 0) left = x; right = x; break; }
      }
    }
    if (left >= 0) metrics[String.fromCharCode(code)] = [left, right - left + 1];
  }
  return metrics;
}

/** Draw `text` in the game's font; returns the width drawn. Spaces advance like the narrowest glyph. */
export function drawText(ctx, font, metrics, text, x, y) {
  let at = x;
  for (const character of text) {
    const m = metrics[character];
    if (!m) { at += 6 * FONT.scale; continue; }
    const index = character.codePointAt(0) - FONT.first;
    ctx.drawImage(font, (index % FONT.columns) * FONT.cell, Math.floor(index / FONT.columns) * FONT.cell, FONT.cell, FONT.cell,
      at - m[0] * FONT.scale, y, FONT.cell * FONT.scale, FONT.cell * FONT.scale);
    at += (m[1] + 3) * FONT.scale;
  }
  return at - x;
}

/** The Chao Select window, sliced from its texture: bands kept, stripes repeated, sides stretched down. */
function drawWindow(ctx, atlas, H) {
  const { x, y, w, h, band, side, corner, stripe } = PIECE, W = WINDOW.width;
  const slices = edge => [[0, edge, 0, edge], [edge, w - 2 * edge, edge, W - 2 * edge], [w - edge, edge, W - edge, edge]];
  for (const [sx, sw, dx, dw] of slices(corner)) {
    ctx.drawImage(atlas, x + sx, y, sw, band, dx, 0, dw, band);
    ctx.drawImage(atlas, x + sx, y + h - band, sw, band, dx, H - band, dw, band);
  }
  for (const [sx, sw, dx, dw] of slices(side)) {
    for (let dy = band; dy < H - band; dy += stripe) {
      const rows = Math.min(stripe, H - band - dy);
      ctx.drawImage(atlas, x + sx, y + band, sw, rows, dx, dy, dw, rows);
    }
  }
}

/**
 * The whole window: `items` are {icon: Image, quantity, label}; `selected`
 * gets the cursor and its name in the message bar.
 */
export function drawInventory(ctx, { atlas, font, metrics }, items, selected) {
  const size = canvasSize(items.length);
  ctx.imageSmoothingEnabled = false;
  ctx.clearRect(0, 0, size.width, size.height);
  drawWindow(ctx, atlas, size.window);
  items.forEach((item, n) => {
    const { x, y } = slotAt(n);
    if (n === selected) { ctx.fillStyle = 'rgb(127,127,255)'; ctx.fillRect(x, y, ...GRID.cursor); }
    if (item.icon) { ctx.imageSmoothingEnabled = true; ctx.drawImage(item.icon, x + 8, y + 2, 32, 32); ctx.imageSmoothingEnabled = false; }
    const count = `x${item.quantity}`;
    const width = [...count].reduce((sum, c) => sum + ((metrics[c]?.[1] ?? 6) + 3) * FONT.scale, 0);
    drawText(ctx, font, metrics, count, x + GRID.cursor[0] - width, y + GRID.cursor[1] - 13); // under the icon, not over it
  });
  ctx.fillStyle = '#000';
  ctx.fillRect(0, size.window, size.width, BAR);
  const label = items[selected]?.label;
  if (label) drawText(ctx, font, metrics, label, 8, size.window + 8);
}

const ROW = 24; // one Chao name per row, the font's 16 px plus the cursor's margin

/** A Chao Select window listing names, one per row, for picking a Chao. */
export function listSize(count) {
  const height = Math.max(WINDOW.height, GRID.y + count * ROW + PIECE.band);
  return { width: WINDOW.width, height: height + BAR, window: height };
}
export function rowAt(n) { return { x: GRID.x, y: GRID.y + n * ROW, width: WINDOW.width - 2 * GRID.x, height: ROW }; }

/** `rows` are {label, note}: the name in the row, `note` in the message bar when it's selected. */
export function drawChaoList(ctx, { atlas, font, metrics }, rows, selected) {
  const size = listSize(rows.length);
  ctx.imageSmoothingEnabled = false;
  ctx.clearRect(0, 0, size.width, size.height);
  drawWindow(ctx, atlas, size.window);
  rows.forEach((row, n) => {
    const box = rowAt(n);
    if (n === selected) { ctx.fillStyle = 'rgb(127,127,255)'; ctx.fillRect(box.x, box.y, box.width, box.height); }
    drawText(ctx, font, metrics, row.label, box.x + 6, box.y + 2);
  });
  ctx.fillStyle = '#000';
  ctx.fillRect(0, size.window, size.width, BAR);
  const note = rows[selected]?.note;
  if (note) drawText(ctx, font, metrics, note, 8, size.window + 8);
}

let art;
/** The window atlas and font from the server's native assets, with glyph widths measured once. Null when they can't load. */
export function loadInventoryArt(document = globalThis.document) {
  if (!globalThis.Image || !document?.createElement) return Promise.resolve(null);
  art ??= Promise.all(['chao-select-window.png', 'chao-stat-font.png'].map(name => new Promise(resolve => {
    const image = new Image(); image.onload = () => resolve(image); image.onerror = () => resolve(null); image.src = `/api/assets/${name}`;
  }))).then(([atlas, font]) => {
    if (!atlas || !font) return null;
    const canvas = document.createElement('canvas'); canvas.width = font.naturalWidth; canvas.height = font.naturalHeight;
    const ctx = canvas.getContext('2d'); ctx.drawImage(font, 0, 0);
    return { atlas, font, metrics: measureFont(ctx.getImageData(0, 0, canvas.width, canvas.height)) };
  });
  return art;
}

/**
 * Scale2x (EPX), the standard pixel-art doubler: each pixel becomes four, and a
 * corner takes a neighbour's colour only where two neighbours agree, so edges
 * smooth without inventing colours. `data` is RGBA; returns the doubled RGBA.
 */
export function scale2x(data, width, height) {
  const out = new Uint8ClampedArray(width * height * 16);
  const px = (x, y) => (Math.min(height - 1, Math.max(0, y)) * width + Math.min(width - 1, Math.max(0, x))) * 4;
  const same = (a, b) => data[a] === data[b] && data[a + 1] === data[b + 1] && data[a + 2] === data[b + 2] && data[a + 3] === data[b + 3];
  const put = (x, y, from) => out.set(data.subarray(from, from + 4), (y * width * 2 + x) * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const e = px(x, y), b = px(x, y - 1), d = px(x - 1, y), f = px(x + 1, y), h = px(x, y + 1);
      const edge = !same(b, h) && !same(d, f);
      put(2 * x, 2 * y, edge && same(d, b) ? d : e);
      put(2 * x + 1, 2 * y, edge && same(b, f) ? f : e);
      put(2 * x, 2 * y + 1, edge && same(d, h) ? d : e);
      put(2 * x + 1, 2 * y + 1, edge && same(h, f) ? f : e);
    }
  }
  return out;
}

/** A panel doubled twice (4x) for sharp icons on any screen; the game's own pixels, smoothed. */
export function upscalePanel(image, document = globalThis.document) {
  let width = image.naturalWidth, height = image.naturalHeight;
  const canvas = document.createElement('canvas'); canvas.width = width; canvas.height = height;
  const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0);
  let data = ctx.getImageData(0, 0, width, height).data;
  for (let pass = 0; pass < 2; pass++) { data = scale2x(data, width, height); width *= 2; height *= 2; }
  canvas.width = width; canvas.height = height;
  canvas.getContext('2d').putImageData(new ImageData(data, width, height), 0, 0);
  return canvas;
}
