import { build, loadConfigFromFile } from 'vite';
import { execFileSync } from 'node:child_process';
import { readFile, writeFile, mkdir, readdir, rename } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, relative, extname } from 'node:path';

const source = process.env.TINY_PHONE_SOURCE;
if (!source || !process.env.TINY_GARDEN_ART_DIR) throw Error('Set TINY_PHONE_SOURCE and TINY_GARDEN_ART_DIR explicitly.');
const root = process.cwd();
const directory = resolve(root, process.env.TINY_PHONE_ISOLATED_BUILD === '1' ? '.local/native-phone-games' : '.local/native-phone');
const stamp = `snapshot-${Date.now()}`;
const output = resolve(directory, stamp);
const sourceCommit = execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const status = execFileSync('git', ['-C', source, 'status', '--porcelain'], { encoding: 'utf8' }).trim();
if (status) throw Error('Native Tiny source must be clean for a reproducible preview.');
const embed = await readFile(resolve(root, 'src/phone-embed.js'), 'utf8');
const css = await readFile(resolve(root, 'src/phone-embed.css'), 'utf8');
const statUi = await readFile(resolve(root, 'src/stat-ui.js'), 'utf8');
await mkdir(directory, { recursive: true });
const { config } = await loadConfigFromFile({ command: 'build', mode: 'production' }, resolve(source, 'android/preview/vite.config.mjs'), source);
await build({ ...config, root: source, configFile: false,
  base: '/phone-tiny/', publicDir: false, logLevel: 'warn',
  plugins: [...config.plugins, { name: 'phone-preview-adapter', enforce: 'pre',
    resolveId(id, importer) {
      if (id === './stat-labels.css' && importer?.replaceAll('\\', '/').endsWith('/src/stat-ui.js')) return resolve(root, 'src/stat-labels.css');
      if (id === './tiny-flame.js') return resolve(root, 'src/tiny-flame.js');
      if (id === './tiny-widget-motion.js') return resolve(root, 'src/tiny-widget-motion.js');
      if (id === './tiny-shop.js' || id === './tiny-shop.css') return resolve(root, 'src', id.slice(2));
      if (id === './tiny-resident.js') return resolve(root, 'src/tiny-resident.js');
      if (id === './tiny-diary.js') return resolve(root, 'src/tiny-diary.js');
      if (['./tiny-stock-shelf.js','./tiny-toys.js','./tiny-ordinary-chao.js','./tiny-weeds.js','./tiny-native-games.js','./tiny-native-games.css'].includes(id)) return resolve(root,'src',id.slice(2));
      if (id === './tiny-eggs.js') return resolve(root, 'src/tiny-eggs.js');
      if (id === './tiny-training.js') return resolve(root, 'src/tiny-afk-training.js');
      if (id === './tiny-widget-layout.js' || id === './tiny-widget-layout.css') return resolve(root, 'src', id.slice(2));
      if (id === './tiny-native-sidebar.js' || id === './tiny-native-sidebar.css' || id === './tiny-garden-layout.js') return resolve(root, 'src', id.slice(2));
      if (id === './tiny-sidebar-toggle.js') return resolve(root, 'src/tiny-sidebar-toggle.js');
      if (id === './stat-ui.js' && importer?.replaceAll('\\', '/').endsWith('/src/tiny-widget-layout.js')) return resolve(root, 'src/stat-ui.js');
    },
    load(id) { if (id.replaceAll('\\', '/').endsWith('/src/stat-ui.js')) return statUi; },
    transform(code, id) {
    const filename = id.replaceAll('\\', '/');
    if (!filename.includes('/src/')) return;
    if (filename.endsWith('/src/tiny-watch.js') || filename.endsWith('/src/widget.js')) {
      code=code.replace("drawWidgetEmeraldLayer(context, emeraldImages, ornaments, 'chest');","if (chaosVariant(profile) !== 'dark') drawWidgetEmeraldLayer(context, emeraldImages, ornaments, 'chest');");
    }
    if (filename.endsWith('/src/tiny-watch.js')) {
      code=code.replace('backgroundContext.drawImage(sidebar, 171, 0)','backgroundContext.drawImage(sidebar, 176, 0)');
      const anchor = "  const canvas = document.querySelector('#tiny-watch-canvas');";
      if (!code.includes(anchor)) throw Error('Review shared Garden sidebar hook.');
      code = "import {drawGardenWeeds} from './tiny-weeds.js';\nimport {toyScene,drawPlacedToys} from './tiny-toys.js';\nimport { createTinyGardenLayout } from './tiny-garden-layout.js';\n" + code;
      code=code.replace('      if (profile?.id && origin', '      drawGardenWeeds(context);drawPlacedToys(context);\n      if (profile?.id && origin')
        .replace('const x = Math.max(20,', 'const play = reconciled ? toyScene()?.play : null;\n        const x = play?.x ?? Math.max(20,')
        .replace('const y = Math.max(64,','const y = play?.y ?? Math.max(64,')
        .replace('time: artTime, direction: owner', 'time: artTime, gardenPose: play?.pose, direction: owner')
        .replace('context.drawImage(bodyLayer, 0, 0, 240, 160);','context.drawImage(bodyLayer, 0, 0, 240, 160);drawPlacedToys(context,true);');
      code = code.replace(anchor, anchor + '\n  const gardenLayout = createTinyGardenLayout(canvas);')
        .replace('      profile = next; connected = connection.connected;', '      gardenLayout.update(next, nextReconciled);\n      profile = next; connected = connection.connected;');
    }
    if (filename.endsWith('/src/tiny-avatar.js')) {
      const avatarAnchor='  const image = atlases.get(variant);';
      if(!code.includes(avatarAnchor))throw Error('Review normal sprite selection.');
      code="import {drawGardenChao} from './tiny-ordinary-chao.js';\n"+code.replace(avatarAnchor,"  if (drawGardenChao(context,profile,x,y,size,{...options,time:reducedMotion.matches?0:options.time},variant,flame)) return true;\n"+avatarAnchor);
      const condition = "if (variant === 'dark' && flame.complete && flame.naturalWidth === 512 && flame.naturalHeight === 32) {";
      if (!code.includes(condition)) throw Error('Review native flame draw before rebuilding.');
      code = "import { getTinyFlameLayer } from './tiny-flame.js';\n" + code;
      code = code.replace(condition, "const flameLayer = variant === 'dark' ? getTinyFlameLayer(flame) : null;\n  if (flameLayer) {")
        .replace('context.drawImage(flame, frame * 32', 'context.drawImage(flameLayer, frame * 32');
    }
    if (filename.endsWith('/src/widget.js')) {
      const placement = 'const placement = widgetCanvasPlacement(gardenWindow.clientWidth, gardenWindow.clientHeight,\n    chaosVariant(art.subject));';
      if (!code.replaceAll('\r\n', '\n').includes(placement)) throw Error('Review widget motion attachment before rebuilding.');
      code = "import {observeCarePose} from './tiny-ordinary-chao.js';\nimport { createTinyWidgetMotion } from './tiny-widget-motion.js';\nimport { createTinyWidgetLayout } from './tiny-widget-layout.js';\nconst widgetMotion = createTinyWidgetMotion();\nlet widgetCare = null;\n" + code.replaceAll('\r\n', '\n');
      code = code.replace("const gardenWindow = $('.widget-garden-window');", "const gardenWindow = $('.widget-garden-window');\nconst widgetLayout = createTinyWidgetLayout();")
        .replace('function render() {', "function render() {\n  widgetLayout.update(profile, connectionState === 'live', widgetCare); observeCarePose(profile,widgetCare,connectionState==='live');")
        .replace('if (data.version === 2) {', "if (data.version === 1 && message.type === 'snapshot' && message.selfId === profile?.id) widgetCare = message.care || null;\n        if (data.version === 2) {");
      code = code.replace(placement, 'const placement = widgetMotion.placement(widgetCanvasPlacement(gardenWindow.clientWidth, gardenWindow.clientHeight,\n    chaosVariant(art.subject)), gardenWindow.clientWidth, gardenWindow.clientHeight);')
        .replace("`Open ${profile.name || 'your Chao'} in Tiny Garden`", "`${profile.name || 'Your Chao'} in Tiny Garden`")
        .replace('previous = { id: next.id,', 'widgetMotion.observe(next);\n  previous = { id: next.id,')
        .replace('new WebSocket(widgetSocketUrl(location, data.version, token))', "new WebSocket(widgetSocketUrl(location, data.version, token) + (data.version === 1 && data.observerSockets ? '&observe=1' : ''))")
        .replace('function draw(time = 0) {', "function draw(time = 0) {\n  positionAvatar();\n  canvas.dataset.motion = connectionState === 'live' ? (profile?.watch?.activity || 'idle') : 'paused';");
    }
    if (filename.endsWith('/src/widget-observer.js')) {
      code = code.replace('return { version, token: session.token, player: session.player, identity: null }', 'return { version, token: session.token, player: session.player, identity: null, observerSockets: status.data.tinyGarden?.observerSockets === true }');
    }
    if (filename.endsWith('/src/tiny-items.js')) {
      code=code.replace("row.append(element('strong', `${item.label}","row.dataset.itemId=item.id; row.append(element('strong', `${item.label}");
      code = code.replace("id === 'garden-fruit' || nativeId(id)", "id === 'garden-fruit' || nativeId(id) || /^tiny-fruit:[0-6]$/.test(id)")
        .replace('const source = native.get(id);', "const tinyFruit = care.tinyShop?.items?.find(item => item.itemId === id && item.source === 'native-a2ne');\n      const source = native.get(id) || tinyFruit;")
        .replace("const available = id === 'garden-fruit' ||", "const available = !!tinyFruit || id === 'garden-fruit' ||")
        .replace("detail: id === 'garden-fruit' ?", "detail: tinyFruit ? 'Tiny Chao Garden fruit' : id === 'garden-fruit' ?");
    }
    code = code.replaceAll('Tiny Garden', 'Tiny Chao Garden').replaceAll('TINY GARDEN', 'TINY CHAO GARDEN');
    code = code.replaceAll('/__widget-art/', '/phone-tiny/__widget-art/')
      .replaceAll('/api/assets/tiny-', '/phone-tiny/api/assets/tiny-')
      .replaceAll('/api/assets/chao-flame.png', '/phone-tiny/api/assets/chao-flame.png')
      .replaceAll('/tiny.html', '/phone-tiny/tiny.html');
    if (filename.endsWith('/src/tiny.js')) {
      code = code.replaceAll('\r\n', '\n');
      const itemsInit = "const items = createTinyItems({ root: $('#tiny-items-panel'), send });";
      const renderAnchor = '  renderConnection();\n  const active = mode()';
      const eventAnchor = 'if (items.handleEvent(message) && message.accepted === true)';
      if (![itemsInit, renderAnchor, eventAnchor].every(anchor => code.includes(anchor))) throw Error('Review Tiny shop adapter hooks before rebuilding.');
      code = "import { createTinyShop } from './tiny-shop.js';\nimport { createTinyEggs } from './tiny-eggs.js';\nimport { createTinyDiary } from './tiny-diary.js';\nimport { createTinyResident } from './tiny-resident.js';\nimport './tiny-shop.css';\n" + code;
      code = code.replace(itemsInit, "const inventoryRoot = document.createElement('div');\nconst shopRoot = document.createElement('section'); shopRoot.className = 'tiny-shop'; shopRoot.setAttribute('aria-label', 'Fruit shop');\n$('#tiny-items-panel').append(shopRoot, inventoryRoot);\nconst items = createTinyItems({ root: inventoryRoot, send });\nconst shop = createTinyShop({ root: shopRoot, send });\nconst resident = createTinyResident({ send });")
        .replace(renderAnchor, "  diary.update(player, { connected, care: careState, game: tinyGame });\n  eggs.update(player, { connected: mayAct(), care: careState, accountsV2: accountV2 });\n  shop.update(player, { connected: mayAct(), care: careState, accountsV2: accountV2, serverTime });\n  resident.update(player, { connected: mayAct(), care: careState, capabilities, accountsV2: accountV2 });\n  document.documentElement.dataset.autoTiny = String(capabilities.autoResident === true);\n" + renderAnchor)
        .replace(eventAnchor, 'if ((items.handleEvent(message) || eggs.handleEvent(message) || shop.handleEvent(message) || resident.handleEvent(message)) && message.accepted === true)');
      code = code.replace('const resident = createTinyResident({ send });', "const resident = createTinyResident({ send });\nconst diary = createTinyDiary({ root: $('#tiny-garden-panel') });");
      code = code.replace('const shop = createTinyShop({ root: shopRoot, send });', "const shop = createTinyShop({ root: shopRoot, send });\nconst eggRoot = document.createElement('section'); eggRoot.className = 'tiny-shop tiny-egg-shop'; shopRoot.after(eggRoot);\nconst eggs = createTinyEggs({ root: eggRoot, send });");
      code = code.replace('training.update(player, { connected: mayAct(),', 'training.update(player, { care: careState, connected: mayAct(),');
      code = code.replace('  diary.update(player,', "  if (careState?.training?.job && ['training', 'ready'].includes(careState.training.job.phase)) $('#tiny-watch-status').textContent = 'Training';\n  diary.update(player,");
      code = code.replace('const eggs = createTinyEggs({ root: eggRoot, send });', 'const eggs = createTinyEggs({ root: eggRoot, send });\nconst shelf = createStockShelf(shopRoot, eggRoot, inventoryRoot);');
      code = code.replace('  resident.update(player,', '  shelf.update(careState, player); observeCarePose(player,careState,connected);\n  resident.update(player,');
      const hooks = [
        ['const resident = createTinyResident({ send });', "const toys = createTinyToys({root:shopRoot,send});\nconst weeds = createTinyWeeds({ send });\nconst nativeGames = createNativeTinyGames({root: $('#tiny-stage'),send});"],
        ['  diary.update(player,', "  toys.update(careState,player,mayAct() && !accountV2);\n  weeds.update(careState, mayAct() && !accountV2,player?.id);"],
      ];
      code = "import {observeCarePose} from './tiny-ordinary-chao.js';\nimport {createStockShelf} from './tiny-stock-shelf.js';\nimport {createTinyToys} from './tiny-toys.js';\nimport {createTinyWeeds} from './tiny-weeds.js';\nimport {createNativeTinyGames,hasOriginalGame} from './tiny-native-games.js';\n" + code;
      for(const [anchor,insert] of hooks){if(!code.includes(anchor))throw Error('Review Tiny adapter hook: '+anchor);code=code.replace(anchor,insert+'\n'+anchor);}
      code = code.replace('items.handleEvent(message) ||', 'toys.handleEvent(message) || weeds.handleEvent(message) || items.handleEvent(message) ||')
        .replace('  if (!state || !hasGame)', "  const native = nativeGames.update(tinyGame,{focused:selectedScreen === 'games',connected:mayAct()});\n  $('#tiny-stage').hidden=native; $('#tiny-control-tip').hidden=native; if(native){$('#tiny-game-controls').hidden=true; metricItems(nativeGames.metrics); return;}\n  if (!state || !hasGame)")
        .replace('function intent(input) {', 'function intent(input) { if(nativeGames.active) return;')
        .replace("if (selectedScreen === 'games' && state && tinyGame)", "if (!nativeGames.active && selectedScreen === 'games' && state && tinyGame)");
      const startHook = "$('#tiny-start').addEventListener('click', async () => {";
      const controlsHook = '  const native = nativeGames.update(';
      if (![startHook, controlsHook].every(anchor => code.includes(anchor))) throw Error('Review original minigame availability hooks.');
      code = code.replace(startHook, startHook + "\n  if (!hasOriginalGame(selectedGame, capabilities)) return;")
        .replace(controlsHook, `  if (!hasGame && !hasOriginalGame(active, capabilities)) {
    $('#tiny-start').disabled = true;
    $('#tiny-start').textContent = 'Original game unavailable';
    $('#tiny-cover-copy').textContent = 'The original game data is unavailable.';
    $('#tiny-phase').textContent = 'Unavailable';
  }
` + controlsHook);
      const gate = "if (import.meta.env.PROD && 'serviceWorker' in navigator)";
      if (!code.includes(gate)) throw Error('Review Tiny service-worker registration before rebuilding.');
    // This is an embedded browser simulation; do not install or replace the
    // standalone app's root-scoped PWA cache while reviewing its presentation.
      code = code.replace(gate, 'if (false)');
      code = code.replace("location.assign('/');", "if (window.parent !== window) window.parent.postMessage({type:'tiny-phone-desktop'}, location.origin); else location.assign('/');");
    }
    return code;
  } }],
  build: { ...config.build, outDir: output, emptyOutDir: false, rolldownOptions: {
    input: { tiny: resolve(source, 'tiny.html'), widget: resolve(source, 'widget.html') },
  } },
});
await writeFile(resolve(output, 'phone-embed.js'), embed);
await writeFile(resolve(output, 'phone-embed.css'), css);
const shopArtCatalog = JSON.parse(await readFile(resolve(root, 'asset-tools/crates/tiny-shop-presentation/catalog/source.json'), 'utf8'));
if (!process.env.TINY_PHONE_FONT_ART_DIR) throw Error('Set TINY_PHONE_FONT_ART_DIR to the verified native Tiny label export.');
await mkdir(resolve(output, '__tiny-shop'), { recursive: true });
for (const font of shopArtCatalog.fontOutputs) {
  if (!/^tiny-native-(?:label-(?:luck|intelligence)|sidebar-ascii)-a2ne\.png$/.test(font.filename)) throw Error('Unknown Tiny sidebar font asset');
  const bytes = await readFile(resolve(process.env.TINY_PHONE_FONT_ART_DIR, font.filename));
  if (bytes.length > 16384 || createHash('sha256').update(bytes).digest('hex') !== font.sha256) throw Error('Tiny sidebar font mismatch');
  await writeFile(resolve(output, '__tiny-shop', font.filename), bytes);
}
{
  const fruit=await readFile(resolve(root,'.local/extracted/garden-fruit/garden-fruit.png'));
  if(createHash('sha256').update(fruit).digest('hex')!=='ea40e93e9230d01c04c9fb4695e0ec5e116a8fbdccdeedb9da7411f1f04645b8')throw Error('Unexpected SA2 Neutral Garden Fruit icon');
  await writeFile(resolve(output,'__tiny-shop/sa2-garden-fruit.png'),fruit);
}
if (process.env.TINY_PHONE_SHOP_ART_DIR) {
  await mkdir(resolve(output, '__tiny-shop'), { recursive: true });
  for (const subject of shopArtCatalog.subjects) {
    if (!/^tiny-native-(?:fruit-[0-6]|egg-(?:[0-9]|1[01])|toy-[0-2])-a2ne\.png$/.test(subject.filename)) throw Error('Unknown Tiny shop subject');
    const bytes = await readFile(resolve(process.env.TINY_PHONE_SHOP_ART_DIR, subject.filename));
    if (bytes.length > 16384 || createHash('sha256').update(bytes).digest('hex') !== subject.sha256) throw Error('Tiny shop source art mismatch');
    await writeFile(resolve(output, '__tiny-shop', subject.filename), bytes);
  }
}
if(process.env.TINY_PHONE_GAME_ART_DIR){
  const bytes=await readFile(resolve(root,'asset-tools/crates/tiny-game-presentation/catalog/native-games.json'));
  const catalog=JSON.parse(bytes);
  await mkdir(resolve(output,'__tiny-games'),{recursive:true});
  for(const entry of catalog.assets){
    if(!/^[a-z0-9-]+\.png$/.test(entry.filename))throw Error('Invalid game sprite name');
    const png=await readFile(resolve(process.env.TINY_PHONE_GAME_ART_DIR,entry.filename));
    if(createHash('sha256').update(png).digest('hex')!==entry.sha256)throw Error('Game sprite mismatch: '+entry.filename);
    await writeFile(resolve(output,'__tiny-games',entry.filename),png);
  }
  await writeFile(resolve(output,'__tiny-games/catalog.json'),bytes);
}
for(const [env,catalogName] of [['TINY_PHONE_ACTOR_ART_DIR','garden-actors'],['TINY_PHONE_RIG_ART_DIR','garden-rig-poses']]){
  if(!process.env[env])continue;
  const {assets}=JSON.parse(await readFile(resolve(root,'asset-tools/crates/tiny-game-presentation/catalog',catalogName+'.json'),'utf8'));
  await mkdir(resolve(output,'__tiny-garden'),{recursive:true});
  for(const a of assets){
    if(!/^garden-[a-z0-9-]+\.png$/.test(a.filename))throw Error('Bad garden file name');
    const bytes=await readFile(resolve(process.env[env],a.filename));
    if(createHash('sha256').update(bytes).digest('hex')!==a.sha256)throw Error('Garden art mismatch: '+a.filename);
    await writeFile(resolve(output,'__tiny-garden',a.filename),bytes);
  }
}
if(process.env.TINY_PHONE_WEED_ART_DIR){
  const png=await readFile(resolve(process.env.TINY_PHONE_WEED_ART_DIR,'tiny-weed-a2ne.png'));
  if(createHash('sha256').update(png).digest('hex')!=='e105b8be32d7793b44237602b2aaab9adb319b37c6054ab2ee3ca64b4431ec42')throw Error('Garden weed sprite mismatch');
  await mkdir(resolve(output,'__tiny-garden'),{recursive:true});
  await writeFile(resolve(output,'__tiny-garden/tiny-weed-a2ne.png'),png);
}
for (const page of ['tiny.html', 'widget.html']) {
  const file = resolve(output, page);
  const html = await readFile(file, 'utf8');
  await writeFile(file, html.replaceAll('Tiny Garden', 'Tiny Chao Garden').replaceAll('TINY GARDEN', 'TINY CHAO GARDEN')
    .replace(/<link rel="manifest"[^>]*>/, '')
    .replaceAll('href="/tiny.html"', 'href="/phone-tiny/tiny.html"')
    .replaceAll('src="/__widget-art/', 'src="/phone-tiny/__widget-art/')
    .replaceAll('href="/?tiny=1"', 'href="/?tiny=1" target="_top"')
    .replaceAll('href="/"', 'href="/" target="_top"')
    .replace('</body>', '<script type="module" src="/phone-tiny/phone-embed.js"></script></body>'));
}
// Missing original art must fail the build instead of publishing a hollow UI.
for (const filename of ['tiny-garden-a2ne.png', 'tiny-garden-a2ne-sidebar-1.png',
  ...['cyan', 'blue', 'yellow', 'red', 'green', 'purple', 'white'].map(color => `widget-emerald-${color}.png`)]) {
  await readFile(resolve(output, '__widget-art', filename));
}
for (const filename of ['chao-flame.png', ...['neutral', 'hero', 'dark'].map(form => `tiny-${form}-chaos.png`)]) {
  await readFile(resolve(output, 'api/assets', filename));
}
const routes = {};
const mime = { '.json':'application/json', '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.png': 'image/png' };
async function catalog(dir) {
  for (const item of await readdir(dir, { withFileTypes: true })) {
    const file = resolve(dir, item.name);
    if (item.isDirectory()) { await catalog(file); continue; }
    const name = relative(output, file).replaceAll('\\', '/');
    if (!mime[extname(name)]) continue;
    const bytes = await readFile(file);
    const entry = { file: `${stamp}/${name}`, sha256: createHash('sha256').update(bytes).digest('hex'), mime: mime[extname(name)] };
    routes[`/phone-tiny/${name}`] = entry;
  }
}
await catalog(output);
const manifest = { sourceCommit, source, builtAt: new Date().toISOString(),
  statUiSha256: createHash('sha256').update(statUi).digest('hex'), routes };
const pending = resolve(directory, `manifest-${stamp}.tmp`);
await writeFile(pending, JSON.stringify(manifest, null, 2));
await rename(pending, resolve(directory, 'manifest.json'));
console.log(JSON.stringify({ sourceCommit, routes: Object.keys(routes).length, output }));
