// The player's own Chao as the game's Chao Select window shows it: the normal Chao standing
// (motion 0, alm_tati, set by ALBHV_Think via AL_SetMotionLink at ChaoMain 0x47934), turned
// -0x38E (about -5 deg) as CreateChaoExtra places it (ChaoMain 0x55A20). Used for the MMO card
// icon (small) and the login screen (big).
import { captureChaoPose, cloneNativeMotion } from './chao-locomotion.js';
import { applyNativeChaoMaterial } from './chao-materials.js';
import { attachChaoBody, bodyAssetFor } from './chao-body.js';
import { configureChaoFace, loadChaoFaceManifest, resetChaoFace, stepChaoFace } from './chao-face.js';
import { applyChaoLook, loadChaoLookManifest } from './chao-look.js';

// Babylon is left-handed and mirrors the game's X, so the game's -0x38E turn is +0x38E here.
const TURN = 0x38e / 0x10000 * Math.PI * 2;

/** Same Chaos family pick as the hub; everyone else the base Chao model. */
export function portraitAsset(player) {
  if (player.asset) return player.asset;
  if (player.admin || String(player.form || '').includes('chaos')) {
    return player.alignment >= 60 ? 'hero-chaos' : player.alignment <= -60 ? 'dark-chaos' : 'neutral-chaos';
  }
  // The hub's native body family, with its own eyes and look (a saved slot carries no body: the child body).
  // shortcut: a slot's adult Chao shows the child body here, upgrade when the slot carries the server body.
  return bodyAssetFor(player) || 'chao-body-al_ncz';
}

function glbExtras(bytes) {
  try {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + view.getUint32(12, true)))).asset?.extras ?? null;
  } catch { return null; }
}

/** `player`'s Chao model in a scene of its own on `canvas`, standing as imported with the camera on
 * its front; null when its asset is missing. */
export async function openChaoScene(canvas, player, options = {}) {
  const B = await import('@babylonjs/core'); await import('@babylonjs/loaders/glTF/index.js');
  const load = async key => {
    const response = await fetch(`/api/assets/${key}.glb`).catch(() => null);
    if (!response?.ok) return null;
    const bytes = new Uint8Array(await response.arrayBuffer());
    const container = await B.LoadAssetContainerAsync(bytes, scene, { pluginExtension: '.glb' });
    container.extras = glbExtras(bytes);
    // Drawn as the hub draws a Chao (unlit native textures, masked face); a lit PBR default read as flat with dark eyes.
    if (key !== 'chao-behaviour') {
      for (const mat of container.materials) if ('albedoColor' in mat) applyNativeChaoMaterial(key, mat);
      for (const mesh of container.meshes) mesh.hasVertexAlpha = false;
    }
    return container;
  };
  const engine = new B.Engine(canvas, true, { alpha: true, ...options }), scene = new B.Scene(engine);
  scene.clearColor = new B.Color4(0, 0, 0, 0);
  const dispose = () => { engine.stopRenderLoop(); scene.dispose(); engine.dispose(); };
  let key = portraitAsset(player);
  let [model, source] = await Promise.all([load(key), load('chao-behaviour')]);
  // The family bodies are a local conversion; without them the original child model, as the hub falls back.
  if (!model && key.startsWith('chao-body-')) model = await load(key = 'chao');
  if (!model) { dispose(); return null; }
  const entries = model.instantiateModelsToScene(name => name, false);
  for (const group of entries.animationGroups) group.stop();
  const visual = entries.rootNodes[0];
  if (key.startsWith('chao-body-')) {
    // As the hub does for a family body: server morph weights, native look, and the face with its lid motion.
    attachChaoBody(visual, entries.animationGroups, () => player.body);
    await Promise.all([loadChaoFaceManifest(), loadChaoLookManifest()]);
    if (scene.isDisposed) return null;
    applyChaoLook(visual, player.body);
    configureChaoFace(visual, player.body?.typeName);
    resetChaoFace(visual);
    let frames = 0;
    scene.onBeforeRenderObservable.add(() => {
      frames += Math.min(engine.getDeltaTime(), 100) * 0.06;
      for (; frames >= 1; frames -= 1) stepChaoFace(visual);
    });
  }
  const { min, max } = visual.getHierarchyBoundingVectors();
  const height = max.y - min.y;
  const camera = new B.ArcRotateCamera('camera', Math.PI / 2, Math.PI / 2.1, height * 2.4, new B.Vector3(0, min.y + height / 2, 0), scene);
  camera.minZ = 0.01;
  return { B, engine, scene, visual, imported: { entries, visual }, source, dispose, key, camera };
}

