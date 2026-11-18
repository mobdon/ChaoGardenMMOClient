// Isolated visual check. Uses original local assets, no network or saved profile.
import { readFile, mkdir, writeFile, stat, mkdtemp } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const [backgroundFile, importsDirectory, chrome] = process.argv.slice(2);
if (!backgroundFile || !importsDirectory || !chrome) throw new Error('Pass exported background PNG, original imports directory and Chrome executable');
const output = path.join(root, '.local', 'native-scene-check');
await mkdir(output, { recursive: true });
const uri = async file => `data:image/png;base64,${(await readFile(file)).toString('base64')}`;
const atlases = Object.fromEntries(await Promise.all(['neutral', 'hero', 'dark'].map(async variant => [variant, await uri(path.join(importsDirectory, `tiny-${variant}-chaos.png`))])));
const [background, flame, adapter, portrait] = await Promise.all([
  uri(backgroundFile), uri(path.join(importsDirectory, 'chao-flame.png')),
  readFile(path.join(root, 'src', 'tiny-native-garden.js'), 'utf8'),
  readFile(path.join(root, 'src', 'tiny-avatar.js'), 'utf8'),
]);
const avatarCode = portrait.replaceAll('export function ', 'function ')
  .replace('image.src = `/api/assets/tiny-${variant}-chaos.png`;', 'image.src = sourceAtlases[variant];')
  .replace("flame.src = '/api/assets/chao-flame.png';", `flame.src = ${JSON.stringify(flame)};`);
const html = `<!doctype html><meta charset="utf-8"><title>Original Tiny Chao Garden art check</title>
<style>body{margin:0;padding:26px;background:#faf8e9;color:#173d31;font:16px system-ui}h1{margin:0 0 8px;font-size:26px}p{margin:0 0 22px}section{display:flex;gap:24px;align-items:start}canvas{display:block;border:2px solid #b1c990;background:#f8f1cc;max-width:100%;image-rendering:pixelated}h2{font-size:14px}small{display:block;margin-top:22px;color:#526156}</style>
<h1>Original Tiny Chao Garden</h1><p>Native Advance 2 garden and panel · original Dark Chaos art</p>
<section><div><h2>Full garden</h2><canvas id="large" width="480" height="320"></canvas></div><div><h2>Wide garden view</h2><canvas id="wide" width="342" height="160"></canvas><h2>Compact garden view</h2><canvas id="compact" width="171" height="160"></canvas></div></section>
<small>Artwork verification only. No saved Chao connected; unknown values remain —. No care or progression was simulated.</small>
<script type="module">
window.addEventListener('unhandledrejection',event=>{document.body.dataset.error=event.reason?.message||'Rendering failed';});
const sourceAtlases=${JSON.stringify(atlases)};
${avatarCode}
${adapter.replaceAll('export function ', 'function ')}
const background=new Image(); background.src=${JSON.stringify(background)};
await Promise.all([background.decode(),flame.decode(),...Array.from(atlases.values(), image=>image.decode())]);
// Only appearance is specified for this artwork check. No fake needs/stats/rings.
const profile=Object.freeze({form:'chaos',alignment:-100});
for (const id of ['large','wide','compact']) {
 const context=document.getElementById(id).getContext('2d');
 if (!drawNativeGarden(context,{background,profile,drawAvatar:drawTinyAvatar,panel:id==='large'})) throw Error('Original garden failed to draw');
}
document.body.dataset.verified='true';
</script>`;
const page = path.join(output, 'render.html');
const screenshot = path.join(output, 'tiny-native-garden.png');
await writeFile(page, html);
const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
const browserProfile = await mkdtemp(path.join(output, 'browser-'));
const child = spawn(chrome, ['--headless', '--disable-gpu', '--force-device-scale-factor=1', '--window-size=960,600', '--disable-background-networking', '--hide-scrollbars', '--no-first-run',
  '--no-default-browser-check', `--user-data-dir=${browserProfile}`, '--remote-debugging-port=0', 'about:blank'],
  { windowsHide: true, stdio: 'ignore' });
let socket;
try {
  const deadline = Date.now() + 20000;
  let port;
  while (!port && Date.now() < deadline) {
    try { port = Number((await readFile(path.join(browserProfile, 'DevToolsActivePort'), 'utf8')).split('\n')[0]); }
    catch { await pause(100); }
  }
  if (!port) throw new Error('Isolated browser did not start');
  const tab = await (await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent(pathToFileURL(page).href)}`, { method: 'PUT' })).json();
  socket = new WebSocket(tab.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = () => reject(new Error('Browser connection failed')); });
  let sequence = 0;
  const pending = new Map();
  socket.onmessage = event => {
    const result = JSON.parse(event.data), request = pending.get(result.id);
    if (!request) return;
    pending.delete(result.id); clearTimeout(request.timer);
    if (result.error) request.reject(new Error(result.error.message)); else request.resolve(result.result);
  };
  const command = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Browser timed out: ${method}`)); }, 5000);
    pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
  });
  await command('Emulation.setDeviceMetricsOverride', { width: 960, height: 600, deviceScaleFactor: 1, mobile: false });
  let verified = false;
  while (!verified && Date.now() < deadline) {
    const result = await command('Runtime.evaluate', { expression: 'JSON.stringify({ready:document.body?.dataset.verified,error:document.body?.dataset.error})', returnByValue: true });
    if (result.result.value) {
      const state = JSON.parse(result.result.value);
      if (state.error) throw new Error(state.error);
      verified = state.ready === 'true';
    }
    if (!verified) await pause(100);
  }
  if (!verified) throw new Error('Original background/Chao canvas did not finish');
  await command('Runtime.evaluate', { expression: 'new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))', awaitPromise: true });
  const capture = await command('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  await writeFile(screenshot, Buffer.from(capture.data, 'base64'));
  await command('Browser.close');
} finally {
  socket?.close();
  if (child.exitCode === null) child.kill();
}
if ((await stat(screenshot)).size < 1000) throw new Error('No visual check screenshot');
console.log(screenshot);
