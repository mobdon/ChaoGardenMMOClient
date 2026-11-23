// The player's own Chao as the game's Chao Select window shows it: the normal Chao standing
// (motion 0, alm_tati, set by ALBHV_Think via AL_SetMotionLink at ChaoMain 0x47934), turned
// -0x38E (about -5 deg) as CreateChaoExtra places it (ChaoMain 0x55A20). Used for the MMO card
// icon (small) and the login screen (big).
import { captureChaoPose, cloneNativeMotion } from './chao-locomotion.js';
import { gameChaoGlb, gameChaoLight, gameHeadBallTables, gameHeadIconTables, gameHeadIconUrls, gameTextureUrl } from './game-assets.js';
import { haloPicture, headBallColour, headIcon } from './chao-head-ball.js';
import { applyChunkDiffuse, applyNativeChaoMaterial, applyNinjaLight, ninjaLight, orderChaoStrips } from './chao-materials.js';
import { attachChaoBody, bodyAssetFor } from './chao-body.js';
import { configureChaoFace, loadChaoFaceManifest, resetChaoFace, setChaoEyePicture, stepChaoFace } from './chao-face.js';
import { applyChaoLook, loadChaoLookManifest } from './chao-look.js';

// Asset key -> index in the game's own Chao model table (AL_RootObject).
const GAME_MODEL = { chao: 0, 'neutral-chaos': 108, 'hero-chaos': 114, 'dark-chaos': 120, tails: 126, knuckles: 132, amy: 138 };
// Asset key -> the game's Chao type number of that model (a new child when the key has none).
const GAME_TYPE = { 'neutral-chaos': 20, 'hero-chaos': 21, 'dark-chaos': 22, tails: 23, knuckles: 24, amy: 25 };

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

/** `player`'s Chao model in a scene of its own on `canvas`, standing as imported with the camera on
 * its front; null when its asset is missing. Also draws the phone's picture sheet (src/tiny-sheet.js). */
export async function openChaoScene(canvas, player, options = {}) {
  const B = await import('@babylonjs/core'); await import('@babylonjs/loaders/glTF/index.js');
  const load = async key => {
    const response = await fetch(`/api/assets/${key}.glb`).catch(() => null);
    return response?.ok ? parse(key, new Uint8Array(await response.arrayBuffer())) : null;
  };
  const parse = async (key, bytes) => {
    const container = await B.LoadAssetContainerAsync(bytes, scene, { pluginExtension: '.glb' });
    container.extras = glbExtras(bytes);
    // Drawn as the hub draws a Chao (native textures, masked face); the engine's own lights are not used.
    if (key !== 'chao-behaviour') {
      for (const mat of container.materials) if ('albedoColor' in mat) { applyNativeChaoMaterial(key, mat); applyChunkDiffuse(mat); }
      for (const mesh of container.meshes) mesh.hasVertexAlpha = false;
    }
    return container;
  };
  const engine = new B.Engine(canvas, true, { alpha: true, ...options }), scene = new B.Scene(engine);
  scene.clearColor = new B.Color4(0, 0, 0, 0);
  let open = true; // the page and the next mount may both clear it
  const dispose = () => { if (!open) return; open = false; engine.stopRenderLoop(); scene.dispose(); engine.dispose(); };
  let key = portraitAsset(player);
  // The player's own game files first: the model and its stand motion (number 0) in one file.
  // A family body: the family's models as one morphing model, coloured by the Chao's own values.
  const body = player.body, first = Number.isInteger(body?.modelBaseIndex) ? body.modelBaseIndex : 0;
  const look = body && { type: body.type, hPos: body.runPower, vPos: body.swimFly, aPos: body.heroDark, growth: body.growth };
  const game = name => name.startsWith('chao-body-') ? gameChaoGlb(first, 0, look || undefined, true)
    : name in GAME_MODEL ? gameChaoGlb(GAME_MODEL[name], 0) : null;
  let own = await game(key);
  let [model, source] = own ? [await parse(key, own)] : await Promise.all([load(key), load('chao-behaviour')]);
  // The family bodies are a local conversion; without them the original child model, as the hub falls back.
  if (!model && key.startsWith('chao-body-')) model = (own = await game(key = 'chao')) ? await parse(key, own) : await load(key);
  if (own) source = model;
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
  if (own) {
    // A single model from the game files still names a body picture on its eyes; the game swaps in the type's own.
    if (!key.startsWith('chao-body-') && await loadChaoFaceManifest() && !scene.isDisposed) setChaoEyePicture(visual, GAME_TYPE[key] ?? 2);
    if (scene.isDisposed) return null;
    orderChaoStrips(visual);
  }
  // Built from the game files: lit by the game's own Chao Select light. Without the files it stays unlit, as before.
  const lightRow = own && await gameChaoLight();
  if (scene.isDisposed) return null;
  if (lightRow) {
    const light = ninjaLight(lightRow);
    // Every frame: the face and the look swap in materials of their own while the Chao is shown.
    // Once now too, so a caller that waits for the scene to be ready (the picture sheet) waits for the lit shaders.
    const lightAll = () => { for (const mesh of visual.getChildMeshes(false)) applyNinjaLight(mesh.material, light); };
    lightAll(); scene.onBeforeRenderObservable.add(lightAll);
  }
  const { min, max } = visual.getHierarchyBoundingVectors();
  const height = max.y - min.y;
  const camera = new B.ArcRotateCamera('camera', Math.PI / 2, Math.PI / 2.1, height * 2.4, new B.Vector3(0, min.y + height / 2, 0), scene);
  camera.minZ = 0.01;
  return { B, engine, scene, camera, key, visual, imported: { entries, visual }, source, own, look, dispose };
}

