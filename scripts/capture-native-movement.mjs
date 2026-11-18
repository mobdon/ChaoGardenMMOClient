// Opt-in visual regression capture using this checkout's real garden renderer.
// No gameplay service, credential, profile, save, input, or reward is accessed.
// Example (locally installed original assets are required):
// node scripts/capture-native-movement.mjs --assets C:/path/to/.local/imports
// Supply --playwright-module C:/path/to/playwright/index.mjs if not installed.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir, access } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createServer } from 'vite';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const options = Object.fromEntries(process.argv.slice(2).map((value, index, all) => value.startsWith('--') ? [value.slice(2), all[index + 1]] : null).filter(Boolean));
const assetDirectory = path.resolve(options.assets || path.join(root, '.local/imports'));
const output = path.resolve(options.output || path.join(root, '.local/screenshots/native-movement'));
const allowed = new Set(['chao', 'neutral-chaos', 'hero-chaos', 'dark-chaos', 'dark-chao-walker', 'dark-walker-booster', 'dark-walker-shot', 'chao-locomotion', 'garden', 'hero-garden', 'dark-garden', 'tree-neutral', 'amy', 'knuckles', 'tails',
  ...['left', 'right'].flatMap(side => [`shadow-air-shoes-${side}`, `knuckles-hammer-gloves-${side}`]),
  ...['red', 'blue', 'green', 'cyan', 'purple', 'yellow', 'white'].map(color => `emerald-${color}`)].map(name => `${name}.glb`));
allowed.add('shadow-skate-motion.json'); allowed.add('chao-flame.png'); allowed.add('hub-grass.png');
const required = ['neutral-chaos.glb', 'hero-chaos.glb', 'dark-chaos.glb', 'dark-chao-walker.glb', 'shadow-skate-motion.json', 'chao-locomotion.glb'];
for (const file of required) await access(path.join(assetDirectory, file));
await mkdir(output, { recursive: true });

const html = `<!doctype html><meta charset="utf-8"><title>Native Chao movement visual fixture</title>
<style>html,body{margin:0;width:100%;height:100%;overflow:hidden;background:#18252b;font:14px system-ui;color:white}canvas{width:100%;height:100%;display:block}#proof{position:absolute;top:12px;left:12px;padding:10px 14px;background:#15242be8;border-radius:8px;max-width:690px;pointer-events:none}#proof b{display:block;color:#ffe3a3}#proof small{display:block;opacity:.8;margin-top:4px}</style>
<canvas id="garden"></canvas><div id="proof"><b id="phase">Loading original local assets</b><span id="state"></span><small>Rendering fixture • synthetic authoritative snapshot sequence • no service/save access</small></div>
<script type="module" src="/qa-native-movement.js"></script>`;

