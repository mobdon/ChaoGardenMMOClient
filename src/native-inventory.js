// The carried-items window, drawn only from the game's own pieces: the Chao
// Select window (add_select_race_en 104,4 40x60: 16 px bands, 8 px stripe
// rows, 2 px sides), its flat cursor (127,127,255) and black message bar, the
// al_panel item icons, and the 22px ad_22ascii font. The game has no inventory
// screen; this layout is the Chao Select screen's, measured at 640x480.
export const WINDOW = { width: 240, height: 202 }; // smallest window; it grows a row at a time
const PIECE = { x: 104, y: 4, w: 40, h: 60, band: 16, side: 2, stripe: 8 };
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
  const { x, y, w, h, band, side, stripe } = PIECE, W = WINDOW.width;
  const columns = [[0, side, 0, side], [side, w - 2 * side, side, W - 2 * side], [w - side, side, W - side, side]];
  for (const [sx, sw, dx, dw] of columns) {
    ctx.drawImage(atlas, x + sx, y, sw, band, dx, 0, dw, band);
    ctx.drawImage(atlas, x + sx, y + h - band, sw, band, dx, H - band, dw, band);
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
    if (item.icon?.complete && item.icon.naturalWidth) ctx.drawImage(item.icon, x + 8, y + 7, 32, 32);
    const count = `x${item.quantity}`;
    const width = [...count].reduce((sum, c) => sum + ((metrics[c]?.[1] ?? 6) + 3) * FONT.scale, 0);
    drawText(ctx, font, metrics, count, x + GRID.cursor[0] - width, y + GRID.cursor[1] - 16);
  });
  ctx.fillStyle = '#000';
  ctx.fillRect(0, size.window, size.width, BAR);
  const label = items[selected]?.label;
  if (label) drawText(ctx, font, metrics, label, 8, size.window + 8);
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
