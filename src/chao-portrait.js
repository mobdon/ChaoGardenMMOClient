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

/** Render `player`'s Chao into `canvas`. Returns a dispose function; a missing asset leaves the canvas empty. */
export async function mountChaoPortrait(canvas, player) {
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
  const engine = new B.Engine(canvas, true, { alpha: true }), scene = new B.Scene(engine);
  scene.clearColor = new B.Color4(0, 0, 0, 0);
  const dispose = () => { engine.stopRenderLoop(); scene.dispose(); engine.dispose(); };
  let key = portraitAsset(player);
  let [model, source] = await Promise.all([load(key), load('chao-behaviour')]);
  // The family bodies are a local conversion; without them the original child model, as the hub falls back.
  if (!model && key.startsWith('chao-body-')) model = await load(key = 'chao');
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
  const stand = source && cloneNativeMotion(imported, source, captureChaoPose(imported), 'alm_tati', 'Stand');
  // The motion table's own PlaySpeed (native frames per 60 Hz tick); the clip's keys are one per native frame.
  const speed = source?.extras?.chaoBehaviour?.clips?.find(row => row.nativeName === 'alm_tati')?.tableProperties?.PlaySpeed;
  stand?.start(true, Number.isFinite(speed) && speed > 0 ? speed : 1);
  const { min, max } = visual.getHierarchyBoundingVectors();
  const height = max.y - min.y;
  const camera = new B.ArcRotateCamera('camera', Math.PI / 2, Math.PI / 2.1, height * 2.4, new B.Vector3(0, min.y + height / 2, 0), scene);
  camera.minZ = 0.01;
  engine.runRenderLoop(() => scene.render());
  return dispose;
}