const fixture = `
import { createGarden } from '/src/scene.js';
import { EngineStore, Vector3, Matrix, Ray } from '@babylonjs/core';
const canvas=document.querySelector('canvas');
const garden=createGarden(canvas);
let config={phase:'idle',speed:0,form:'dark-chaos',zone:'hub',mounted:false,air:0,vertical:0,owner:true,remote:false}, previous=performance.now(), elapsed=0, serverTime=100000, x=0,z=6, timer;
let lastPlayers=[];
const make=(id,owner,form,x,z)=>{const ground= config.zone==='hub'?0:terrainAt(x,z)?.y??0;return {id,name:id==='proof'?'Movement proof':'Remote observer',admin:owner,form,alignment:form==='dark-chaos'?-100:form==='hero-chaos'?100:0,x,z,y:ground+config.air,groundY:ground,jumpOffset:config.air,vy:config.vertical,verticalVelocity:config.vertical,facing:0,zone:config.zone,hp:100,gear:{footwear:'shadow-air-shoes',hands:'knuckles-hammer-gloves'},vehicle:config.mounted&&id==='proof'?'dark-chao-walker':null,mech:{grounded:config.air<=0.001,boosting:!!config.boost}}};
function tick(){const now=performance.now(),dt=Math.min(.15,(now-previous)/1000);previous=now;elapsed+=dt;serverTime+=dt*1000;if(config.zone==='hub')z+=config.speed*dt;else x+=config.speed*dt;config.air=Math.max(0,config.air+config.vertical*dt);
const self=make('proof',config.owner,config.form,x,z);lastPlayers=[self];if(config.remote)lastPlayers.push(make('remote',false,'neutral-chaos',x+.9,z+.2));garden.setSnapshot({type:'snapshot',serverTime,selfId:'proof',zone:config.zone,players:lastPlayers,rings:[],enemies:[]});
document.querySelector('#phase').textContent=config.phase;
document.querySelector('#state').textContent=[canvas.dataset.chaoMotion||'loading',canvas.dataset.skateSpeed+' units/s',canvas.dataset.skateRate+' skate cadence',canvas.dataset.skateBlend+' skate blend'].join(' · ');}
tick();timer=setInterval(tick,100);
function scene(){return EngineStore.LastCreatedScene;}
function terrainAt(x,z){const s=scene();if(!s)return null;const hits=s.multiPickWithRay(new Ray(new Vector3(x,1000,z),new Vector3(0,-1,0),2000),m=>m.isEnabled()&&m.metadata?.nativeTerrain)||[];const hit=hits.filter(h=>h.hit&&h.getNormal(true)?.y>.3).sort((a,b)=>a.distance-b.distance)[0];return hit?{y:hit.pickedPoint.y,normal:hit.getNormal(true).asArray(),mesh:hit.pickedMesh.name}:null;}
function model(){return scene().getTransformNodeByName('chao-proof-local-model');}
function meshes(){return model()?.getChildMeshes().filter(m=>m.getTotalVertices()&&m.isEnabled())||[];}
function materialReport(){return meshes().map(m=>({mesh:m.name,material:m.material?.name,alpha:m.material?.alpha,transparencyMode:m.material?.transparencyMode,disableDepthWrite:m.material?.disableDepthWrite}));}
function measure(){const s=scene(),v=model(),m=meshes();for(const skel of new Set(m.map(m=>m.skeleton).filter(Boolean)))skel.prepare(true);let min=Infinity;for(const mesh of m){mesh.refreshBoundingInfo(true,true);mesh.computeWorldMatrix(true);min=Math.min(min,mesh.getBoundingInfo().boundingBox.minimumWorld.y)}
const nodes=v?.getDescendants().filter(n=>/\\d{3}_object_/.test(n.name))||[];
const green=s.transformNodes.find(n=>n.name==='native-chaos-emerald-green'&&n.metadata?.nativeColor);green?.computeWorldMatrix(true);const greenScreen=green?Vector3.Project(green.getAbsolutePosition(),Matrix.Identity(),s.getTransformMatrix(),s.activeCamera.viewport.toGlobal(canvas.width,canvas.height)):null;
const playerRoot=s.getTransformNodeByName('chao-proof'),rp=playerRoot?.position;
return {phase:config.phase,fixture:structuredClone(config),snapshot:structuredClone(lastPlayers),canvas:{...canvas.dataset},nativeNodes:nodes.length,nativeNodeTransforms:nodes.map(n=>({name:n.name,parent:n.parent?.name,scale:n.scaling.asArray(),position:n.position.asArray(),rotation:n.rotationQuaternion?.asArray()})),minimumModelY:min,renderedRootY:rp?.y,renderedTerrain:rp&&terrainAt(rp.x,rp.z),materials:materialReport(),greenScreen:greenScreen?.asArray(),remoteDiagnostics:s.getTransformNodeByName('chao-remote')?.metadata?.shadowSkate?.diagnostics||null,flame:s.getMeshByName('original-dark-chaos-flame')?{alphaMode:s.getMeshByName('original-dark-chaos-flame').material?.alphaMode,transparency:s.getMeshByName('original-dark-chaos-flame').material?.transparencyMode}:null,emeraldMaterials:s.meshes.filter(m=>m.metadata?.nativeEmeraldGlow&&m.isEnabled()).map(m=>({name:m.name,alpha:m.material?.alpha,transparency:m.material?.transparencyMode,depthWrite:!m.material?.disableDepthWrite})),effectLayers:s.effectLayers.map(l=>({name:l.name,includedOnlyCount:l._includedOnlyMeshes?.length||0}))};}
let occlusionObserver=null;
window.qa={set(next){config={...config,...next};if(next.x!==undefined)x=next.x;if(next.z!==undefined)z=next.z;tick();},measure,ready(){return !!model()&&canvas.dataset.nativeRun?.includes('loaded')&&canvas.dataset.ownerGear?.includes('footwear')&&canvas.dataset.emeraldTextures==='7';},findSlope(){for(let z=1;z<=12;z+=.5)for(let x=-6;x<=6;x+=.5){const a=terrainAt(x,z),b=terrainAt(x+.4,z);if(a&&b&&a.normal[1]<.995&&a.normal[1]>.6&&Math.abs(a.y-b.y)>.025&&Math.abs(a.y-b.y)<.25)return {x,z,ground:a,next:b};}return null;},camera(alpha=-Math.PI/2,beta=1.3,radius=2.4){const c=scene().activeCamera;c.alpha=alpha;c.beta=beta;c.radius=radius;c.inertialAlphaOffset=c.inertialBetaOffset=c.inertialRadiusOffset=0;},occlusion(mode){const s=scene();if(occlusionObserver){s.onBeforeRenderObservable.remove(occlusionObserver);occlusionObserver=null;}const gems=s.transformNodes.filter(n=>n.name.startsWith('native-chaos-emerald-')&&n.metadata?.nativeColor);for(const gem of gems)gem.setEnabled(mode==='off'||gem.metadata.nativeColor==='green');if(mode!=='off')occlusionObserver=s.onBeforeRenderObservable.add(()=>{const green=gems.find(n=>n.metadata.nativeColor==='green');if(green)green.position.set(0,.29,mode==='behind'?.20:-.38);});},dispose(){clearInterval(timer);garden.dispose();}};
`;

