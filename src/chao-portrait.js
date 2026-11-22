// The player's own Chao as the game's Chao Select window shows it: the normal Chao standing
// (motion 0, alm_tati, set by ALBHV_Think via AL_SetMotionLink at ChaoMain 0x47934), turned
// -0x38E (about -5 deg) as CreateChaoExtra places it (ChaoMain 0x55A20). Used for the MMO card
// icon (small) and the login screen (big).
import { captureChaoPose, cloneNativeMotion } from './chao-locomotion.js';
import { gameChaoGlb } from './game-assets.js';

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
  // shortcut: body-family Chao show the base model here, upgrade when the snapshot carries its asset key.
  return 'chao';
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
  const container = async bytes => {
    const made = await B.LoadAssetContainerAsync(bytes, scene, { pluginExtension: '.glb' });
    made.extras = glbExtras(bytes);
    return made;
  };
  const load = async key => {
    const response = await fetch(`/api/assets/${key}.glb`).catch(() => null);
    return response?.ok ? container(new Uint8Array(await response.arrayBuffer())) : null;
  };
  const engine = new B.Engine(canvas, true, { alpha: true }), scene = new B.Scene(engine);
  scene.clearColor = new B.Color4(0, 0, 0, 0);
  new B.HemisphericLight('light', new B.Vector3(0, 1, 0.4), scene);
  const dispose = () => { engine.stopRenderLoop(); scene.dispose(); engine.dispose(); };
  // The player's own game files first: the model and its stand motion (number 0) in one file.
  const key = portraitAsset(player), own = key in GAME_MODEL ? await gameChaoGlb(GAME_MODEL[key], 0) : null;
  const model = own ? await container(own) : await load(key);
  if (!model) { dispose(); return () => {}; }
  const entries = model.instantiateModelsToScene(name => name, false);
  for (const group of entries.animationGroups) group.stop();
  const visual = entries.rootNodes[0];
  visual.rotate(B.Axis.Y, TURN); // the select window's turn; the camera sits on the Chao's front (+z)
  const imported = { entries, visual };
  const source = own ? model : await load('chao-behaviour');
  const stand = own ? entries.animationGroups[0]
    : source && cloneNativeMotion(imported, source, captureChaoPose(imported), 'alm_tati', 'Stand');
  // The motion table's own PlaySpeed (native frames per 60 Hz tick); the clip's keys are one per native frame.
  const speed = source?.extras?.chaoBehaviour?.clips?.find(row => row.nativeName === (own ? 'motion' : 'alm_tati'))?.tableProperties?.PlaySpeed;
  stand?.start(true, Number.isFinite(speed) && speed > 0 ? speed : 1);
  const { min, max } = visual.getHierarchyBoundingVectors();
  const height = max.y - min.y;
  const camera = new B.ArcRotateCamera('camera', Math.PI / 2, Math.PI / 2.1, height * 2.4, new B.Vector3(0, min.y + height / 2, 0), scene);
  camera.minZ = 0.01;
  engine.runRenderLoop(() => scene.render());
  return dispose;
}
