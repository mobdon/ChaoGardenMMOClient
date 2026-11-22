// Game pictures and Chao models converted in the browser from the player's own game folder (picked once on
// the login page, see game-files.js). Nothing here is fetched from the server and nothing is
// stored: each one is converted in memory when a page first asks for it.

// alias -> [archives to look in, texture name]. Each archive is the Steam install's
// resource/gd_PC/<archive>.prs; names and archives as recorded by scripts/extract-chao-ui.js.
const MENU_ARCHIVES = ['al_tex_common', 'al_tex_chao', 'al_tex_ent_common_en', 'al_tex_odekake_menu_en', 'al_tex_cave_menu_en', 'al_common_tex', 'al_panel', 'al_icon'];
// al_panel order = native item ids 0-24 (21 animals, then the 4 Chaos Drives).
const PANELS = ['pen', 'goma', 'rako', 'usa', 'chit', 'ino', 'kuma', 'tora', 'gori', 'kuja', 'oum', 'kon', 'ska',
  'shep', 'ara', 'han', 'bone', 'batman', 'dra', 'uni', 'phen', 'cdr_y', 'cdr_g', 'cdr_r', 'cdr_p'];
export const TEXTURES = {
  'chao-stat-panel.png': [['al_tex_common'], 'level00'],
  'chao-stat-font.png': [['al_tex_common'], 'ad_22ascii'],
  'chao-race-parameter.png': [['al_tex_ent_common_en'], 'add_parameter_race_en'],
  // The extraction script takes this one from whichever menu archive holds it.
  'chao-select-window.png': [MENU_ARCHIVES, 'add_select_race_en'],
  'chao-icon-heart.png': [['al_icon'], 'al_icon05'],
  'emotion-sphere.png': [['al_icon'], 'al_icon01'],
  'item-panel-fruit.png': [['al_tex_common'], 'panel_fruit'],
  'item-panel-egg.png': [['al_tex_common'], 'panel_egg'],
  ...Object.fromEntries(PANELS.map((name, id) => [`item-panel-${id}.png`, [['al_panel'], `panel_${name}`]])),
};

/** One named texture of a `.prs` archive as PNG bytes, or null. `x` is game-texture.wasm's exports. */
export function convertTexture(x, prs, name) {
  const text = new TextEncoder().encode(name);
  const p = x.buf_alloc(prs.length), n = x.buf_alloc(text.length);
  new Uint8Array(x.memory.buffer, p, prs.length).set(prs);
  new Uint8Array(x.memory.buffer, n, text.length).set(text);
  const ok = x.convert(p, prs.length, n, text.length);
  x.buf_free(p, prs.length); x.buf_free(n, text.length);
  return ok ? new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len()).slice() : null;
}

/** One Chao model with one motion (clip name `motion`) as GLB bytes, or null. `x` is game-texture.wasm's exports. */
export function convertChao(x, dll, body, palettes, model, motion, look = {}, family = false) {
  const inputs = [dll, body, palettes].map(bytes => {
    const p = x.buf_alloc(bytes.length);
    new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
    return [p, bytes.length];
  });
  const ok = x.chao(...inputs.flat(), model, motion, look.type ?? 2, look.hPos ?? 0, look.vPos ?? 0, look.aPos ?? 0, look.growth ?? 0, family ? 1 : 0);
  for (const [p, length] of inputs) x.buf_free(p, length);
  return ok ? new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len()).slice() : null;
}

let wasm, steam;
// A failed read isn't remembered: read access may only arrive with the login click.
const files = new Map(), urls = new Map(), models = new Map();
const converter = () => wasm ??= WebAssembly.instantiateStreaming(fetch('/game-texture.wasm'), {}).then(r => r.instance.exports);
/** A file of the Steam install's resource/gd_PC folder (path below it) as bytes, or null. */
function gameFile(path) {
  // shortcut: reads the Steam folder only; add the disc image when a disc-only player needs it.
  if (!files.has(path)) files.set(path, (async () => {
    steam ??= import('./game-files.js').then(m => m.savedHandles()).then(h => h.steam);
    const parts = ['resource', 'gd_PC', ...path.split('/')], name = parts.pop();
    let dir = await steam;
    for (const part of parts) dir = await dir.getDirectoryHandle(part);
    return new Uint8Array(await (await (await dir.getFileHandle(name)).getFile()).arrayBuffer());
  })().catch(() => null).then(bytes => { if (!bytes) files.delete(path); return bytes; }));
  return files.get(path);
}
const archive = name => gameFile(`${name}.prs`);

// The game's body palette files in its own table order (same list as chao_palette::SLOT_FILES).
const PALETTES = ['nc00', 'nc01', ...['z', 'n', 's', 'f', 'r', 'p'].map(s => `hc${s}`), 'dc',
  ...['hn', 'hs', 'hf', 'hr', 'hp'].flatMap(f => ['z', 'n', 's', 'f', 'r', 'p'].map(s => f + s))];
let palettes;
const bodyPalettes = () => palettes ??= Promise.all(PALETTES.map(n => gameFile(`al_${n}.GVP`))).then(all => {
  if (all.includes(null)) return palettes = null;
  const joined = new Uint8Array(all.reduce((n, f) => n + f.length, 0));
  all.reduce((at, f) => (joined.set(f, at), at + f.length), 0);
  return joined;
});

/**
 * GLB bytes of the game's Chao model `model` (its index in the game's own model table) playing
 * motion number `motion`, built from the player's game folder; null when it can't be read.
 * `look` is the Chao's own { type, hPos, vPos, aPos, growth } (a new child when left out): the
 * game blends the body colours from them. `family` asks for the whole body family that starts at
 * `model` as one model with morph targets (the shapes the Chao grows between).
 */