const requested = new Set(), missing = new Set();
const server = await createServer({
  root, configFile: false, logLevel: 'error',
  plugins: [{ name: 'native-movement-fixture',
    resolveId(id) { if (id === '/qa-native-movement.js') return '\0native-movement-fixture'; },
    load(id) { if (id === '\0native-movement-fixture') return fixture; },
    configureServer(vite) {
    vite.middlewares.use(async (request, response, next) => {
      const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
      if (pathname === '/qa-native-movement') { response.setHeader('Content-Type', 'text/html'); response.end(html); return; }
      if (pathname.startsWith('/api/')) {
        const name = pathname.slice('/api/assets/'.length);
        if (!pathname.startsWith('/api/assets/') || !allowed.has(name)) { response.writeHead(404).end(); return; }
        requested.add(name);
        // Use newly built asset in this worktree before the read-only source directory.
        const local = path.join(root, '.local/imports', name === 'hub-grass.png' ? 'nativehubgrass.png' : name);
        const fallback = path.join(assetDirectory, name === 'hub-grass.png' ? 'nativehubgrass.png' : name);
        try {
          let bytes; try { bytes = await readFile(local); } catch { bytes = await readFile(fallback); }
          response.setHeader('Content-Type', name.endsWith('.json') ? 'application/json' : name.endsWith('.png') ? 'image/png' : 'model/gltf-binary');
          response.setHeader('Cache-Control', 'no-store'); response.end(bytes);
        } catch { missing.add(name); response.writeHead(404).end(); }
        return;
      }
      if (pathname.includes('.local') || pathname.includes('server/') || pathname.includes('scripts/')) { response.writeHead(403).end(); return; }
      next();
    });
  } }],
  server: { host: '127.0.0.1', port: 0, strictPort: true, fs: { strict: true, allow: [root, path.join(root, '../../..', 'node_modules')] } },
});

