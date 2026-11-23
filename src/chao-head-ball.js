// The head ball's colour, as the game works it out: a port of AL_CalcIconColor (GC ChaoMain.rel .text 0x14574).
// `tables` = the game's three 108-byte colour tables (neutral, hero, dark): six rows (child, normal, swim, fly,
// run, power) of six RGB colours (zero, normal, swim, fly, run, power).

const f = Math.fround, TINY = f(1e-6);
// Types with one fixed colour (jump table .data 0x315C): the three Chaos Chao, Tails, Knuckles, Amy.
const FIXED = { 20: [255, 255, 255], 21: [255, 255, 0], 22: [255, 255, 255], 23: [76, 255, 0], 24: [76, 255, 0], 25: [255, 0, 0] };

/** One row's colour: zero colour -> (run/power, swim/fly mix) by growth. */
function row(table, at, h, v, growth) {
  const c = (n, i) => table[at + n * 3 + i];
  const hi = h < 0 ? 4 : 5, vi = v < 0 ? 2 : 3; h = Math.abs(h); v = Math.abs(v);
  const inv = f(1 / f(h + v)), wh = f(inv * h), wv = f(inv * v);
  return [0, 1, 2].map(i => {
    const side = f(f(1 - v) * c(1, i) + f(v * c(vi, i))), front = f(f(1 - h) * c(1, i) + f(h * c(hi, i)));
    return f(f(f(wh * front + f(wv * side)) * growth) + f(1 - growth) * c(0, i));
  });
}

/** [r, g, b] for a Chao's `look` {type, hPos, vPos, aPos, growth}, or null for a type the game gives no table row. */
export function headBallColour(tables, look) {
  const type = look.type;
  if (FIXED[type]) return FIXED[type];
  if (type !== 2 && !(type >= 5 && type <= 19)) return null;
  const clamp = x => Math.max(-1, Math.min(1, f(x) === 0 ? TINY : f(x)));
  const h = clamp(look.hPos), v = clamp(look.vPos), growth = Math.min(1, f(look.growth));
  // An adult: its own family's table, the row of its second evolution (jump table .data 0x31C4).
  if (type !== 2) return row(tables[(type - 5) % 3], (Math.floor((type - 5) / 3) + 1) * 18, h, v, growth).map(Math.trunc);
  // A child: the neutral child row, mixed toward the hero or dark child row by alignment.
  const a = Math.max(-1, Math.min(1, f(look.aPos))), w = Math.abs(a);
  const own = row(tables[0], 0, h, v, growth), other = row(tables[a >= 0 ? 1 : 2], 0, h, v, growth);
  return own.map((x, i) => Math.trunc(f(f(other[i] * w) + f(x * f(1 - w)))));
}

/**
 * What floats over the head of a Chao of `type`, as AL_IconDrawSub picks it (GC ChaoMain.rel .text 0x12F0C..0x132B4,
 * ball type 0): the al_icon picture, the size against the plain ball, and whether it is added to the screen
 * (SRC_ALPHA, ONE) instead of blended.
 */
export function headIcon(type) {
  if (type >= 23 && type <= 25) return { kind: 'ball', picture: 0, scale: 1, additive: false };
  // Hero types 6, 9, .. 21: the ring, one of 35 pictures by its tilt (see haloPicture); Hero Chaos adds it.
  if (type >= 6 && (type - 6) % 3 === 0) return { kind: 'halo', scale: 2, additive: type === 21 };
  // Dark types 7, 10, .. 19: the spike ball; Dark Chaos (22): the 16-picture flame, one picture per 3 frames.
  if (type >= 7 && (type - 7) % 3 === 0) return type === 22 ? { kind: 'flame', scale: 1.5, additive: true } : { kind: 'spike', picture: 5, scale: 1.5, additive: false };
  // Neutral Chaos: picture 6, turned to a new random angle every frame.
  if (type === 20) return { kind: 'glow', picture: 6, scale: 1.5, additive: true };
  return { kind: 'ball', picture: 0, scale: 1, additive: false };
}

/**
 * The ring's al_icon picture number for `cos` = (head's up direction) . (direction to the camera):
 * acos to the game's 65536-step angle, to whole degrees (x 360/65536 + 0.5), modulo 181 into `table`, plus 6
 * (ChaoMain .text 0x13004..0x130B0).
 */
export function haloPicture(table, cos) {
  const bams = Math.trunc(Math.acos(Math.max(-1, Math.min(1, cos))) * 10430.38043493439) & 0xFFFF;
  return table[Math.trunc(bams * 0.0054931640625 + 0.5) % 181] + 6;
}
