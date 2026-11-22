// The game's breeding scene, small, inside the breeding menu. Timing is the
// game's own (ChaoMain GSNE8P fn_60_40670 inviter, fn_60_40130 answerer, read
// from the disassembly; see BREEDING.md section 4). It starts when the two
// Chao touch, since in the menu nobody walks over.
import { advanceFrame } from './chao-behaviour.js';

export const STATE = { STAND: 48, NADE: 252, HASYAGU: 468, BANZAI: 492 };
const NADE_SPEED = 1.5, CHEER_AT = 302; // answerer: command 1 on its 302nd nade frame
const HEART = { grow: 0.2, max: 2.8, shrink: 0.25 };

function table(row) { const t = row?.tableProperties || {}; return { start: t.StartFrame ?? 0, end: t.EndFrame ?? 0, speed: t.PlaySpeed ?? 0, loop: t.LoopProperty ?? 0, next: t.NextAnimation ?? -1 }; }
// Same table modes as chao-behaviour.js: 0 loops, 7 swings back and forth, the rest play once.
const ended = (row, frame) => {
  const t = table(row);
  return t.loop !== 0 && t.loop !== 7 && (t.speed < 0 ? frame <= Math.min(t.start, t.end) : frame >= Math.max(t.start, t.end));
};

/**
 * One native frame at a time: what each Chao plays. `rows` maps native state
 * to its motion table row; `rand` picks the banzai repeats (inviter 3-6,
 * answerer 4-7, as the code does).
 */
export function createCourtship(rows, rand = Math.random) {
  const chao = (state, repeats) => ({ state, frame: table(rows.get(state)).start, speed: 1, repeats, done: false });
  const inviter = chao(STATE.STAND, 3 + Math.floor(rand() * 4));
  const answerer = { ...chao(STATE.NADE, 4 + Math.floor(rand() * 4)), speed: NADE_SPEED };
  let tick = 0, cheer = false, cheerFrames = 0;
  const events = ['jingle'];
  const play = (c, state, speed = 1) => Object.assign(c, { state, speed, frame: table(rows.get(state)).start });
  function advance(c) {
    const row = rows.get(c.state);
    if (!row || c.done) return;
    const t = table(row);
    c.frame = advanceFrame(row, c.frame, t.speed * c.speed, 1);
    if (!ended(row, c.frame)) return;
    // The cheer ends when its motion ends (fn_60_40130 phase 5), then banzai.
    if (c.state === STATE.HASYAGU) play(c, STATE.BANZAI);
    else if (t.next >= 0 && rows.has(t.next)) play(c, t.next, c.speed);
    else if (c.state === STATE.STAND) play(c, STATE.NADE, NADE_SPEED);
    else if (c.state === STATE.BANZAI) { if (--c.repeats > 0) play(c, STATE.BANZAI); else c.done = true; }
  }
  return {
    inviter, answerer,
    get done() { return inviter.done && answerer.done; },
    step() {
      const out = events.splice(0);
      tick++;
      if (!cheer && tick >= CHEER_AT) { cheer = true; play(inviter, STATE.HASYAGU); play(answerer, STATE.HASYAGU); }
      advance(inviter); advance(answerer);
      // The 21st frame of the cheer: both hop back (and the game lays the egg).
      if (cheer && ++cheerFrames === 21) out.push('hop');
      return out;
    },
  };
}

/** The heart icon's scale on frame n: grows 0.2 a frame to 2.8, holds, then shrinks 0.25 a frame. */
export function heartScale(n, hold) {
  const grown = Math.min(HEART.max, n * HEART.grow);
  return n <= hold ? grown : Math.max(0, HEART.max - (n - hold) * HEART.shrink);
}

function glbExtras(bytes) {
  try {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + view.getUint32(12, true)))).asset?.extras ?? null;
  } catch { return null; }
}