export function gameChaoGlb(model, motion, look, family = false) {
  const key = `${model}:${motion}:${family}:${look ? [look.type, look.hPos, look.vPos, look.aPos, look.growth] : ''}`;
  if (!models.has(key)) models.set(key, (async () => {
    const [dll, body, colours] = await Promise.all([gameFile('DLL/Win32/Data_DLL.dll'), gameFile('al_body.prs'), bodyPalettes()]);
    return dll && body && colours ? convertChao(await converter(), dll, body, colours, model, motion, look, family) : null;
  })().catch(() => null).then(glb => { if (!glb) models.delete(key); return glb; }));
  return models.get(key);
}

// Runs one converter export over whole input files; the made file's bytes, or null.
function convertWith(x, name, inputs, ...numbers) {
  const held = inputs.map(bytes => {
    const p = x.buf_alloc(bytes.length);
    new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
    return [p, bytes.length];
  });
  const ok = x[name](...held.flat(), ...numbers);
  for (const [p, length] of held) x.buf_free(p, length);
  return ok ? new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len()).slice() : null;
}
const remembered = (key, make) => {
  if (!models.has(key)) models.set(key, make().catch(() => null).then(glb => { if (!glb) models.delete(key); return glb; }));
  return models.get(key);
};

/**
 * GLB bytes of the game's Chao model `model` as a new child with every motion number in `motions`
 * (clips named `m<number>`, each with its motion table row in the file's extras), or null.
 */
export function gameChaoMotionsGlb(model, motions) {
  return remembered(`motions:${model}:${motions}`, async () => {
    const [dll, body, colours] = await Promise.all([gameFile('DLL/Win32/Data_DLL.dll'), gameFile('al_body.prs'), bodyPalettes()]);
    return dll && body && colours
      ? convertWith(await converter(), 'chao_motions', [dll, body, colours, new Uint8Array(new Uint16Array(motions).buffer)], model) : null;
  });
}

/**
 * GLB bytes of the garden egg from the game program: the one-piece egg with its shake motion `Egg`,
 * or with `hatching` the two-shell egg with the hatch motions `Pon` and `Kyoro`; null without the files.
 */
export function gameEggGlb(hatching = false) {
  return remembered(`egg:${hatching}`, async () => {
    steam ??= import('./game-files.js').then(m => m.savedHandles()).then(h => h.steam);
    const file = await (await (await steam).getFileHandle('sonic2app.exe')).getFile();
    // shortcut: only the Steam build of this size is traced, another build gets no egg; add its addresses when one turns up.
    if (file.size !== 21858816) return null;
    const [body, colours] = await Promise.all([gameFile('al_body.prs'), bodyPalettes()]);
    return body && colours
      ? convertWith(await converter(), 'egg', [new Uint8Array(await file.arrayBuffer()), body, colours], hatching ? 1 : 0) : null;
  });
}

/**
 * The game's 13 body colours as [r, g, b, a], index = colour number - 1 (Steam sonic2app.exe .data
 * 0x129844C, file offset 0xE95E4C; the same 13 rows as the GameCube gxColorTable), or null.
 */
export async function gameBodyColours() {
  try {
    steam ??= import('./game-files.js').then(m => m.savedHandles()).then(h => h.steam);
    const file = await (await (await steam).getFileHandle('sonic2app.exe')).getFile();
    // shortcut: only the Steam build of this size is traced, another build gets no colours; add its offset when one turns up.
    if (file.size !== 21858816) return null;
    const bytes = new Uint8Array(await file.slice(0xE95E4C, 0xE95E4C + 52).arrayBuffer());
    return Array.from({ length: 13 }, (_, i) => [...bytes.subarray(i * 4, i * 4 + 4)]);
  } catch { return null; }
}

/** The texture names of game archive `name` (without `.prs`) in its own order, or null. */
export async function gameArchiveNames(name) {
  const prs = await archive(name), x = prs && await converter();
  if (!prs) return null;
  const p = x.buf_alloc(prs.length);
  new Uint8Array(x.memory.buffer, p, prs.length).set(prs);
  const ok = x.names(p, prs.length);
  x.buf_free(p, prs.length);
  return ok ? new TextDecoder().decode(new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len())).split('\n') : null;
}

/**
 * A blob: URL of the game picture `alias` (a TEXTURES key, or `archive/texture`), or null when the
 * game files or the picture can't be read.
 */
export function gameTextureUrl(alias) {
  if (!urls.has(alias)) urls.set(alias, (async () => {
    const [from, name] = TEXTURES[alias] ?? (alias.includes('/') ? [[alias.split('/')[0]], alias.split('/')[1]] : [[], '']);
    for (const archiveName of from) {
      const prs = await archive(archiveName);
      const png = prs && convertTexture(await converter(), prs, name);
      if (png) return URL.createObjectURL(new Blob([png], { type: 'image/png' }));
    }
    return null;
  })().catch(() => null).then(url => { if (!url) urls.delete(alias); return url; }));
  return urls.get(alias);
}

/** Points the style sheets at the converted stat panel and font (they read these two variables). */
export async function applyGameTextureStyles(root = document.documentElement) {
  for (const [variable, alias] of [['--stat-panel-url', 'chao-stat-panel.png'], ['--stat-font-url', 'chao-stat-font.png']]) {
    const url = await gameTextureUrl(alias);
    if (url) root.style.setProperty(variable, `url('${url}')`);
  }
}