async function drawChaoPortrait(canvas, player) {
  const opened = await openChaoScene(canvas, player);
  if (!opened) return () => {};
  const { B, engine, scene, camera, key, visual, imported, source, own, look, dispose } = opened;
  visual.rotate(B.Axis.Y, TURN); // the select window's turn; the camera sits on the Chao's front (+z)
  const stand = own ? imported.entries.animationGroups[0]
    : source && cloneNativeMotion(imported, source, captureChaoPose(imported), 'alm_tati', 'Stand');
  // The motion table's own PlaySpeed (native frames per 60 Hz tick); the clip's keys are one per native frame.
  const speed = source?.extras?.chaoBehaviour?.clips?.find(row => row.nativeName === (own ? 'motion' : 'alm_tati'))?.tableProperties?.PlaySpeed;
  stand?.start(true, Number.isFinite(speed) && speed > 0 ? speed : 1);
  // The game's own head icon (ChaoMain.rel): AL_IconDrawSub 0x12B8C draws a quad +-0.55 wide, 0.3 up and 0.3 toward
  // the viewer in view space, squashed by k = 0.92 + 0.08*(1+sin phase), phase += 0x780/frame; AL_CalcShadowPos 0x100C
  // puts it at node 33 + 0.045*sin(psi), psi += 0x470/frame; colour by AL_CalcIconColor 0x14574 (chao-head-ball.js),
  // a fresh child's 255,255,76 without the tables. Which picture, how big and how blended: headIcon().
  // shortcut: the icon sits on its node; the game lets it trail the head on a spring (fn_60_13340). Add it when the portrait moves.
  const type = look?.type ?? GAME_TYPE[key] ?? 2, icon = headIcon(type);
  const nodes = visual.getDescendants(false), node33 = nodes.find(node => /^033_object/.test(node.name));
  const node15 = nodes.find(node => /^015_object/.test(node.name));
  if ((key.startsWith('chao-body-') || own) && node33) {
    const iconTables = icon.kind === 'halo' || icon.kind === 'flame' ? await gameHeadIconTables() : null;
    // Picture numbers this icon can show: the ring's 35 are 7..41 (the halo table's 1..35 plus 6).
    const numbers = icon.kind === 'halo' ? iconTables && [...new Set(iconTables.halo)].map(n => n + 6)
      : icon.kind === 'flame' ? iconTables?.flame : [icon.picture];
    const urls = icon.kind === 'ball' ? [await gameTextureUrl('emotion-sphere.png') ?? '/api/assets/emotion-sphere.png']
      : numbers && await gameHeadIconUrls(numbers);
    const tables = await gameHeadBallTables();
    const [r, g, b] = tables && headBallColour(tables, look || { type }) || (type === 2 ? [255, 255, 76] : []);
    if (scene.isDisposed) return () => {};
    // The plain ball has a server picture to fall back on; the other icons need the game files and a colour.
    if (urls && r !== undefined && (icon.kind !== 'halo' || node15)) {
      const pictures = new Map(urls.map((url, i) => {
        const texture = new B.Texture(url, scene, true, true, B.Texture.BILINEAR_SAMPLINGMODE);
        texture.hasAlpha = true;
        return [icon.kind === 'ball' ? 0 : numbers[i], texture];
      }));
      const ball = B.MeshBuilder.CreatePlane('native-head-icon', { size: 1.1 }, scene);
      const mat = new B.StandardMaterial('native-head-icon', scene);
      mat.diffuseTexture = pictures.values().next().value; mat.useAlphaFromDiffuseTexture = true;
      mat.disableLighting = true; mat.emissiveColor = new B.Color3(r / 255, g / 255, b / 255); mat.backFaceCulling = false;
      if (icon.additive) { mat.alphaMode = B.Engine.ALPHA_ADD; mat.disableDepthWrite = true; }
      ball.material = mat; ball.rotationQuaternion = new B.Quaternion();
      let phase = 0, psi = 0, ticks = 0, frame = 0, wait = 0, turn = 0;
      scene.onBeforeRenderObservable.add(() => {
        ticks += Math.min(engine.getDeltaTime(), 100) * 0.06;
        for (; ticks >= 1; ticks -= 1) {
          phase += 0x780; psi += 0x470;
          if ((wait += 1) > 2) { wait = 0; frame = (frame + 1) % 16; } // fn_60_13340 0x13620: the next flame picture every third frame
          turn = Math.random() * 2 * Math.PI; // 0x1366C: a new random angle every frame
        }
        const k = 0.92 + 0.08 * (1 + Math.sin(phase / 0x10000 * 2 * Math.PI));
        node33.computeWorldMatrix(true);
        ball.position.copyFrom(node33.getAbsolutePosition());
        ball.position.y += 0.045 * Math.sin(psi / 0x10000 * 2 * Math.PI);
        ball.position.addInPlace(camera.getDirection(B.Axis.Y).scale(0.3)).addInPlace(camera.getDirection(B.Axis.Z).scale(-0.3));
        ball.scaling.set(k * icon.scale, (2 - k) * icon.scale, 1);
        let roll = icon.kind === 'glow' ? turn : 0;
        if (icon.kind === 'flame') mat.diffuseTexture = pictures.get(iconTables.flame[frame]);
        if (icon.kind === 'halo') {
          // The ring's picture by how far the head's up direction (node 15, AL_CalcShadowPos 0x10C0) leans from the
          // line to the camera, turned so its axis lies along that direction on screen (0x130B4..0x130F8).
          const up = B.Vector3.TransformNormal(B.Axis.Y, node15.computeWorldMatrix(true)).normalize();
          mat.diffuseTexture = pictures.get(haloPicture(iconTables.halo, B.Vector3.Dot(up, camera.position.subtract(ball.position).normalize())));
          const seen = B.Vector3.TransformNormal(up, camera.getViewMatrix());
          roll = -Math.atan2(seen.x, seen.y);
        }
        // Faces the viewer as the game's view-space quad does, then turns about the view axis.
        B.Quaternion.FromRotationMatrixToRef(camera.getWorldMatrix().getRotationMatrix(), ball.rotationQuaternion);
        ball.rotationQuaternion.multiplyInPlace(B.Quaternion.RotationAxis(B.Axis.Z, roll));
      });
    }
  }
  engine.runRenderLoop(() => scene.render());
  return dispose;
}