/** Play the scene in `host` (a small box in the menu). Resolves when it ends; never throws. */
export async function playBreedingScene(host) {
  const B = await import('@babylonjs/core'); await import('@babylonjs/loaders/glTF/index.js');
  // The player's own game files first: the child with the four motions the scene plays.
  const game = await import('./game-assets.js').catch(() => null);
  let bytes = await game?.gameChaoMotionsGlb(0, Object.values(STATE));
  if (!bytes) {
    const response = await fetch('/api/assets/chao-behaviour.glb').catch(() => null);
    if (!response?.ok) return false;
    bytes = new Uint8Array(await response.arrayBuffer());
  }
  const rows = new Map((glbExtras(bytes)?.chaoBehaviour?.clips ?? []).map(row => [row.nativeState, row]));
  const canvas = document.createElement('canvas'); canvas.className = 'breeding-scene'; canvas.width = 480; canvas.height = 300;
  canvas.setAttribute('role', 'img'); canvas.setAttribute('aria-label', 'Your Chao breeding');
  host.append(canvas);
  const engine = new B.Engine(canvas, true, { preserveDrawingBuffer: false }), scene = new B.Scene(engine);
  scene.clearColor = new B.Color4(0, 0, 0, 0);
  new B.HemisphericLight('light', new B.Vector3(0, 1, 0.3), scene);
  const container = await B.LoadAssetContainerAsync(bytes, scene, { pluginExtension: '.glb' });
  const pair = [-1.5, 1.5].map((x, n) => {
    const entries = container.instantiateModelsToScene(name => name, false);
    const root = entries.rootNodes[0];
    root.position.x = x; root.rotate(B.Axis.Y, n ? -Math.PI / 2 : Math.PI / 2); // a turn on top of the loader's own root turn // face each other, 3.0 apart (touching)
    for (const g of entries.animationGroups) g.stop();
    return { root, groups: new Map(entries.animationGroups.map(g => [g.name, g])), playing: null };
  });
  const { min, max } = pair[0].root.getHierarchyBoundingVectors();
  const height = max.y - min.y;
  new B.ArcRotateCamera('camera', -Math.PI / 2, 1.25, height * 3.2, new B.Vector3(0, height / 2, 0), scene);
  const heartMaterial = new B.StandardMaterial('heart', scene);
  const heartUrl = await import('./game-assets.js').then(m => m.gameTextureUrl('chao-icon-heart.png')).catch(() => null);
  if (heartUrl) { heartMaterial.diffuseTexture = new B.Texture(heartUrl, scene); heartMaterial.diffuseTexture.hasAlpha = true; }
  heartMaterial.emissiveColor = new B.Color3(1, 0.45, 0.6); heartMaterial.disableLighting = true; heartMaterial.backFaceCulling = false;
  const hearts = pair.map(c => {
    const heart = B.MeshBuilder.CreatePlane('heart', { size: height * 0.12 }, scene);
    heart.material = heartMaterial; heart.billboardMode = B.Mesh.BILLBOARDMODE_ALL;
    heart.position.set(c.root.position.x, max.y + 0.7, 0); // shortcut: node 33 isn't identified, so above the model's top
    return heart;
  });
  const jingle = new Audio('/api/assets/jingle-chao-g-dance.mp3');
  // The hop on the 21st cheer frame (GC ChaoMain.rel .text 0x40488, 0x40938): speed 0.8 up and 0.25 backward.
  // Then one MOV_Control frame each (GC main.dol 0x8002F448): the Chao's gravity -0.055 (CreateChaoExtra 0x3167C)
  // is added to the upward speed and the speed is added to the position. No egg is drawn (the scene's own choice).
  let hop = null;
  const startHop = () => { hop = pair.map((c, n) => ({ c, x: n ? 0.25 : -0.25, y: 0.8 })); };
  const stepHop = () => hop?.forEach(h => {
    if (!h.c) return;
    h.y -= 0.055; h.c.root.position.x += h.x; h.c.root.position.y += h.y;
    // shortcut: the landing (floor bounce and friction in MOV_DetectCollision) is not traced, so the Chao stops on the ground
    if (h.c.root.position.y <= 0) { h.c.root.position.y = 0; h.c = null; }
  });
  const courtship = createCourtship(rows);
  const show = (c, s) => {
    const row = rows.get(s.state), group = row && c.groups.get(row.nativeName);
    if (!group) return;
    if (c.playing !== group) { c.playing?.stop(); group.start(true, 1); group.pause(); c.playing = group; }
    group.goToFrame(s.frame);
  };
  let frame = 0, carry = 0;
  return new Promise(resolve => {
    engine.runRenderLoop(() => {
      carry += engine.getDeltaTime() / (1000 / 60);
      for (; carry >= 1 && !courtship.done; carry--, frame++) {
        for (const event of courtship.step()) {
          if (event === 'jingle') void jingle.play().catch(() => {});
          if (event === 'hop') startHop();
        }
        stepHop();
      }
      show(pair[0], courtship.inviter); show(pair[1], courtship.answerer);
      hearts.forEach(h => h.scaling.setAll(heartScale(frame, 60) / HEART.max));
      scene.render();
      if (courtship.done) {
        engine.stopRenderLoop(); setTimeout(() => { jingle.pause(); scene.dispose(); engine.dispose(); canvas.remove(); resolve(true); }, 800);
      }
    });
  }).catch(() => false);
}
