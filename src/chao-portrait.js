// The player's own Chao as the game's Chao Select window shows it: the normal Chao standing
// (motion 0, alm_tati, set by ALBHV_Think via AL_SetMotionLink at ChaoMain 0x47934), turned
// -0x38E (about -5 deg) as CreateChaoExtra places it (ChaoMain 0x55A20). Used for the MMO card
// icon (small) and the login screen (big).
import { captureChaoPose, cloneNativeMotion } from './chao-locomotion.js';
import { gameChaoGlb, gameTextureUrl } from './game-assets.js';
import { applyNativeChaoMaterial } from './chao-materials.js';
import { attachChaoBody, bodyAssetFor } from './chao-body.js';
import { configureChaoFace, loadChaoFaceManifest, resetChaoFace, stepChaoFace } from './chao-face.js';
import { applyChaoLook, loadChaoLookManifest } from './chao-look.js';

// Asset key -> index in the game's own Chao model table (AL_RootObject).
const GAME_MODEL = { chao: 0, 'neutral-chaos': 108, 'hero-chaos': 114, 'dark-chaos': 120, tails: 126, knuckles: 132, amy: 138 };

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
  const B = await import('@babylonjs/core'); await import('@babylonjs/loaders/glTF/index.js');
  const load = async key => {
    const response = await fetch(`/api/assets/${key}.glb`).catch(() => null);
    return response?.ok ? parse(key, new Uint8Array(await response.arrayBuffer())) : null;
  };
  const parse = async (key, bytes) => {
    const container = await B.LoadAssetContainerAsync(bytes, scene, { pluginExtension: '.glb' });
    container.extras = glbExtras(bytes);
    // Drawn as the hub draws a Chao (unlit native textures, masked face); a lit PBR default read as flat with dark eyes.
    if (key !== 'chao-behaviour') {
      for (const mat of container.materials) if ('albedoColor' in mat) applyNativeChaoMaterial(key, mat);
      for (const mesh of container.meshes) mesh.hasVertexAlpha = false;
    }
    return container;
  };
  const engine = new B.Engine(canvas, true, { alpha: true }), scene = new B.Scene(engine);
  scene.clearColor = new B.Color4(0, 0, 0, 0);
  let open = true; // the page and the next mount may both clear it
  const dispose = () => { if (!open) return; open = false; engine.stopRenderLoop(); scene.dispose(); engine.dispose(); };
  let key = portraitAsset(player);
  // The player's own game files first: the model and its stand motion (number 0) in one file.
  const game = name => name in GAME_MODEL ? gameChaoGlb(GAME_MODEL[name], 0) : null;
  let own = await game(key);
  let [model, source] = own ? [await parse(key, own)] : await Promise.all([load(key), load('chao-behaviour')]);
  // The family bodies are a local conversion; without them the original child model, as the hub falls back.
  if (!model && key.startsWith('chao-body-')) model = (own = await game(key = 'chao')) ? await parse(key, own) : await load(key);
  if (own) source = model;
  if (!model) { dispose(); return () => {}; }
  const entries = model.instantiateModelsToScene(name => name, false);
  for (const group of entries.animationGroups) group.stop();
  const visual = entries.rootNodes[0];
  visual.rotate(B.Axis.Y, TURN); // the select window's turn; the camera sits on the Chao's front (+z)
  if (key.startsWith('chao-body-')) {
    // As the hub does for a family body: server morph weights, native look, and the face with its lid motion.
    attachChaoBody(visual, entries.animationGroups, () => player.body);
    await Promise.all([loadChaoFaceManifest(), loadChaoLookManifest()]);
    if (scene.isDisposed) return () => {};
    applyChaoLook(visual, player.body);
    configureChaoFace(visual, player.body?.typeName);
    resetChaoFace(visual);
    let frames = 0;
    scene.onBeforeRenderObservable.add(() => {
      frames += Math.min(engine.getDeltaTime(), 100) * 0.06;
      for (; frames >= 1; frames -= 1) stepChaoFace(visual);
    });
  }
  const imported = { entries, visual };
  const stand = own ? entries.animationGroups[0]
    : source && cloneNativeMotion(imported, source, captureChaoPose(imported), 'alm_tati', 'Stand');
  // The motion table's own PlaySpeed (native frames per 60 Hz tick); the clip's keys are one per native frame.
  const speed = source?.extras?.chaoBehaviour?.clips?.find(row => row.nativeName === (own ? 'motion' : 'alm_tati'))?.tableProperties?.PlaySpeed;
  stand?.start(true, Number.isFinite(speed) && speed > 0 ? speed : 1);
  const { min, max } = visual.getHierarchyBoundingVectors();
  const height = max.y - min.y;
  const camera = new B.ArcRotateCamera('camera', Math.PI / 2, Math.PI / 2.1, height * 2.4, new B.Vector3(0, min.y + height / 2, 0), scene);
  camera.minZ = 0.01;
  // The game's own head ball for a default child (ChaoMain.rel): AL_IconDrawSub 0x12B8C quad +-0.55, +0.3 up in view,
  // squash k = 0.92 + 0.08*(1+sin phase), phase += 0x780/frame; AL_CalcShadowPos 0x100C node 33 + 0.045*sin(psi),
  // psi += 0x470/frame; AL_CalcIconColor 0x14574 zero-evolution child colour 255,255,76.
  // shortcut: other body types' ball colours and ring branches are not ported; add them when the portrait shows evolved Chao.
  const node33 = visual.getDescendants(false).find(node => /^033_object/.test(node.name));
  if ((key.startsWith('chao-body-') || own && key === 'chao') && node33) {
    const picture = await gameTextureUrl('emotion-sphere.png') ?? '/api/assets/emotion-sphere.png';
    if (scene.isDisposed) return () => {};
    const ball = B.MeshBuilder.CreatePlane('native-head-ball', { size: 1.1 }, scene);
    const mat = new B.StandardMaterial('native-head-ball', scene);
    mat.diffuseTexture = new B.Texture(picture, scene, true, true, B.Texture.BILINEAR_SAMPLINGMODE);
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
