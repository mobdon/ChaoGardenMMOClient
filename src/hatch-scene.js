// The game's egg hatch, played small once a new Chao is named. Timing is the
// game's own (ChaoMain GSNE8P AL_EggBornPon 0x9D94 / AL_EggBornKyoro 0x9B5C;
// see BREEDING.md section 5). It starts at the shakes (AL_EggJump 0x9FCC): the
// whole egg wobbles 2 times (60%) or 3, its 16-frame motion at 0.16 a frame.
const EGG = { Pon: { frames: 27, chao: 206 }, Kyoro: { frames: 20, chao: 205 } };
const EGG_SPEED = 0.16, CHAO_SPEED = 0.2, CHAO_END = 19, SHAKE_FRAMES = 16;

/** One native frame at a time: shakes, then the egg's hatch frame and the newborn's. 50/50 Pon or Kyoro. */
export function createHatch(rand = Math.random) {
  const shakes = rand() < 0.6 ? 2 : 3;
  const kind = rand() < 0.5 ? 'Pon' : 'Kyoro', { frames, chao } = EGG[kind];
  const out = {
    kind, shakes, frames, chaoState: chao, shakeFrame: 0, eggFrame: 0, chaoFrame: 0,
    get shaking() { return out.shakeFrame < shakes * SHAKE_FRAMES; },
    get done() { return out.eggFrame >= frames && out.chaoFrame >= CHAO_END; },
    step() {
      if (out.shaking) { out.shakeFrame = Math.min(shakes * SHAKE_FRAMES, out.shakeFrame + EGG_SPEED); return; }
      out.eggFrame = Math.min(frames, out.eggFrame + EGG_SPEED);
      out.chaoFrame = Math.min(CHAO_END, out.chaoFrame + CHAO_SPEED);
    },
  };
  return out;
}

function glbExtras(bytes) {
  try {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + view.getUint32(12, true)))).asset?.extras ?? null;
  } catch { return null; }
}
const load = (B, scene, url) => fetch(url).then(r => r.ok ? r.arrayBuffer() : null)
  .then(buffer => buffer && LoadGlb(B, scene, new Uint8Array(buffer))).catch(() => null);
// The converted egg motions keep their own key timing; play them by native frame over their whole range.
const seek = (group, frame, frames) => group?.goToFrame(group.from + (group.to - group.from) * frame / frames);
const LoadGlb = (B, scene, bytes) => B.LoadAssetContainerAsync(bytes, scene, { pluginExtension: '.glb' }).then(c => ({ c, bytes }));

/** Play the hatch in `host`. Resolves when it ends; never throws. */
export async function playHatchScene(host) {
  try {
    const B = await import('@babylonjs/core'); await import('@babylonjs/loaders/glTF/index.js');
    const canvas = document.createElement('canvas'); canvas.className = 'breeding-scene'; canvas.width = 480; canvas.height = 300;
    canvas.setAttribute('role', 'img'); canvas.setAttribute('aria-label', 'Your egg hatching');
    host.append(canvas);
    const engine = new B.Engine(canvas, true), scene = new B.Scene(engine);
    scene.clearColor = new B.Color4(0, 0, 0, 0);
    new B.HemisphericLight('light', new B.Vector3(0, 1, 0.3), scene);
    const [egg, chao, whole] = await Promise.all(['chao-egg-hatching', 'chao-behaviour', 'chao-egg'].map(n => load(B, scene, `/api/assets/${n}.glb`)));
    const finish = () => { scene.dispose(); engine.dispose(); canvas.remove(); };
    if (!egg) { finish(); return false; }
    const hatch = createHatch();
    const eggEntries = egg.c.instantiateModelsToScene(name => name, false);
    const eggGroup = eggEntries.animationGroups.find(g => g.name === hatch.kind);
    for (const g of eggEntries.animationGroups) g.stop();
    eggGroup?.start(false, 1); eggGroup?.pause();
    // The game shakes the one-piece egg (object_alo_dummyegg with its "Egg" motion), then swaps to the 3-part egg.
    const wholeEntries = whole?.c.instantiateModelsToScene(name => name, false);
    for (const g of wholeEntries?.animationGroups ?? []) g.stop();
    const shakeGroup = wholeEntries?.animationGroups.find(g => g.name === 'Egg');
    shakeGroup?.start(true, 1); shakeGroup?.pause();
    const show = (entries, on) => entries?.rootNodes.forEach(n => n.setEnabled(on));
    // The newborn is created on the hatch's first frame (CreateChaoFromEgg), where the egg is.
    const row = chao && (glbExtras(chao.bytes)?.chaoBehaviour?.clips ?? []).find(r => r.nativeState === hatch.chaoState);
    const chaoEntries = row ? chao.c.instantiateModelsToScene(name => name, false) : null;
    for (const g of chaoEntries?.animationGroups ?? []) g.stop();
    const chaoGroup = chaoEntries?.animationGroups.find(g => g.name === row.nativeName);
    chaoGroup?.start(false, 1); chaoGroup?.pause();
    const { min, max } = eggEntries.rootNodes[0].getHierarchyBoundingVectors();
    const height = Math.max(max.y - min.y, 1);
    new B.ArcRotateCamera('camera', -Math.PI / 2, 1.25, height * 4.5, new B.Vector3(0, height / 2, 0), scene);
    let carry = 0;
    return await new Promise(resolve => {
      engine.runRenderLoop(() => {
        carry += engine.getDeltaTime() / (1000 / 60);
        for (; carry >= 1 && !hatch.done; carry--) hatch.step();
        const shaking = hatch.shaking && !!shakeGroup;
        show(wholeEntries, shaking); show(eggEntries, !shaking); show(chaoEntries, !hatch.shaking);
        if (shaking) seek(shakeGroup, hatch.shakeFrame % SHAKE_FRAMES, SHAKE_FRAMES);
        seek(eggGroup, hatch.eggFrame, hatch.frames); chaoGroup?.goToFrame(hatch.chaoFrame);
        scene.render();
        if (hatch.done) { engine.stopRenderLoop(); setTimeout(() => { finish(); resolve(true); }, 800); }
      });
    });
  } catch { return false; }
}