let browser;
try {
  await server.listen();
  const url = `http://127.0.0.1:${server.httpServer.address().port}/qa-native-movement`;
  const moduleName = options['playwright-module'] ? pathToFileURL(path.resolve(options['playwright-module'])).href : 'playwright';
  const { chromium } = await import(moduleName);
  browser = await chromium.launch({ channel: options.channel || 'chrome', headless: true, args: ['--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') console.error(message.text()); });
  await page.goto(url, { waitUntil: 'networkidle' });
  try { await page.waitForFunction(() => window.qa?.ready(), null, { timeout: 90000 }); }
  catch (error) {
    console.error(JSON.stringify({ errors, missing: [...missing], requested: [...requested], state: await page.evaluate(() => ({ qa: !!window.qa, canvas: { ...document.querySelector('canvas').dataset } })) }));
    await page.screenshot({ path: path.join(output, 'load-failed.png') });
    throw error;
  }
  await page.evaluate(() => window.qa.camera(Math.PI / 3, 1.3, 1.9));
  const phases = [];
  async function capture(name, config, wait = 950) {
    await page.evaluate(config => window.qa.set(config), { phase: name, ...config });
    await page.waitForTimeout(wait);
    const samples = [];
    for (let index = 0; index < 3; index++) {
      samples.push(await page.evaluate(() => window.qa.measure()));
      await page.screenshot({ path: path.join(output, `${name}-${index}.png`) });
      await page.waitForTimeout(70);
    }
    phases.push({ name, samples });
    console.log(`${name}: ${samples[0].canvas.chaoMotion}; speed=${samples[0].canvas.skateSpeed}; blend=${samples[0].canvas.skateBlend}`);
    return samples;
  }
  await capture('01-idle-dark', { speed: 0 });
  await capture('02-walk-dark', { speed: .798 });
  await capture('03-run-dark', { speed: 1.236 });
  await capture('04-skate-entry', { speed: 1.6 });
  await capture('05-skate-fast', { speed: 1.994 });
  await capture('06-skate-hysteresis', { speed: 1.4 });
  await capture('07-run-exit', { speed: 1.236 });
  await capture('08-stop', { speed: 0 });
  await capture('09-jump-rise', { speed: 1.994, air: .35, vertical: .9 }, 180);
  await capture('10-jump-fall', { speed: 1.994, air: 1, vertical: -.65 }, 180);
  await capture('11-land', { speed: 0, air: 0, vertical: 0 }, 40);
  await capture('12-idle-recovery', { speed: 0 }, 700);
  await capture('13-mech-idle', { mounted: true }, 2000);
  await capture('14-mech-move', { speed: 1.994 }, 1100);
  await capture('15-mech-boost', { air: .5, boost: true }, 350);
  await capture('16-mech-land', { air: 0, boost: false, speed: 0 }, 40);
  await capture('17-unmount', { mounted: false }, 1800);
  await capture('18-neutral-run', { form: 'neutral-chaos', speed: 1.236 }, 2000);
  await capture('19-neutral-skate', { speed: 1.994 });
  await capture('20-hero-run', { form: 'hero-chaos', speed: 1.236 }, 2000);
  await capture('21-hero-skate', { speed: 1.994 });
  await capture('22-remote-ownership', { form: 'dark-chaos', remote: true, speed: 1.994 }, 1800);
  await capture('23-return-dark-idle', { remote: false, speed: 0 }, 900);
  await page.evaluate(() => { window.qa.camera(-Math.PI/2,1.3,1.5); window.qa.occlusion('behind'); });
  await capture('24-emerald-occluded', { speed: 0 }, 800);
  await page.evaluate(() => window.qa.occlusion('front'));
  await capture('25-emerald-front', { speed: 0 }, 100);
  await page.evaluate(() => window.qa.occlusion('off'));
  await page.evaluate(() => window.qa.camera(Math.PI / 3, 1.3, 1.9));
  await capture('26-skate-before-jump', { speed: 1.994 }, 700);
  await capture('27-immediate-skate-jump', { air: .2, vertical: .8 }, 100);
  await capture('28-immediate-skate-land', { speed: 0, air: 0, vertical: 0 }, 35);
  await page.evaluate(() => window.qa.set({ phase: 'Loading native garden terrain', zone: 'garden', x: 0, z: 6, speed: 0 }));
  await page.waitForFunction(() => document.querySelector('canvas').dataset.nativeGarden === 'garden', null, { timeout: 20000 });
  const slope = await page.evaluate(() => window.qa.findSlope());
  assert.ok(slope, 'Native garden must contain an actual sloping triangle for contact proof');
  await page.evaluate(slope => window.qa.set({ x: slope.x, z: slope.z, speed: 0 }), slope);
  await page.waitForTimeout(1600);
  await page.evaluate(() => window.qa.camera(0, 1.2, 1.9));
  await capture('29-native-garden-slope-idle', { speed: 0 }, 700);
  await capture('30-native-garden-slope-walk', { speed: .798 }, 700);

  const occlusionPixels = {};
  for (const name of ['24-emerald-occluded', '25-emerald-front']) {
    const png = (await readFile(path.join(output, `${name}-0.png`))).toString('base64');
    const center = phases.find(p => p.name === name).samples[0].greenScreen;
    occlusionPixels[name] = await page.evaluate(async ({ png, center }) => {
      const image = new Image(); image.src = `data:image/png;base64,${png}`; await image.decode();
      const canvas = document.createElement('canvas'); canvas.width = image.width; canvas.height = image.height;
      const context = canvas.getContext('2d'); context.drawImage(image, 0, 0);
      const x = Math.round(center[0] - 22), y = Math.round(center[1] - 22);
      const pixels = context.getImageData(x, y, 44, 44).data;
      let greenPixels = 0;
      for (let i = 0; i < pixels.length; i += 4) if (pixels[i + 1] > 60 && pixels[i + 1] > pixels[i] * 1.4 && pixels[i + 1] > pixels[i + 2] * 1.1) greenPixels++;
      return { x, y, width: 44, height: 44, greenPixels, totalPixels: pixels.length / 4 };
    }, { png, center });
  }

  const checks = {
    walkBelowThreshold: phases.find(p => p.name === '02-walk-dark').samples.every(s => s.canvas.chaoMotion === 'Walk' && s.canvas.skateActive === 'false'),
    nativeRunBelowThreshold: phases.find(p => p.name === '03-run-dark').samples.every(s => s.canvas.chaoMotion === 'NativeRun119' && s.canvas.skateActive === 'false'),
    sourceSkateAtHighSpeed: phases.find(p => p.name === '05-skate-fast').samples.every(s => /^ShadowSkate17[234]$/.test(s.canvas.chaoMotion)),
    cadenceScales: Number(phases.find(p => p.name === '05-skate-fast').samples[0].canvas.skateRate) > Number(phases.find(p => p.name === '04-skate-entry').samples[0].canvas.skateRate),
    hysteresis: phases.find(p => p.name === '06-skate-hysteresis').samples.every(s => s.canvas.skateActive === 'true'),
    exitRestoresNativeRun: phases.find(p => p.name === '07-run-exit').samples.every(s => s.canvas.chaoMotion === 'NativeRun119'),
    noAirborneSkating: phases.filter(p => /jump-rise|jump-fall|mech/.test(p.name)).every(p => p.samples.every(s => s.canvas.skateActive === 'false')),
    nativeJumpSelected: phases.find(p => p.name === '09-jump-rise').samples.every(s => /^NativeJump13[23]$/.test(s.canvas.chaoMotion)),
    nativeFallSelected: phases.find(p => p.name === '10-jump-fall').samples.every(s => /^NativeFall13[56]$/.test(s.canvas.chaoMotion)),
    nativeLandingSelected: phases.find(p => p.name === '11-land').samples.some(s => s.canvas.chaoMotion === 'NativeLand137'),
    immediateSkateJumpHasNoOverlay: phases.find(p => p.name === '27-immediate-skate-jump').samples.every(s => /^NativeJump13[23]$/.test(s.canvas.chaoMotion) && s.canvas.skateBlend === '0.000'),
    stopRestoresIdle: phases.find(p => p.name === '08-stop').samples.every(s => s.canvas.chaoMotion === 'Idle'),
    emeraldTranslucencyPreserved: phases[0].samples[0].emeraldMaterials.some(m => m.alpha < 1 || m.transparency === 2),
    emeraldGlowKeepsOpaqueOccluders: phases[0].samples[0].effectLayers.some(l => l.name === 'original-emerald-glow' && l.includedOnlyCount === 0),
    opaqueNativeBodies: phases.flatMap(p => p.samples).every(s => {
      const bodies = s.materials.filter(m => /^al_[ndh]chaos/.test(m.material));
      return bodies.length > 0 && bodies.every(m => m.alpha === 1 && m.transparencyMode === 0 && m.disableDepthWrite === false);
    }),
    originalFlameStaysTransparent: phases[0].samples[0].flame?.alphaMode === 1 && phases[0].samples[0].flame?.transparency === 2,
    remoteCannotBorrowOwnerSkate: phases.find(p => p.name === '22-remote-ownership').samples.every(s => s.remoteDiagnostics?.active === false),
    nativeGreenEmeraldOccluded: occlusionPixels['25-emerald-front'].greenPixels > 100 && occlusionPixels['24-emerald-occluded'].greenPixels < 10,
    nativeSlopeContact: phases.filter(p => /native-garden-slope/.test(p.name)).every(p => p.samples.every(s => s.renderedTerrain && Math.abs(s.minimumModelY - s.renderedTerrain.y) < .05)),
    pageErrorsAbsent: errors.length === 0,
  };
  const assets = [];
  for (const file of requested) {
    const local = path.join(root, '.local/imports', file === 'hub-grass.png' ? 'nativehubgrass.png' : file);
    let bytes; try { bytes = await readFile(local); } catch { try { bytes = await readFile(path.join(assetDirectory, file === 'hub-grass.png' ? 'nativehubgrass.png' : file)); } catch { continue; } }
    assets.push({ file, sha256: createHash('sha256').update(bytes).digest('hex'), bytes: bytes.length });
  }
  const report = { result: Object.values(checks).every(Boolean) ? 'PASS' : 'FAIL', checks, occlusionPixels, slope, errors, missing: [...missing], assets, phases,
    scope: 'Actual createGarden browser renderer with original local assets and synthetic server-shaped snapshot fixtures. Not a live Rust service round trip, retail mechanics parity, or cross-device acceptance.',
    visualReview: 'Inspect phase PNGs and the front/behind emerald pair. Numeric material checks alone do not certify visual contact or occlusion.' };
  await writeFile(path.join(output, 'proof.json'), JSON.stringify(report, null, 2));
  await writeFile(path.join(output, 'index.html'), `<meta charset="utf-8"><title>Native movement proof</title><style>body{font:16px system-ui;background:#19252b;color:white}img{width:min(100%,640px)}article{display:inline-block;vertical-align:top;max-width:660px;margin:8px}h2{font-size:17px}</style><h1>Native movement rendering fixture</h1><p>${report.scope}</p>${phases.map(p => `<article><h2>${p.name}</h2><img src="${p.name}-0.png"><p>${p.samples[0].canvas.chaoMotion} · ${p.samples[0].canvas.skateSpeed} units/s</p></article>`).join('')}`);
  assert.equal(report.result, 'PASS', `Visual fixture checks failed; inspect ${path.join(output, 'proof.json')}`);
  console.log(`PASS: ${phases.length} phases, ${phases.length * 3} PNG captures in ${output}`);
} finally {
  await browser?.close();
  await server.close();
}
