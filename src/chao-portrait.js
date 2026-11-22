// The player's own Chao as the game's Chao Select window shows it: the normal Chao standing
// (motion 0, alm_tati, set by ALBHV_Think via AL_SetMotionLink at ChaoMain 0x47934), turned
// -0x38E (about -5 deg) as CreateChaoExtra places it (ChaoMain 0x55A20). Used for the MMO card
// icon (small) and the login screen (big).
import { captureChaoPose, cloneNativeMotion } from './chao-locomotion.js';

const TURN = -0x38e / 0x10000 * Math.PI * 2;

/** Same Chaos family pick as the hub; everyone else the base Chao model. */
export function portraitAsset(player) {
  if (player.asset) return player.asset;
  if (player.admin || String(player.form || '').includes('chaos')) {
    return player.alignment >= 60 ? 'hero-chaos' : player.alignment <= -60 ? 'dark-chaos' : 'neutral-chaos';
  }
  // shortcut: body-family Chao show the base model here, upgrade when the snapshot carries its asset key.
  return 'chao';
}

/** Render `player`'s Chao into `canvas`. Returns a dispose function; a missing asset leaves the canvas empty. */
export async function mountChaoPortrait(canvas, player) {
  const B = await import('@babylonjs/core'); await import('@babylonjs/loaders/glTF/index.js');
  const load = async key => {
    const response = await fetch(`/api/assets/${key}.glb`).catch(() => null);
    return response?.ok ? B.LoadAssetContainerAsync(new Uint8Array(await response.arrayBuffer()), scene, { pluginExtension: '.glb' }) : null;
  };
  const engine = new B.Engine(canvas, true, { alpha: true }), scene = new B.Scene(engine);
  scene.clearColor = new B.Color4(0, 0, 0, 0);
  new B.HemisphericLight('light', new B.Vector3(0, 1, 0.4), scene);
  const dispose = () => { engine.stopRenderLoop(); scene.dispose(); engine.dispose(); };
  const [model, source] = await Promise.all([load(portraitAsset(player)), load('chao-behaviour')]);
  if (!model) { dispose(); return () => {}; }
  const entries = model.instantiateModelsToScene(name => name, false);
  for (const group of entries.animationGroups) group.stop();
  const visual = entries.rootNodes[0];
  visual.rotate(B.Axis.Y, TURN); // the select window's turn; the camera sits on the Chao's front (+z)
  const imported = { entries, visual };
  const stand = source && cloneNativeMotion(imported, source, captureChaoPose(imported), 'alm_tati', 'Stand');
  stand?.start(true, 1);
  const { min, max } = visual.getHierarchyBoundingVectors();
  const height = max.y - min.y;
  const camera = new B.ArcRotateCamera('camera', Math.PI / 2, Math.PI / 2.1, height * 2.4, new B.Vector3(0, min.y + height / 2, 0), scene);
  camera.minZ = 0.01;
  engine.runRenderLoop(() => scene.render());
  return dispose;
}
