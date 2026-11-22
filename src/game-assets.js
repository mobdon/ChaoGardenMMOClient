// Game pictures converted in the browser from the player's own game folder (picked once on
// the login page, see game-files.js). Nothing here is fetched from the server and nothing is
// stored: each picture is converted in memory when a page first asks for it.

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

let wasm, steam;
// A failed read isn't remembered: read access may only arrive with the login click.
const archives = new Map(), urls = new Map();
const converter = () => wasm ??= WebAssembly.instantiateStreaming(fetch('/game-texture.wasm'), {}).then(r => r.instance.exports);
function archive(name) {
  // shortcut: reads the Steam folder only; add the disc image when a disc-only player needs it.
  if (!archives.has(name)) archives.set(name, (async () => {
    steam ??= import('./game-files.js').then(m => m.savedHandles()).then(h => h.steam);
    const dir = await (await (await steam).getDirectoryHandle('resource')).getDirectoryHandle('gd_PC');
    return new Uint8Array(await (await (await dir.getFileHandle(`${name}.prs`)).getFile()).arrayBuffer());
  })().catch(() => null).then(bytes => { if (!bytes) archives.delete(name); return bytes; }));
  return archives.get(name);
}

/** A blob: URL of the game picture `alias`, or null when the game files or the picture can't be read. */
export function gameTextureUrl(alias) {
  if (!urls.has(alias)) urls.set(alias, (async () => {
    const [from, name] = TEXTURES[alias] ?? [[], ''];
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