// One Chao per canvas at a time: a mount that starts while another is still loading would put two engines
// on one canvas, and neither draws. Each mount waits for the one before it, clears it, and gives way to a newer one.
const mounts = new WeakMap();

/** Render `player`'s Chao into `canvas`. Returns a dispose function; a missing asset leaves the canvas empty. */
export function mountChaoPortrait(canvas, player) {
  const before = mounts.get(canvas) || Promise.resolve(() => {});
  const task = before.then(dispose => { dispose(); return mounts.get(canvas) === task ? drawChaoPortrait(canvas, player) : () => {}; })
    .catch(() => () => {});
  mounts.set(canvas, task);
  return task;
}

async function drawChaoPortrait(canvas, player) {
  const opened = await openChaoScene(canvas, player);
  if (!opened) return () => {};
  const { B, engine, scene, visual, imported, source, key, camera } = opened;
  let open = true;
  const dispose = () => { if (open) { open = false; opened.dispose(); } }; // the page and the next mount may both clear it
  visual.rotate(B.Axis.Y, TURN); // the select window's turn; the camera sits on the Chao's front (+z)
  const stand = source && cloneNativeMotion(imported, source, captureChaoPose(imported), 'alm_tati', 'Stand');
  // The motion table's own PlaySpeed (native frames per 60 Hz tick); the clip's keys are one per native frame.
  const speed = source?.extras?.chaoBehaviour?.clips?.find(row => row.nativeName === 'alm_tati')?.tableProperties?.PlaySpeed;
  stand?.start(true, Number.isFinite(speed) && speed > 0 ? speed : 1);
  // The head ball as ChaoMain.rel (GSNE8P) draws it for a child with no evolution; other types are not covered yet.
  // AL_IconDrawSub 0x12B8C: quad +-0.55 (rodata F90/F94), view-space y +0.3 (F98), AL_ICON tex 0, scale (k, 2-k),
  // k = 0.92 + 0.08*(1 + sin phase), phase += 0x780/frame (fn_60_13340); SRC_ALPHA/INV_SRC_ALPHA.
  // AL_CalcShadowPos 0x100C: base = node 33 translation, y + 0.045*sin(psi), psi += 0x470/frame.
  // AL_CalcIconColor 0x14574: child with zero evolution = .data 0x2FDC = (255, 255, 76).
  // shortcut: the quad's view-space z 0.3 and the follow spring are left out (a standing Chao), port them for a moving one.
  const node33 = visual.getDescendants(false).find(node => /^033_object/.test(node.name));
  if (key.startsWith('chao-body-') && node33) {
    const ball = B.MeshBuilder.CreatePlane('native-head-ball', { size: 1.1 }, scene);
    const mat = new B.StandardMaterial('native-head-ball', scene);
    mat.diffuseTexture = new B.Texture('/api/assets/emotion-sphere.png', scene, true, true, B.Texture.BILINEAR_SAMPLINGMODE);
    mat.diffuseTexture.hasAlpha = true; mat.useAlphaFromDiffuseTexture = true;
    mat.disableLighting = true; mat.emissiveColor = new B.Color3(1, 1, 76 / 255); mat.backFaceCulling = false;
    ball.material = mat; ball.billboardMode = B.Mesh.BILLBOARDMODE_ALL;
    let phase = 0, psi = 0, ticks = 0;
    scene.onBeforeRenderObservable.add(() => {
      ticks += Math.min(engine.getDeltaTime(), 100) * 0.06;
      for (; ticks >= 1; ticks -= 1) { phase += 0x780; psi += 0x470; }
      const k = 0.92 + 0.08 * (1 + Math.sin(phase / 0x10000 * 2 * Math.PI));
      node33.computeWorldMatrix(true);
      ball.position.copyFrom(node33.getAbsolutePosition());
      ball.position.y += 0.045 * Math.sin(psi / 0x10000 * 2 * Math.PI);
      ball.position.addInPlace(camera.getDirection(B.Axis.Y).scale(0.3));
      ball.scaling.set(k, 2 - k, 1);
    });
  }
  engine.runRenderLoop(() => scene.render());
  return dispose;
}
