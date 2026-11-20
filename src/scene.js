import {
  Engine, Scene, Vector3, Matrix, Quaternion, Color3, Color4, ArcRotateCamera,
  HemisphericLight, DirectionalLight, ShadowGenerator, StandardMaterial,
  MeshBuilder, Mesh, TransformNode, DynamicTexture, GlowLayer, LoadAssetContainerAsync, Ray, Texture,
  MaterialPluginBase,
} from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { NATIVE_GARDENS, isNativeTerrainMesh } from './world-layout.js';
import { attachNativeGardenExit, nativeGardenExitAction } from './garden-exits.js';
import { createShadowSkate, observeSkateMotion } from './shadow-skate.js';
import { attachNativeLocomotion, captureChaoPose, createNativeMotionState, createGaitControls, updateNativeLocomotion, smoothObservedSpeed, stepInputPace } from './chao-locomotion.js';
import { CHAO_GEAR_JOINTS, CHAO_SHOE_LENGTH_RATIO, CHAO_SHOE_REFERENCE_SIDE, attachRigidGearAtRest, chestSurfacePoint, collectNativeShoeParts, mirroredShadowShoeRest, shadowShoeMountRotation, updateNativeShoeEffects } from './chao-attachments.js';
import { restoreNativeChaosSampling } from './chao-materials.js';
import { createSteadyHead } from './chao-head.js';
import { attachChaoBody, bodyAssetFor } from './chao-body.js';
import { applyChaoLook, loadChaoLookManifest } from './chao-look.js';
import { configureChaoFace, loadChaoFaceManifest, resetChaoFace, stepChaoFace } from './chao-face.js';
import { createNativeChaoShadow, createNativeShadowTerrainIndex } from './chao-shadows.js';
import './chao-locomotion.css';

class NativeEmeraldEnvironment extends MaterialPluginBase {
  constructor(material) {
    super(material, 'NativeEmeraldEnvironment', 200, {}, true, true);
    this.doNotSerialize = true;
  }

  getCustomCode(shaderType) {
    if (shaderType !== 'fragment') return null;
    return {
      // SA2/SAIO environment maps use camera-space normals as UVs. Babylon's
      // generic spherical mode uses reflected view vectors, so restore the
      // original mapping while retaining PBR lighting and alpha blending.
      '!return computeSphericalCoords\\(worldPos,worldNormal,view,reflectionMatrix\\);': `
        vec3 nativeNormal = normalize(vec3(view * vec4(worldNormal, 0.0)));
        return vec3(nativeNormal.xy * 0.5 + 0.5, 0.0);
      `,
      CUSTOM_FRAGMENT_BEFORE_LIGHTS: `
        vec3 nativeEmeraldNormal = normalize(vec3(view * vec4(normalW, 0.0)));
        vec2 nativeEmeraldUV = nativeEmeraldNormal.xy * vec2(0.5, -0.5) + 0.5;
        surfaceAlbedo *= toLinearSpace(texture2D(reflectionSampler, nativeEmeraldUV).rgb);
      `,
    };
  }
}

// Native Chao, trees, and garden geometry load only through the local bridge
// from this machine's installed game resources, with a playable hub fallback.
export function createGarden(canvas, { onInput = () => {}, onAction = () => {}, onReady = () => {} } = {}) {
  const engine = new Engine(canvas, true, { stencil: true, preserveDrawingBuffer: false });
  engine.setHardwareScalingLevel(Math.min(1.6, Math.max(1, window.devicePixelRatio / 1.5)));
  const scene = new Scene(engine);
  scene.clearColor = Color4.FromHexString('#bddeedff');
  scene.fogMode = Scene.FOGMODE_EXP2;
  scene.fogDensity = 0.009;
  scene.fogColor = Color3.FromHexString('#bddeed');
  scene.ambientColor = new Color3(0.06, 0.07, 0.065);
  const camera = new ArcRotateCamera('garden-camera', -Math.PI / 2, 1.3, 2.4, new Vector3(0, 0.28, 6), scene);
  camera.lowerRadiusLimit = 1.5;
  camera.upperRadiusLimit = 12;
  camera.lowerBetaLimit = 0.45;
  camera.upperBetaLimit = 1.4;
  camera.wheelPrecision = 35;
  camera.panningSensibility = 0;
  camera.inertia = 0.8;
  camera.checkCollisions = true;
  camera.collisionRadius = new Vector3(0.12, 0.12, 0.12);
  camera.attachControl(canvas, true);
  camera.inputs.attached.keyboard?.detachControl();
  if (camera.inputs.attached.pointers) camera.inputs.attached.pointers.buttons = [2];
  // Babylon maps right drag to pan; remap it because garden panning is disabled.
  camera.movement.input.setInteractions('pointer', { button: 2 }, 'rotate');
  const hemi = new HemisphericLight('sky-light', new Vector3(0, 1, 0), scene);
  hemi.intensity = 0.6;
  hemi.groundColor = Color3.FromHexString('#496b68');
  const sun = new DirectionalLight('sun', new Vector3(-0.6, -1, 0.45), scene);
  sun.position = new Vector3(15, 35, -18);
  sun.intensity = 0.7;
  sun.diffuse = Color3.FromHexString('#fff3d3');
  const shadows = new ShadowGenerator(1024, sun);
  shadows.useBlurExponentialShadowMap = true;
  shadows.blurKernel = 24;
  shadows.setDarkness(0.22);
  const glow = new GlowLayer('garden-glow', scene, { blurKernelSize: 32 });
  glow.intensity = 0.35;
  // Only emeralds emit in this pass, but every opaque scene mesh must still
  // write depth. An inclusion-only list let hidden gems glow through Chao
  // bodies and made their solid skin appear transparent.
  const emeraldGlow = new GlowLayer('original-emerald-glow', scene, { blurKernelSize: 24, mainTextureRatio: 0.5 });
  emeraldGlow.intensity = 0.45;
  emeraldGlow.customEmissiveTextureSelector = () => null;
  emeraldGlow.customEmissiveColorSelector = (mesh, _subMesh, _material, result) => {
    const color = mesh.metadata?.nativeEmeraldGlow;
    result.copyFromFloats(color?.r || 0, color?.g || 0, color?.b || 0, color ? 0.32 : 0);
  };

  const materials = new Map();
  const animatedDecor = [];
  const nativeShots = [];
  const playerMeshes = new Map();
  const ringMeshes = new Map();
  const enemyMeshes = new Map();
  const assetContainers = new Map();
  const localTextures = new Map();
  const keys = new Set();
  const touch = { dx: 0, dz: 0 };
  let world = null;
  let scenery = null;
  let terrain = null;
  let worldVersion = 0;
  const floorCache = new Map();
  const groundAnchors = [];
  const nativeChaoShadows = new Set();
  let nativeShadowTerrain = null;
  let activeZone = '';
  let snapshot = null;
  let paused = false;
  let disposed = false;
  let time = 0;
  let inputTimer = 0;
  let inputPace = 0;
  let renderDelta = 0;
  const gaitControls = createGaitControls(canvas);
  let flameTexture = null;
  let flameMaterialPromise = null;
  let selfVehicleActive = false;
  let onFootCameraRadius = camera.radius;

  function material(color, emissive = 0, alpha = 1) {
    const key = `${color}:${emissive}:${alpha}`;
    if (materials.has(key)) return materials.get(key);
    const mat = new StandardMaterial(`mat-${key}`, scene);
    mat.diffuseColor = Color3.FromHexString(color);
    mat.specularColor = new Color3(0.09, 0.09, 0.09);
    mat.emissiveColor = mat.diffuseColor.scale(emissive);
    mat.alpha = alpha;
    materials.set(key, mat);
    return mat;
  }

  function sphere(name, position, scale, color, parent = scenery || world, segments = 12) {
    const mesh = MeshBuilder.CreateSphere(name, { diameter: 1, segments }, scene);
    mesh.position.copyFromFloats(...position);
    mesh.scaling.copyFromFloats(...scale);
    mesh.material = material(color);
    mesh.parent = parent;
    return mesh;
  }

  function box(name, position, scale, color, parent = scenery || world) {
    const mesh = MeshBuilder.CreateBox(name, { size: 1 }, scene);
    mesh.position.copyFromFloats(...position);
    mesh.scaling.copyFromFloats(...scale);
    mesh.material = material(color);
    mesh.parent = parent;
    mesh.receiveShadows = true;
    return mesh;
  }

  function cylinder(name, position, options, color, parent = scenery || world) {
    const mesh = MeshBuilder.CreateCylinder(name, { tessellation: 12, ...options }, scene);
    mesh.position.copyFromFloats(...position);
    mesh.material = material(color);
    mesh.parent = parent;
    mesh.receiveShadows = true;
    return mesh;
  }

  function label(text, position, color = '#eaf7f0', width = 3.5, parent = world, background = 'rgba(18,42,42,.84)') {
    const texture = new DynamicTexture(`label-${text}`, { width: 512, height: 96 }, scene, false);
    const ctx = texture.getContext();
    ctx.clearRect(0, 0, 512, 96);
    ctx.fillStyle = background;
    ctx.beginPath();
    ctx.roundRect(5, 12, 502, 72, 30);
    ctx.fill();
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.font = '600 29px system-ui, sans-serif';
    ctx.fillStyle = color;
    ctx.fillText(text.slice(0, 32), 256, 49, 470);
    texture.hasAlpha = true;
    texture.update();
    const mat = new StandardMaterial(`label-mat-${text}`, scene);
    mat.diffuseTexture = texture;
    mat.emissiveColor = Color3.White();
    mat.disableLighting = true;
    mat.useAlphaFromDiffuseTexture = true;
    mat.backFaceCulling = false;
    const plane = MeshBuilder.CreatePlane(`label-plane-${text}`, { width, height: width * 96 / 512 }, scene);
    plane.position.copyFromFloats(...position);
    plane.material = mat;
    plane.billboardMode = Mesh.BILLBOARDMODE_ALL;
    plane.isPickable = false;
    plane.parent = parent;
    plane.metadata = { labelMaterial: mat, labelTexture: texture };
    return plane;
  }

  function destroyGroup(root) {
    for (const mesh of root.getChildMeshes()) {
      if (mesh.metadata?.labelTexture) {
        mesh.metadata.labelTexture.dispose();
        mesh.metadata.labelMaterial.dispose();
      }
    }
    // Entries own cloned skeletons and animation groups. Mesh disposal alone
    // leaves those alive; materials remain shared with the source container.
    const descendants = [root, ...root.getDescendants()];
    for (const node of descendants) if (node.metadata?.nativeContactShadow) {
      nativeChaoShadows.delete(node.metadata.nativeContactShadow);
      node.metadata.nativeContactShadow.dispose();
    }
    const entries = new Set(descendants.map(node => node.metadata?.importedEntries).filter(Boolean));
    root.metadata?.steadyHead?.dispose();
    root.metadata?.chaoBodyDispose?.();
    root.metadata?.shadowSkate?.dispose();
    for (const imported of entries) imported.dispose();
    for (const node of descendants) for (const mat of node.metadata?.ownedMaterials || []) mat.dispose(false, false);
    root.dispose();
  }

  function assetFor(data) {
    if (data.asset) return data.asset;
    const form = String(data.form || 'normal').toLowerCase();
    if (data.admin || form.includes('chaos')) {
      if (form.includes('hero') || data.alignment >= 60) return 'hero-chaos';
      if (form.includes('dark') || data.alignment <= -60) return 'dark-chaos';
      return 'neutral-chaos';
    }
    // Server-owned native body family (child or adult morphs); falls back to
    // the original child model when the family has not been converted locally.
    const body = bodyAssetFor(data);
    if (body) return body;
    return form === 'normal' || form === 'neutral' ? 'chao' : null;
  }

  function vehicleFor(data) {
    return data.admin && data.vehicle === 'dark-chao-walker' ? data.vehicle : null;
  }

  function gearFor(data) {
    return {
      footwear: data.admin && data.gear?.footwear === 'shadow-air-shoes' ? 'shadow-air-shoes' : null,
      hands: data.admin && data.gear?.hands === 'knuckles-hammer-gloves' ? 'knuckles-hammer-gloves' : null,
    };
  }

  function appearanceFor(data) {
    return `${String(data.form || 'normal').toLowerCase()}:${!!data.admin}:${assetFor(data)}:${vehicleFor(data) || ''}:${data.body?.look?.color ?? 0}`;
  }

  function loadLocalAsset(key) {
    if (assetContainers.has(key)) return assetContainers.get(key);
    const task = (async () => {
      try {
        // A missing import is normal during setup. Fetch first so a 404 does
        // not invoke the glTF parser or break the procedural fallback.
        const response = await fetch(`/api/assets/${key}.glb`, { cache: 'no-store' });
        if (!response.ok || disposed) return null;
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (bytes.byteLength < 12 || new DataView(bytes.buffer).getUint32(0, true) !== 0x46546c67 || disposed) return null;
        const container = await LoadAssetContainerAsync(bytes, scene, { pluginExtension: '.glb', name: `${key}.glb` });
        if (disposed) { container.dispose(); return null; }
        for (const group of container.animationGroups) group.stop();
        const faceMaterials = {
          chao: ['al_ncz.sa2mdl_3', 'al_ncz.sa2mdl_4', 'al_ncz.sa2mdl_5'],
          'neutral-chaos': ['al_nchaos.sa2mdl_10', 'al_nchaos.sa2mdl_11', 'al_nchaos.sa2mdl_12'],
          'hero-chaos': ['al_hchaos.sa2mdl_4', 'al_hchaos.sa2mdl_6', 'al_hchaos.sa2mdl_7', 'al_hchaos.sa2mdl_8'],
          'dark-chaos': ['al_dchaos.sa2mdl_4', 'al_dchaos.sa2mdl_7', 'al_dchaos.sa2mdl_8', 'al_dchaos.sa2mdl_9'],
          tails: ['al_tails.sa2mdl_2', 'al_tails.sa2mdl_4', 'al_tails.sa2mdl_5', 'al_tails.sa2mdl_6'],
          knuckles: ['al_knuckles.sa2mdl_3', 'al_knuckles.sa2mdl_5', 'al_knuckles.sa2mdl_6', 'al_knuckles.sa2mdl_7'],
          amy: ['al_amy.sa2mdl_2', 'al_amy.sa2mdl_4', 'al_amy.sa2mdl_5', 'al_amy.sa2mdl_6'],
        };
        const isFamilyBody = key.startsWith('chao-body-');
        const isCharacter = isFamilyBody || Object.hasOwn(faceMaterials, key);
        const isCinematicEmerald = /^emerald-(red|blue|green|cyan|purple|yellow|white)$/.test(key);
        const isNativeWalker = key === 'dark-chao-walker';
        const isNativeGear = /^(shadow-air-shoes|knuckles-hammer-gloves)-(left|right)$/.test(key);
        const isNativeWalkerEffect = key === 'dark-walker-shot' || key === 'dark-walker-booster';
        for (const mat of container.materials) {
          if ('albedoColor' in mat) {
            if (isNativeWalkerEffect) {
              const source = mat.metadata?.gltf?.extras?.sa2Source;
              mat.unlit = !!source?.ignoreLighting;
              mat.metallic = 0;
              mat.roughness = 1;
              if (source?.destinationAlpha === 'ONE') {
                mat.alphaMode = Engine.ALPHA_ADD;
                mat.transparencyMode = 2;
                mat.disableDepthWrite = true;
              }
              continue;
            }
            if (isNativeWalker || isNativeGear) {
              const source = mat.metadata?.gltf?.extras?.sa2Source;
              mat.metallic = 0;
              mat.unlit = !!source?.ignoreLighting;
              // Native hull overlays retain source transparency; the solid
              // hull and feet stay opaque. These are separate original parts.
              if (source?.environment && mat.albedoTexture) {
                mat.reflectionTexture = mat.albedoTexture;
                mat.reflectionTexture.coordinatesMode = Texture.SPHERICAL_MODE;
                mat.albedoTexture = null;
                new NativeEmeraldEnvironment(mat);
              }
              continue;
            }
            if (isCinematicEmerald) {
              // These original models have zero UVs: the native shader maps
              // the embedded image by surface reflection, rather than UVs.
              // Preserve their original base color, alpha, and blend mode.
              const nativeMap = mat.albedoTexture;
              if (nativeMap) {
                nativeMap.coordinatesMode = Texture.SPHERICAL_MODE;
                mat.reflectionTexture = nativeMap;
                mat.albedoTexture = null;
              }
              mat.unlit = false;
              mat.metallic = 0.3;
              mat.roughness = 0.12;
              mat.directIntensity = 0.35;
              mat.environmentIntensity = 1.3;
              if (nativeMap) new NativeEmeraldEnvironment(mat);
              continue;
            }
            mat.albedoColor = Color3.White();
            mat.metallic = 0;
            mat.roughness = 1;
            // Body and wing PNG alpha belongs to the native Chao shader, not
            // browser cutout geometry. Only actual facial overlays may mask.
            mat.unlit = true;
            mat.alpha = 1;
            // Family bodies keep the original eye and mouth overlays; only those mask.
            const facePart = isFamilyBody ? /al_eye|al_mouth/i.test(mat.albedoTexture?.name || '') : faceMaterials[key]?.includes(mat.name);
            const solidCharacterPart = isCharacter && !facePart;
            mat.transparencyMode = solidCharacterPart ? 0 : mat.transparencyMode === 2 || mat.transparencyMode === 3 ? 1 : mat.transparencyMode ?? 0;
            mat.alphaCutOff = 0.45;
            mat.useAlphaFromAlbedoTexture = mat.transparencyMode === 1;
            if (solidCharacterPart) {
              mat.opacityTexture = null;
              mat.alphaMode = Engine.ALPHA_DISABLE;
              mat.disableDepthWrite = false;
              mat.backFaceCulling = false;
              mat.twoSidedLighting = true;
            }
            restoreNativeChaosSampling(key, mat);
          }
        }
        return container;
      } catch {
        return null;
      }
    })();
    assetContainers.set(key, task);
    return task;
  }

  function loadLocalTexture(filename, invertY = true) {
    const key = `${filename}:${invertY}`;
    if (localTextures.has(key)) return localTextures.get(key);
    const task = (async () => {
      try {
        const response = await fetch(`/api/assets/${filename}`, { cache: 'no-store' });
        if (!response.ok || disposed) return null;
        const url = URL.createObjectURL(await response.blob());
        const texture = await new Promise((resolve, reject) => {
          const loaded = new Texture(url, scene, false, invertY, Texture.BILINEAR_SAMPLINGMODE,
            () => resolve(loaded), (_message, error) => reject(error || new Error('Local texture unavailable.')));
        }).finally(() => URL.revokeObjectURL(url));
        if (disposed) { texture.dispose(); return null; }
        texture.name = filename;
        return texture;
      } catch { return null; }
    })();
    localTextures.set(key, task);
    return task;
  }

  async function applyOriginalGrass(ground) {
    const texture = await loadLocalTexture('hub-grass.png');
    if (!texture || disposed || ground.isDisposed()) return;
    texture.uScale = 24;
    texture.vScale = 24;
    texture.wrapU = Texture.WRAP_ADDRESSMODE;
    texture.wrapV = Texture.WRAP_ADDRESSMODE;
    const mat = new StandardMaterial('original-game-hub-grass', scene);
    mat.diffuseTexture = texture;
    mat.diffuseColor = Color3.White();
    mat.specularColor = Color3.Black();
    ground.material = mat;
    ground.metadata = { ...ground.metadata, ownedMaterials: [mat] };
    canvas.dataset.hubGrass = 'native';
  }

  async function attachLocalAsset(item, key) {
    if (!key) return;
    let container = await loadLocalAsset(key);
    if (!container && key.startsWith('chao-body-')) {
      key = 'chao';
      container = await loadLocalAsset(key);
    }
    if (!container || disposed || item.root.isDisposed()) return;
    const entries = container.instantiateModelsToScene(name => `${item.root.name}-${name}`, false, { doNotInstantiate: true });
    const visual = new TransformNode(`${item.root.name}-local-model`, scene);
    visual.parent = item.root;
    // Native models have different heights. Keep a consistent playable scale
    // while preserving each model's original proportions and skeleton.
    const scale = 0.1;
    visual.scaling.setAll(scale);
    for (const root of entries.rootNodes) root.parent = visual;
    for (const group of entries.animationGroups) group.stop();
    let minY = Infinity;
    let maxY = -Infinity;
    for (const mesh of visual.getChildMeshes()) {
      mesh.computeWorldMatrix(true);
      const bounds = mesh.getBoundingInfo().boundingBox;
      minY = Math.min(minY, (bounds.minimumWorld.y - item.root.position.y) / item.root.scaling.y);
      maxY = Math.max(maxY, (bounds.maximumWorld.y - item.root.position.y) / item.root.scaling.y);
      mesh.isPickable = !!item.npc;
      if (item.npc && item.action) mesh.metadata = { ...mesh.metadata, action: item.action };
      mesh.receiveShadows = true;
      mesh.visibility = 1;
      mesh.hasVertexAlpha = false;
      shadows.addShadowCaster(mesh, false);
    }
    if (Number.isFinite(minY)) visual.position.y = -minY + 0.015;
    const height = Number.isFinite(maxY - minY) ? maxY - minY : 2.2;
    for (const mesh of item.proxyMeshes) mesh.setEnabled(false);
    // An imported Chao uses native appearance only. Its emotion icon is a
    // separate original-game asset and must not be replaced by an invented halo.
    item.emotion.setEnabled(false);
    item.emotionBase = height + 0.08;
    item.nameplate.position.y = height + 0.55;
    item.root.metadata = { ...item.root.metadata, importedEntries: entries };
    item.imported = {
      entries, visual, meshes: visual.getChildMeshes(), height,
      idle: entries.animationGroups.find(group => /idle/i.test(group.name)),
      walk: entries.animationGroups.find(group => /walk/i.test(group.name)),
      current: null,
      motionState: createNativeMotionState(),
    };
    item.imported.gearSockets = captureGearSockets(item);
    const locomotionPose = captureChaoPose(item.imported);
    void loadLocalAsset('chao-locomotion').then(source => {
      if (source && !disposed && !item.root.isDisposed()) {
        attachNativeLocomotion(item.imported, source, locomotionPose);
        canvas.dataset.nativeRun = item.imported.run ? 'native119-loaded' : 'unavailable';
        canvas.dataset.nativeAir = item.imported.jumpUp && item.imported.jumpDown ? 'native132-137-loaded' : 'unavailable';
      }
    });
    item.imported.skate = createShadowSkate(item.imported);
    item.root.metadata.shadowSkate = item.imported.skate;
    item.imported.steadyHead = createSteadyHead(item.imported);
    item.root.metadata.steadyHead = item.imported.steadyHead;
    if (item.accessory) item.accessory.position.y = height * 0.78;
    canvas.dataset.localAssets = [...assetContainers.keys()].join(',');
    canvas.dataset.chaoAsset = 'loaded';
    if (key.startsWith('chao-body-')) {
      // Morph weights and node offsets follow the server body each frame.
      item.root.metadata.chaoBodyDispose = attachChaoBody(visual, entries.animationGroups, () => item.bodyState);
      await loadChaoFaceManifest();
      if (!item.root.isDisposed()) {
        await loadChaoLookManifest();
        applyChaoLook(visual, item.bodyState?.look);
        configureChaoFace(visual, item.bodyState?.typeName);
        resetChaoFace(visual);
        item.faceRoot = visual;
        // Native lid/blink logic runs per 60 Hz frame.
        let frames = 0;
        const faceObserver = scene.onBeforeRenderObservable.add(() => {
          frames += Math.min(scene.getEngine().getDeltaTime(), 100) * 0.06;
          for (; frames >= 1; frames -= 1) stepChaoFace(visual);
        });
        const disposeBody = item.root.metadata.chaoBodyDispose;
        item.root.metadata.chaoBodyDispose = () => { disposeBody?.(); scene.onBeforeRenderObservable.remove(faceObserver); };
      }
    }
    if (key === 'dark-chaos') attachNativeFlame(item);
    if (item.admin) { attachNativeEmeralds(item); attachNativeChestEmerald(item); }
    if (item.vehicleKey) attachNativeWalker(item);
    setOwnerGear(item, item.gear, true);
    canvas.dispatchEvent(new CustomEvent('chao-assets', { detail: { key, loaded: true } }));
  }

  async function attachNativeWalker(item) {
    const container = await loadLocalAsset('dark-chao-walker');
    if (!container || disposed || item.root.isDisposed() || !item.imported) return;
    const root = new TransformNode(`${item.root.name}-dark-walker`, scene);
    const visual = new TransformNode(`${item.root.name}-dark-walker-model`, scene);
    visual.parent = root;
    // SA2 character and garden units all use the same .1 browser scale.
    // The glTF loader's LH conversion mirrors the native -X forward axis.
    // Rotate that imported +X axis toward the existing Chao's +Z heading.
    visual.scaling.setAll(0.1);
    visual.rotation.y = -Math.PI / 2;
    const entries = container.instantiateModelsToScene(name => `${item.root.name}-walker-${name}`, false, { doNotInstantiate: true });
    for (const entryRoot of entries.rootNodes) entryRoot.parent = visual;
    for (const group of entries.animationGroups) {
      group.stop(); group.enableBlending = true; group.blendingSpeed = 0.16;
    }
    const motion = name => entries.animationGroups.find(group => group.name.endsWith(name));
    const idle = motion('Idle');
    // Native motion89 is a constant standing pose. glTF may omit its
    // duplicate tracks; preserve the exported source pose for stopping.
    const standPose = [...new Set(entries.animationGroups.flatMap(group => group.targetedAnimations.map(entry => entry.target)))].map(node => ({
      node, position: node.position.clone(), rotation: node.rotation.clone(),
      quaternion: node.rotationQuaternion?.clone(), scaling: node.scaling.clone(),
    }));
    idle?.start(true, 1);
    idle?.goToFrame(0);
    let minimum = new Vector3(Infinity, Infinity, Infinity);
    let maximum = new Vector3(-Infinity, -Infinity, -Infinity);
    const meshes = visual.getChildMeshes().filter(mesh => mesh.getTotalVertices());
    for (const mesh of meshes) {
      mesh.computeWorldMatrix(true);
      const bounds = mesh.getBoundingInfo().boundingBox;
      minimum = Vector3.Minimize(minimum, bounds.minimumWorld);
      maximum = Vector3.Maximize(maximum, bounds.maximumWorld);
      mesh.isPickable = false;
      mesh.receiveShadows = true;
      shadows.addShadowCaster(mesh, false);
    }
    const pilotSeat = visual.getDescendants().find(node => node.metadata?.gltf?.extras?.chaoPilotSeat || node.name.endsWith('ChaoPilotSeat'));
    if (!pilotSeat || !Number.isFinite(minimum.y) || maximum.y <= minimum.y) {
      entries.dispose(); root.dispose(); return;
    }
    visual.position.y = -minimum.y;
    root.parent = item.root;
    root.metadata = { importedEntries: entries };
    const size = maximum.subtract(minimum);
    item.vehicle = {
      root, visual, entries, meshes, pilotSeat, height: size.y, size, motion, standPose,
      radius: Math.max(Math.abs(minimum.x), Math.abs(maximum.x), Math.abs(minimum.z), Math.abs(maximum.z)),
      idle, walk: motion('Walk'), current: idle,
    };
    for (const emerald of item.emeralds || []) emerald.root.parent = root;
    item.nameplate.position.y = size.y + 0.55;
    groundImportedFeet(item);
    attachNativeBooster(item);
    void loadLocalAsset('dark-walker-shot');
    canvas.dataset.walkerAsset = 'loaded';
    canvas.dispatchEvent(new CustomEvent('chao-assets', { detail: { key: 'dark-chao-walker', loaded: true } }));
  }

  function nativeEffect(container, name, parent = null) {
    const root = new TransformNode(name, scene);
    root.parent = parent;
    const entries = container.instantiateModelsToScene(nodeName => `${name}-${nodeName}`, false, { doNotInstantiate: true });
    for (const entryRoot of entries.rootNodes) entryRoot.parent = root;
    root.metadata = { importedEntries: entries };
    for (const mesh of root.getChildMeshes()) {
      mesh.isPickable = false;
      glow.addExcludedMesh(mesh);
    }
    return root;
  }

  function geometryPoints(meshes, toLocal = Matrix.Identity()) {
    const points = [];
    for (const mesh of meshes) {
      const positions = mesh.getVerticesData('position');
      if (!positions) continue;
      mesh.computeWorldMatrix(true);
      const transform = mesh.getWorldMatrix().multiply(toLocal);
      for (let i = 0; i < positions.length; i += 3) points.push(Vector3.TransformCoordinates(Vector3.FromArray(positions, i), transform));
    }
    return points;
  }

  function boundsFor(points) {
    if (!points.length) return null;
    let min = new Vector3(Infinity, Infinity, Infinity), max = new Vector3(-Infinity, -Infinity, -Infinity);
    for (const point of points) { min = Vector3.Minimize(min, point); max = Vector3.Maximize(max, point); }
    return { min, max, size: max.subtract(min), center: min.add(max).scale(0.5) };
  }

  function captureGearSockets(item) {
    // Capture the original rest pose once, before native Idle/Walk starts.
    // Async gear loading must not bake whichever animated pose happens to
    // be active when its asset finishes loading into the attachment offset.
    const nodes = item.imported.visual.getDescendants();
    const find = index => nodes.find(node => new RegExp(`(?:^|-)${index}_object_`).test(node.name));
    item.root.computeWorldMatrix(true);
    const toRoot = Matrix.Invert(item.root.getWorldMatrix());
    const result = { footwear: [], hands: [] };
    for (const [side, armIndex, handIndex, footIndex, footMeshIndex] of CHAO_GEAR_JOINTS) {
      for (const [slot, socket, meshNode] of [
        // The visible palm follows the arm frame, while its distal marker
        // twists independently in the native skating overlay.
        ['footwear', find(footIndex), find(footMeshIndex)], ['hands', find(armIndex), find(armIndex)],
      ]) {
        if (!socket || !meshNode) continue;
        socket.computeWorldMatrix(true); meshNode.computeWorldMatrix(true);
        const meshes = meshNode.getChildMeshes().filter(mesh => mesh.getTotalVertices());
        let points = geometryPoints(meshes, toRoot);
        if (slot === 'hands' && points.length) {
          const distalMarker = find(handIndex);
          if (!distalMarker) continue;
          distalMarker.computeWorldMatrix(true);
          const shoulder = Vector3.TransformCoordinates(meshNode.getAbsolutePosition(), toRoot);
          const hand = Vector3.TransformCoordinates(distalMarker.getAbsolutePosition(), toRoot);
          const axis = hand.subtract(shoulder).normalize();
          const reach = Math.max(...points.map(point => Vector3.Dot(point.subtract(shoulder), axis)));
          // Match the native distal palm, keeping the original forearm and
          // hand geometry. The glove is a rigid shell around that hand.
          points = points.filter(point => Vector3.Dot(point.subtract(shoulder), axis) >= reach * 0.55);
        }
        const bounds = boundsFor(points);
        if (bounds) result[slot].push({ side, socket, meshes, bounds, restToRoot: socket.getWorldMatrix().multiply(toRoot) });
      }
    }
    const torso = find('001');
    if (torso) {
      torso.computeWorldMatrix(true);
      const bodyMeshes = torso.getChildren().filter(node => /(?:^|-)attach_/.test(node.name))
        .flatMap(node => [node, ...node.getChildMeshes()]).filter(mesh => mesh.getTotalVertices?.());
      const bounds = boundsFor(geometryPoints(bodyMeshes, toRoot));
      if (bounds) result.torso = { socket: torso, bounds, surface: chestSurfacePoint(bodyMeshes, toRoot, bounds), restToRoot: torso.getWorldMatrix().multiply(toRoot) };
    }
    return result;
  }

  function gearMountRotation(itemId, side) {
    if (itemId === 'shadow-air-shoes') return shadowShoeMountRotation();
    const references = {
      // Original Knuckles142 wrist/hand rest matrices041/042 and060/061
      // provide the glove's source orientation. Fitting that rigid shell
      // around a Chao palm is a deliberate attachment adaptation.
      'knuckles-hammer-gloves': {
        left: { up: [0.1710015, 0.8968497, -0.4079447], forward: [-0.9851516, 0.1620746, -0.0566396] },
        right: { up: [0.1505539, 0.9335916, 0.3251772], forward: [-0.9840459, 0.1730633, -0.0412652] },
      },
    };
    const reference = references[itemId]?.[side];
    if (!reference) return Quaternion.Identity();
    const up = Vector3.FromArray(reference.up), forward = Vector3.FromArray(reference.forward);
    up.x *= -1; forward.x *= -1;
    up.normalize(); forward.normalize();
    const basis = Matrix.Identity();
    Matrix.FromXYZAxesToRef(Vector3.Cross(up, forward).normalize(), up, forward, basis);
    return Quaternion.FromRotationMatrix(basis).conjugate().normalize();
  }

  async function attachOwnerGear(item, slot, itemId, version) {
    const imported = item.imported;
    const sockets = imported?.gearSockets[slot];
    if (!sockets || sockets.length !== 2) return;
    const mirroredShoes = slot === 'footwear' && itemId === 'shadow-air-shoes';
    const containers = await Promise.all(sockets.map(({ side }) => loadLocalAsset(`${itemId}-${mirroredShoes ? CHAO_SHOE_REFERENCE_SIDE : side}`)));
    if (containers.some(container => !container) || disposed || item.root.isDisposed() || item.imported !== imported || version !== item.gearVersion) return;
    const attachments = [];
    for (let i = 0; i < sockets.length; i++) {
      const fit = sockets[i];
      const root = nativeEffect(containers[i], `${item.root.name}-${itemId}-${fit.side}`);
      root.rotationQuaternion = gearMountRotation(itemId, fit.side);
      const { meshes, effects } = collectNativeShoeParts(root);
      const source = boundsFor(geometryPoints(meshes));
      if (!source) { destroyGroup(root); continue; }
      const scale = slot === 'footwear' ? fit.bounds.size.z * CHAO_SHOE_LENGTH_RATIO / source.size.z
        : Math.max(fit.bounds.size.x, fit.bounds.size.y, fit.bounds.size.z) * 1.12 / Math.max(source.size.x, source.size.y, source.size.z);
      const position = fit.bounds.center.subtract(source.center.scale(scale));
      if (slot === 'footwear') position.y = fit.bounds.min.y - source.min.y * scale;
      const desired = Matrix.Compose(new Vector3(scale, scale, scale), root.rotationQuaternion, position);
      if (!mirroredShoes) attachRigidGearAtRest(root, fit, desired);
      for (const mesh of meshes) { mesh.receiveShadows = true; shadows.addShadowCaster(mesh, false); }
      attachments.push({ root, meshes, fit, source, nativeEffects: effects });
    }
    if (attachments.length !== 2) { for (const entry of attachments) destroyGroup(entry.root); return; }
    if (mirroredShoes) {
      const reference = attachments.find(entry => entry.fit.side === CHAO_SHOE_REFERENCE_SIDE);
      const rest = mirroredShadowShoeRest(reference.source, reference.fit.bounds);
      for (const entry of attachments) {
        attachRigidGearAtRest(entry.root, entry.fit, rest[entry.fit.side]);
        entry.root.metadata.nativeShoeAttachment = { sourceSide: CHAO_SHOE_REFERENCE_SIDE, mirrored: entry.fit.side !== CHAO_SHOE_REFERENCE_SIDE };
      }
      canvas.dataset.footwearSymmetry = 'native-right-mirrored-at-rest';
    }
    item.ownerGear[slot] = attachments;
    if (slot === 'footwear') {
      for (const { fit } of attachments) for (const mesh of fit.meshes) mesh.setEnabled(false);
      imported.footwearMeshes = attachments.flatMap(entry => entry.meshes);
    }
    canvas.dataset.ownerGear = Object.entries(item.ownerGear).filter(([, entries]) => entries.length).map(([key]) => key).join(',');
  }

  function setOwnerGear(item, next, force = false) {
    const gear = gearFor({ admin: item.admin, gear: next });
    const key = `${gear.footwear || ''}:${gear.hands || ''}`;
    item.gear = gear;
    if (!force && key === item.gearKey) return;
    item.gearKey = key;
    item.gearVersion = (item.gearVersion || 0) + 1;
    for (const attachments of Object.values(item.ownerGear || {})) for (const entry of attachments) {
      for (const mesh of entry.fit.meshes) mesh.setEnabled(true);
      destroyGroup(entry.root);
    }
    item.ownerGear = { footwear: [], hands: [] };
    if (item.imported) item.imported.footwearMeshes = [];
    for (const slot of ['footwear', 'hands']) if (gear[slot] && item.imported) void attachOwnerGear(item, slot, gear[slot], item.gearVersion);
  }

  async function attachNativeBooster(item) {
    const container = await loadLocalAsset('dark-walker-booster');
    if (!container || disposed || item.root.isDisposed() || !item.vehicle) return;
    // Model283 retains the source exhaust placement. Sharing the vehicle's
    // uniformly scaled/rotated model parent preserves those original units.
    const booster = nativeEffect(container, `${item.root.name}-original-booster`, item.vehicle.visual);
    const chest = item.vehicle.visual.getDescendants().find(node => /010_object_/.test(node.name));
    if (chest) booster.setParent(chest);
    booster.setEnabled(!!item.mech?.boosting);
    item.vehicle.booster = booster;
  }

  async function showNativeShot(message) {
    if (message.zone !== activeZone || ![message.x, message.y, message.z, message.hitX, message.hitY, message.hitZ].every(Number.isFinite)) return;
    const version = worldVersion;
    const container = await loadLocalAsset('dark-walker-shot');
    if (!container || disposed || version !== worldVersion) return;
    const start = new Vector3(message.x, message.y, message.z);
    const end = new Vector3(message.hitX, message.hitY, message.hitZ);
    const direction = end.subtract(start);
    if (direction.lengthSquared() < 0.0001) return;
    const root = nativeEffect(container, 'original-dark-walker-cannon-shot');
    root.scaling.setAll(0.1);
    root.position.copyFrom(start);
    root.rotation.y = Math.atan2(direction.x, direction.z) - Math.PI / 2;
    nativeShots.push({ root, start, end, age: 0, duration: Math.max(0.06, Math.min(0.3, direction.length() / 30)) });
  }

  function handleEvent(message) {
    if (message?.type === 'event' && message.kind === 'mech-shot') void showNativeShot(message);
  }

  function mountedCameraRadius(vehicle) {
    const aspect = Math.max(0.4, engine.getAspectRatio(camera));
    const halfVertical = camera.fov / 2;
    const halfHorizontal = Math.atan(Math.tan(halfVertical) * aspect);
    const diameter = 2 * (vehicle.radius + 0.4) + 0.6;
    return Math.max((vehicle.height + 0.75) / (2 * Math.tan(halfVertical)), diameter / (2 * Math.tan(halfHorizontal))) * 1.12 + diameter / 2;
  }

  function animateWalker(item, dt) {
    const vehicle = item.vehicle;
    const state = item.mech || { boosting: false, grounded: true };
    if (state.boosting && !vehicle.wasBoosting) vehicle.takeoffUntil = time + 0.12;
    if (state.grounded && vehicle.wasGrounded === false) vehicle.landingUntil = time + 0.12;
    let name = 'Idle', loop = true, rate = 1;
    if (state.boosting) {
      name = time < vehicle.takeoffUntil ? 'Jump' : 'Hover';
      loop = name === 'Hover';
    } else if (!state.grounded) { name = 'Fall'; loop = false; }
    else if (time < (vehicle.landingUntil || 0)) { name = 'Land'; loop = false; }
    else if (item.moving) {
      const speed = item.groundSpeed || 0;
      name = speed > 1.65 ? 'Run' : speed > 1.2 ? 'Jog' : 'Walk';
      const reference = name === 'Run' ? 1.9 : name === 'Jog' ? 1.5 : 1;
      rate = Math.max(0.4, Math.min(1.5, speed / reference));
    }
    const animation = vehicle.motion(name) || vehicle.idle;
    if (name === 'Idle' && !animation && vehicle.current) {
      vehicle.current.stop();
      vehicle.current = null;
      for (const pose of vehicle.standPose) {
        pose.node.position.copyFrom(pose.position);
        pose.node.rotation.copyFrom(pose.rotation);
        if (pose.quaternion && pose.node.rotationQuaternion) pose.node.rotationQuaternion.copyFrom(pose.quaternion);
        pose.node.scaling.copyFrom(pose.scaling);
      }
    }
    if (animation && vehicle.current !== animation) {
      vehicle.current?.stop();
      animation.start(loop, rate);
      vehicle.current = animation;
    }
    if (animation) animation.speedRatio = rate;
    vehicle.booster?.setEnabled(!!state.boosting);
    vehicle.wasBoosting = !!state.boosting;
    vehicle.wasGrounded = !!state.grounded;
  }

  function nativeFlameMaterial() {
    if (!flameMaterialPromise) flameMaterialPromise = (async () => {
      try {
        const response = await fetch('/api/assets/chao-flame.png', { cache: 'no-store' });
        if (!response.ok || disposed) return null;
        const objectUrl = URL.createObjectURL(await response.blob());
        const texture = await new Promise((resolve, reject) => {
          const loaded = new Texture(objectUrl, scene, false, false, Texture.NEAREST_SAMPLINGMODE,
            () => resolve(loaded), (_message, error) => reject(error || new Error('Native flame texture unavailable.')));
        }).finally(() => URL.revokeObjectURL(objectUrl));
        if (disposed) { texture.dispose(); return null; }
        texture.hasAlpha = true;
        texture.uScale = 1 / 16;
        texture.vScale = -1;
        texture.vOffset = 1;
        texture.wrapU = Texture.CLAMP_ADDRESSMODE;
        texture.wrapV = Texture.CLAMP_ADDRESSMODE;
        flameTexture = texture;
        const mat = new StandardMaterial('original-dark-chaos-emotion', scene);
        mat.diffuseTexture = texture;
        mat.disableLighting = true;
        mat.emissiveColor = Color3.White();
        mat.useAlphaFromDiffuseTexture = true;
        mat.backFaceCulling = false;
        // SA2's flame atlas uses black as the additive blend background.
        // Adding black contributes zero, matching the original sprite pipeline.
        mat.alphaMode = Engine.ALPHA_ADD;
        mat.transparencyMode = 2;
        mat.disableDepthWrite = true;
        return mat;
      } catch { return null; }
    })();
    return flameMaterialPromise;
  }

  async function attachNativeFlame(item) {
    const imported = item.imported;
    const mat = await nativeFlameMaterial();
    if (!mat || disposed || item.root.isDisposed() || item.imported !== imported) return;
    const sprite = MeshBuilder.CreatePlane('original-dark-chaos-flame', { size: 0.18 }, scene);
    sprite.material = mat;
    sprite.parent = item.root;
    sprite.position.y = item.imported.height + 0.13;
    sprite.billboardMode = Mesh.BILLBOARDMODE_ALL;
    sprite.isPickable = false;
    glow.addExcludedMesh(sprite);
    item.nativeEmotion = sprite;
  }

  async function attachNativeEmeralds(item) {
    const colors = ['red', 'blue', 'green', 'cyan', 'purple', 'yellow', 'white'];
    const containers = await Promise.all(colors.map(color => loadLocalAsset(`emerald-${color}`)));
    if (disposed || item.root.isDisposed()) return;
    item.emeralds = [];
    for (let i = 0; i < colors.length; i++) {
      const container = containers[i];
      if (!container) continue;
      const orbit = new TransformNode(`native-chaos-emerald-${colors[i]}`, scene);
      const visual = new TransformNode(`native-chaos-emerald-${colors[i]}-visual`, scene);
      visual.parent = orbit;
      const entries = container.instantiateModelsToScene(name => `${item.root.name}-emerald-${colors[i]}-${name}`, false, { doNotInstantiate: true });
      for (const root of entries.rootNodes) root.parent = visual;
      let minimum = new Vector3(Infinity, Infinity, Infinity);
      let maximum = new Vector3(-Infinity, -Infinity, -Infinity);
      for (const mesh of visual.getChildMeshes()) {
        if (!mesh.getTotalVertices()) continue;
        mesh.computeWorldMatrix(true);
        const bounds = mesh.getBoundingInfo().boundingBox;
        minimum = Vector3.Minimize(minimum, bounds.minimumWorld);
        maximum = Vector3.Maximize(maximum, bounds.maximumWorld);
        mesh.isPickable = false;
        glow.addExcludedMesh(mesh);
        addNativeEmeraldGlow(mesh, colors[i]);
      }
      const height = maximum.y - minimum.y;
      if (!Number.isFinite(height) || height <= 0) {
        entries.dispose();
        orbit.dispose();
        continue;
      }
      // Cutscene files retain scene-space positions far from their origin.
      // Recenter before applying a consistent small, original-proportion size.
      const scale = 0.09 / height;
      visual.scaling.setAll(scale);
      visual.position.copyFrom(minimum.add(maximum).scale(-0.5 * scale));
      orbit.parent = item.vehicle?.root || item.root;
      orbit.metadata = { importedEntries: entries, nativeColor: colors[i] };
      item.emeralds.push({ root: orbit, phase: i * Math.PI * 2 / colors.length });
    }
    canvas.dataset.emeraldTextures = String(item.emeralds.length);
    canvas.dataset.emeraldModels = 'cinematic';
  }

  function addNativeEmeraldGlow(mesh, color) {
    // Tint only the bloom pass; no texture recolors or extra aura geometry.
    const sourceColor = { red: '#ed3638', blue: '#343df1', green: '#2be92d', cyan: '#39dce9', purple: '#b444ed', yellow: '#f6e929', white: '#eaf2ff' }[color] || '#ffffff';
    mesh.metadata = { ...mesh.metadata, nativeEmeraldGlow: Color3.FromHexString(sourceColor).scale(0.48) };
    canvas.dataset.emeraldGlow = 'native-depth-occluded-bloom';
  }

  async function attachNativeChestEmerald(item) {
    const imported = item.imported;
    const fit = imported?.gearSockets.torso;
    if (!fit) return;
    const container = await loadLocalAsset('emerald-cyan');
    if (!container || disposed || item.root.isDisposed() || item.imported !== imported) return;
    const chest = new TransformNode(`${item.root.name}-native-chest-emerald`, scene);
    const visual = nativeEffect(container, `${item.root.name}-chest-cyan-model`, chest);
    const meshes = visual.getChildMeshes().filter(mesh => mesh.getTotalVertices());
    const source = boundsFor(geometryPoints(meshes));
    if (!source || source.size.y <= 0) { destroyGroup(chest); return; }
    const scale = 0.09 / source.size.y;
    visual.scaling.setAll(scale);
    visual.position.copyFrom(source.center.scale(-scale));
    // The center was on the skin; tuck another quarter-depth inside so only
    // a small facet cap projects from the chest instead of a pendant shape.
    const position = fit.surface.subtract(new Vector3(0, 0, source.size.z * scale * .25));
    const local = Matrix.Translation(position.x, position.y, position.z).multiply(Matrix.Invert(fit.restToRoot));
    chest.rotationQuaternion = Quaternion.Identity();
    local.decompose(chest.scaling, chest.rotationQuaternion, chest.position);
    const magnitude = (Math.abs(chest.scaling.x) + Math.abs(chest.scaling.y) + Math.abs(chest.scaling.z)) / 3;
    chest.scaling.set(Math.sign(chest.scaling.x) * magnitude, Math.sign(chest.scaling.y) * magnitude, Math.sign(chest.scaling.z) * magnitude);
    chest.parent = fit.socket;
    for (const mesh of meshes) addNativeEmeraldGlow(mesh, 'cyan');
    chest.metadata = { ...chest.metadata, nativeColor: 'cyan', nativeTorsoSocket: fit.socket.name };
    item.chestEmerald = chest;
    canvas.dataset.chestEmerald = 'cyan-native';
    canvas.dispatchEvent(new CustomEvent('chao-assets', { detail: { key: 'owner-chest-emerald', loaded: true } }));
  }

  function castFloor(x, z, meshes) {
    const hits = scene.multiPickWithRay(new Ray(new Vector3(x, 100, z), Vector3.Down(), 250), mesh => meshes.has(mesh) && mesh.isEnabled()) || [];
    for (const hit of hits.sort((a, b) => a.distance - b.distance)) {
      if (!hit.pickedPoint) continue;
      const normal = hit.getNormal(true);
      if (!normal || normal.y > 0.3) return hit.pickedPoint.y;
    }
    return null;
  }

  function floorAt(x, z) {
    if (!terrain) return 0;
    const key = `${Math.round(x * 10)},${Math.round(z * 10)}`;
    if (floorCache.has(key)) return floorCache.get(key);
    const height = castFloor(x, z, terrain.meshes);
    if (floorCache.size > 6000) floorCache.clear();
    floorCache.set(key, height);
    return height;
  }

  function setNativeShadowTerrain(meshes) {
    nativeShadowTerrain = createNativeShadowTerrainIndex(meshes);
    for (const shadow of nativeChaoShadows) shadow.setTerrainIndex(nativeShadowTerrain);
  }

  function updateChaoShadow(item) {
    if (!item.nativeShadow) return;
    item.root.computeWorldMatrix(true);
    const center = item.root.getAbsolutePosition();
    const groundY = floorAt(center.x, center.z);
    const scale = Math.abs(item.root.scaling.x);
    item.nativeShadow.update({ center, width: .35 * scale, depth: .28 * scale,
      groundY, enabled: groundY !== null && item.root.isEnabled() && !item.vehicleKey });
  }

  async function loadNativeGarden(zone, version, parent, proxyScenery) {
    const layout = NATIVE_GARDENS[zone];
    if (!layout) return;
    const key = layout.asset;
    const container = await loadLocalAsset(key);
    if (!container || disposed || version !== worldVersion || parent.isDisposed()) return;
    const entries = container.instantiateModelsToScene(name => `native-${zone}-${name}`, false, { doNotInstantiate: true });
    const stage = new TransformNode(`native-stage-${zone}`, scene);
    stage.parent = parent;
    stage.scaling.setAll(layout.scale);
    stage.metadata = { importedEntries: entries };
    for (const root of entries.rootNodes) root.parent = stage;
    const meshes = new Set();
    const min = new Vector3(Infinity, Infinity, Infinity);
    const max = new Vector3(-Infinity, -Infinity, -Infinity);
    for (const mesh of stage.getChildMeshes()) {
      // glTF's __root__ is a zero-vertex Mesh that parents the whole stage.
      // Disabling it also disables every real terrain primitive below it.
      if (mesh.getTotalVertices() === 0) continue;
      mesh.computeWorldMatrix(true);
      const bounds = mesh.getBoundingInfo().boundingBox;
      const size = bounds.maximumWorld.subtract(bounds.minimumWorld);
      // The original sky domes are enormous. Keep them out of collision and
      // use the browser scene's sky around the original playable terrain.
      if (!isNativeTerrainMesh(mesh.name, size.x, size.z, mesh.getTotalVertices(), zone)) {
        mesh.setEnabled(false);
        continue;
      }
      min.minimizeInPlace(bounds.minimumWorld);
      max.maximizeInPlace(bounds.maximumWorld);
      mesh.metadata = { ...mesh.metadata, nativeTerrain: true };
      mesh.checkCollisions = true;
      mesh.isPickable = true;
      mesh.receiveShadows = true;
      meshes.add(mesh);
    }
    if (!meshes.size || !Number.isFinite(min.x)) { destroyGroup(stage); return; }
    stage.position.copyFromFloats(...layout.position);
    for (const mesh of meshes) mesh.computeWorldMatrix(true);
    terrain = { stage, meshes, key };
    floorCache.clear();
    if (![...meshes].some(mesh => mesh.isEnabled())) {
      terrain = null;
      destroyGroup(stage);
      canvas.dataset.nativeGarden = 'fallback';
      return;
    }
    proxyScenery.setEnabled(false);
    setNativeShadowTerrain(meshes);
    const nativeExit = attachNativeGardenExit(zone, meshes);
    canvas.dataset.nativeGardenExit = nativeExit.ready ? nativeExit.trigger : 'fallback';
    canvas.dataset.nativeGardenExitMeshes = nativeExit.meshes.join(',');
    canvas.dataset.nativeGardenExitReachable = String(nativeExit.ready && nativeExit.reachable);
    const nativePortals = { mission: [0, -8], hero: [-7, 2], dark: [7, 2], race: [6, 9] };
    for (const anchor of groundAnchors) {
      if (nativeExit.ready && anchor.node.metadata?.portalAction === 'hub') {
        anchor.node.setEnabled(false);
        continue;
      }
      const portalPosition = nativePortals[anchor.node.metadata?.portalAction];
      if (portalPosition) { anchor.node.position.x = portalPosition[0]; anchor.node.position.z = portalPosition[1]; }
      if (anchor.node.name === 'chao-race-guide') anchor.node.position.set(7, 0, 6);
      const floor = floorAt(anchor.node.position.x, anchor.node.position.z);
      anchor.node.position.y = (floor ?? 0) + (anchor.baseY || 0);
      if (anchor.node.metadata?.portalAction === 'hub' && floor === null) {
        for (const [x, z] of [[0, 8], [0, 7], [1, 7], [-1, 7], [0, 6]]) {
          const candidate = floorAt(x, z);
          if (candidate !== null) { anchor.node.position.set(x, candidate, z); break; }
        }
      }
    }
    canvas.dataset.nativeGarden = key;
    canvas.dispatchEvent(new CustomEvent('chao-assets', { detail: { key, loaded: true } }));
  }

  function groundVisualFeet(imported, targetY, parentScaleY) {
    let footY = Infinity;
    const meshes = [...imported.meshes, ...(imported.footwearMeshes || [])];
    for (const skeleton of new Set(meshes.map(mesh => mesh.skeleton).filter(Boolean))) skeleton.prepare(true);
    for (const mesh of meshes) {
      if (mesh.isDisposed() || !mesh.isEnabled() || !mesh.getTotalVertices()) continue;
      mesh.refreshBoundingInfo(true, true);
      mesh.computeWorldMatrix(true);
      footY = Math.min(footY, mesh.getBoundingInfo().boundingBox.minimumWorld.y);
    }
    if (Number.isFinite(footY)) imported.visual.position.y += (targetY - footY) / parentScaleY;
  }

  function groundImportedFeet(item) {
    if (!item.imported || item.root.isDisposed()) return;
    if (item.vehicle) {
      groundVisualFeet(item.vehicle, item.root.position.y, item.root.scaling.y);
      item.vehicle.pilotSeat.computeWorldMatrix(true);
      item.root.computeWorldMatrix(true);
      const seatWorld = item.vehicle.pilotSeat.getAbsolutePosition();
      const seatLocal = Vector3.TransformCoordinates(seatWorld, Matrix.Invert(item.root.getWorldMatrix()));
      item.imported.visual.position.x = seatLocal.x;
      item.imported.visual.position.z = seatLocal.z;
      groundVisualFeet(item.imported, seatWorld.y, item.root.scaling.y);
      if (item.nativeEmotion) item.nativeEmotion.position.set(seatLocal.x, item.imported.visual.position.y + item.imported.height + 0.13, seatLocal.z);
      if (item.accessory) item.accessory.position.set(seatLocal.x, item.imported.visual.position.y + item.imported.height * 0.78, seatLocal.z);
    } else groundVisualFeet(item.imported, item.root.position.y, item.root.scaling.y);
  }

  function setEquipment(item, equipped) {
    if (item.equipped === equipped) return;
    if (item.accessory) destroyGroup(item.accessory);
    item.accessory = null;
    item.equipped = equipped;
    if (equipped !== 'eggshell') return;
    const cap = new TransformNode('equipped-eggshell', scene);
    cap.parent = item.root;
    cap.position.y = item.imported ? item.imported.height * 0.78 : 0.38;
    cap.scaling.setAll(0.28);
    const shell = MeshBuilder.CreateSphere('eggshell-cap', { diameter: 1, slice: 0.5, segments: 12 }, scene);
    shell.scaling.set(1.12, 0.85, 1.08);
    shell.material = material('#f7ecd1');
    shell.parent = cap;
    cylinder('eggshell-rim', [0, -0.03, 0], { height: 0.12, diameterBottom: 0.98, diameterTop: 1.12, tessellation: 9 }, '#e5d8b7', cap);
    for (let i = 0; i < 3; i++) sphere('eggshell-speckle', [-0.3 + i * 0.27, 0.18 + i % 2 * 0.08, 0.48], [0.07, 0.1, 0.015], '#a9bdb3', cap, 6);
    item.accessory = cap;
  }

  async function attachNativeTree(root, proxyMeshes, fruit) {
    const container = await loadLocalAsset('tree-neutral');
    if (!container || disposed || root.isDisposed()) return;
    const visual = new TransformNode(`${root.name}-native`, scene);
    const entries = container.instantiateModelsToScene(name => `${root.uniqueId}-${name}`, false, { doNotInstantiate: true });
    for (const entryRoot of entries.rootNodes) entryRoot.parent = visual;
    let minimumY = Infinity;
    for (const mesh of visual.getChildMeshes()) {
      if (!mesh.getTotalVertices()) continue;
      mesh.computeWorldMatrix(true);
      minimumY = Math.min(minimumY, mesh.getBoundingInfo().boundingBox.minimumWorld.y);
      mesh.isPickable = fruit;
      if (fruit) mesh.metadata = { ...mesh.metadata, action: 'feed' };
      mesh.receiveShadows = true;
      shadows.addShadowCaster(mesh, false);
    }
    if (!Number.isFinite(minimumY)) {
      entries.dispose();
      visual.dispose();
      return;
    }
    visual.scaling.setAll(0.1);
    visual.position.y = -minimumY * 0.1;
    visual.parent = root;
    root.metadata = { ...root.metadata, importedEntries: entries, nativeTree: true };
    for (const mesh of proxyMeshes) mesh.setEnabled(false);
    canvas.dataset.nativeTrees = String(Number(canvas.dataset.nativeTrees || 0) + 1);
  }

  function tree(x, z, scale = 1, palette = 'green', fruit = false, native = false) {
    const root = new TransformNode('fruit-tree', scene);
    root.position.copyFromFloats(x, 0, z);
    root.scaling.setAll(scale);
    root.parent = scenery || world;
    const colors = palette === 'dark' ? ['#705a9d', '#9d66ab', '#514782'] : ['#62b46b', '#87c47a', '#459b69'];
    const trunk = cylinder('tree-trunk', [0, 1.8, 0], { height: 3.6, diameterBottom: 0.65, diameterTop: 0.4 }, '#94724c', root);
    trunk.rotation.z = 0.08;
    sphere('crown-main', [0, 4.5, 0], [3.9, 3.4, 3.7], colors[0], root, 6);
    sphere('crown-side', [-1.3, 4.0, 0.5], [2.9, 2.8, 2.7], colors[1], root, 6);
    sphere('crown-back', [1.2, 4.1, -0.5], [2.7, 2.9, 2.8], colors[2], root, 6);
    if (fruit) {
      for (const [px, py, pz] of [[1.1, 3.3, 1.1], [-1, 3.5, 1.3], [0.2, 3.8, -1.4], [-1.2, 3.3, -0.8]]) {
        const item = sphere('garden-fruit', [px, py, pz], [0.55, 0.64, 0.55], '#f6bc64', root);
        item.metadata = { action: 'feed' };
        cylinder('fruit-leaf', [px, py + 0.38, pz], { height: 0.28, diameter: 0.08 }, '#3e7960', root);
      }
    }
    for (const mesh of root.getChildMeshes()) if (!mesh.metadata?.labelTexture) shadows.addShadowCaster(mesh, false);
    animatedDecor.push({ node: root, kind: 'tree', phase: x + z });
    if (native) attachNativeTree(root, root.getChildMeshes(), fruit);
    return root;
  }

  function flower(x, z, color, scale = 1) {
    cylinder('flower-stem', [x, 0.24 * scale, z], { height: 0.48 * scale, diameter: 0.04 }, '#4f8f59');
    for (let i = 0; i < 5; i++) {
      const a = i * Math.PI * 2 / 5;
      sphere('flower-petal', [x + Math.cos(a) * 0.16 * scale, 0.48 * scale, z + Math.sin(a) * 0.16 * scale], [0.28 * scale, 0.12 * scale, 0.28 * scale], color, scenery || world, 6);
    }
    sphere('flower-heart', [x, 0.52 * scale, z], [0.16 * scale, 0.14 * scale, 0.16 * scale], '#f8dc80', scenery || world, 6);
  }

  function portal(x, z, title, tint, action) {
    const root = new TransformNode(`portal-${action}`, scene);
    root.position.copyFromFloats(x, 0, z);
    root.parent = world;
    root.scaling.setAll(0.25);
    root.metadata = { portalAction: action };
    groundAnchors.push({ node: root });
    for (const side of [-1, 1]) {
      cylinder('portal-column', [side * 1.4, 1.6, 0], { diameter: 0.65, height: 3.2 }, '#d7dfc9', root);
      cylinder('portal-foot', [side * 1.4, 0.2, 0], { diameter: 1.05, height: 0.4 }, '#afbea9', root);
      sphere('portal-orb', [side * 1.4, 3.4, 0], [0.65, 0.65, 0.65], tint, root);
    }
    const arch = MeshBuilder.CreateTorus('portal-arch', { diameter: 2.9, thickness: 0.45, tessellation: 24 }, scene);
    arch.rotation.x = Math.PI / 2;
    arch.position.y = 2.4;
    arch.parent = root;
    arch.material = material('#d7dfc9');
    const surface = MeshBuilder.CreateDisc('portal-surface', { radius: 1.25, tessellation: 40, sideOrientation: Mesh.DOUBLESIDE }, scene);
    surface.position.set(0, 1.6, 0.05);
    surface.scaling.y = 1.2;
    surface.material = material(tint, 0.35, 0.55);
    surface.parent = root;
    for (const mesh of root.getChildMeshes()) mesh.metadata = { action };
    label(title, [0, 4.4, 0], tint, 3.6, root);
    animatedDecor.push({ node: surface, kind: 'portal', phase: x });
    return root;
  }

  function makeChao(data, npc = false) {
    const root = new TransformNode(`chao-${data.id}`, scene);
    const form = String(data.form || 'normal').toLowerCase();
    const chaos = data.admin || form.includes('chaos');
    const dark = form.includes('dark');
    const hero = form.includes('hero');
    const skin = chaos ? '#b9f4e4' : dark ? '#5a567e' : hero ? '#fff4dc' : '#8bd8ea';
    const accent = chaos ? '#f6d78a' : dark ? '#f19b9c' : hero ? '#eac974' : '#e8d383';
    const belly = chaos ? '#e8fff2' : dark ? '#a58baa' : '#d4f1d5';
    const body = sphere('chao-body', [0, 0.8, 0], [0.95, 1.16, 0.8], skin, root);
    sphere('chao-belly', [0, 0.78, 0.33], [0.64, 0.73, 0.2], belly, root);
    const head = sphere('chao-head', [0, 1.72, 0.02], [1.43, 1.35, 1.32], skin, root, 16);
    const tip = cylinder('chao-head-tip', [0.22, 2.35, -0.09], { height: 0.9, diameterBottom: 0.83, diameterTop: 0.02, tessellation: 12 }, skin, root);
    tip.rotation.z = -0.42;
    tip.rotation.x = -0.25;
    const limbs = [];
    for (const side of [-1, 1]) {
      const arm = sphere('chao-arm', [side * 0.62, 0.93, 0.02], [0.36, 0.62, 0.38], skin, root);
      arm.rotation.z = side * 0.4;
      const foot = sphere('chao-foot', [side * 0.32, 0.19, 0.17], [0.48, 0.34, 0.61], accent, root);
      const eye = sphere('chao-eye', [side * 0.27, 1.76, 0.635], [0.135, dark ? 0.19 : 0.285, 0.09], dark ? '#fce6c1' : '#183b46', root);
      if (dark) eye.rotation.z = side * 0.3;
      sphere('chao-eye-shine', [side * 0.26 - 0.022, 1.82, 0.68], [0.035, 0.055, 0.025], '#ffffff', root, 6);
      sphere('chao-cheek', [side * 0.46, 1.54, 0.54], [0.17, 0.09, 0.07], dark ? '#c57c9c' : '#ecc2b6', root);
      const wing = sphere('chao-wing', [side * 0.61, 1.06, -0.23], [0.15, 0.73, 0.66], accent, root, 8);
      wing.rotation.z = side * 0.6;
      wing.rotation.y = side * 0.4;
      limbs.push({ arm, foot, wing, side });
    }
    const smile = [];
    for (let i = 0; i <= 12; i++) {
      const a = Math.PI * (0.15 + i / 12 * 0.7);
      smile.push(new Vector3(Math.cos(a) * 0.12, 1.51 - Math.sin(a) * 0.065, 0.663));
    }
    const mouth = MeshBuilder.CreateTube('chao-smile', { path: smile, radius: 0.016, tessellation: 5 }, scene);
    mouth.parent = root;
    mouth.material = material('#275260');
    let emotion;
    if (hero || chaos) {
      emotion = MeshBuilder.CreateTorus('chao-halo', { diameter: chaos ? 0.72 : 0.52, thickness: 0.075, tessellation: 28 }, scene);
      emotion.position.y = 2.93;
      emotion.material = material(accent, 0.55);
      emotion.parent = root;
    } else {
      emotion = sphere('chao-emotion', [0, 2.98, 0], [0.34, 0.34, 0.34], accent, root);
      emotion.material = material(accent, 0.4);
      if (dark) {
        for (let i = 0; i < 6; i++) {
          const a = i * Math.PI / 3;
          const spike = cylinder('dark-emotion-spike', [Math.cos(a) * 0.22, 2.98 + Math.sin(a) * 0.22, 0], { height: 0.22, diameterBottom: 0.15, diameterTop: 0 }, accent, root);
          spike.rotation.z = a - Math.PI / 2;
        }
      }
    }
    if (data.admin || chaos) emotion.setEnabled(false);
    const proxyScale = 0.18;
    for (const node of root.getChildren()) {
      node.position?.scaleInPlace(proxyScale);
      node.scaling?.scaleInPlace(proxyScale);
    }
    const proxyMeshes = root.getChildMeshes().filter(mesh => mesh !== emotion);
    const name = `${data.admin ? 'ADMIN • ' : ''}${data.name || 'Chao'}${npc ? ' · NPC' : ''}`;
    const plateWidth = npc ? 0.8 : 0.7;
    const nameplate = label(name, [0, 0.94, 0], data.admin ? '#ffdc72' : '#e8f7ef', plateWidth, root, data.admin ? 'rgba(117,27,35,.96)' : 'rgba(18,42,42,.84)');
    nameplate.metadata.baseWidth = plateWidth;
    for (const mesh of root.getChildMeshes()) if (!mesh.metadata?.labelTexture) shadows.addShadowCaster(mesh, false);
    const item = { root, body, head, limbs, emotion, emotionBase: 2.98 * proxyScale, nameplate, proxyMeshes, proxyScale, admin: !!data.admin, gear: gearFor(data), vehicleKey: vehicleFor(data), appearance: appearanceFor(data), name, target: new Vector3(data.x || 0, data.y || 0, data.z || 0), lastPosition: new Vector3(), moving: false, npc };
    item.bodyState = data.body;
    if (['chao', 'neutral-chaos', 'hero-chaos', 'dark-chaos'].includes(assetFor(data)) || assetFor(data)?.startsWith('chao-body-')) {
      item.nativeShadow = createNativeChaoShadow(scene, { textureUrl: '/api/assets/chao-shadow.png', terrainIndex: nativeShadowTerrain });
      root.metadata = { ...root.metadata, nativeContactShadow: item.nativeShadow };
      nativeChaoShadows.add(item.nativeShadow);
      glow.addExcludedMesh(item.nativeShadow.mesh);
    }
    attachLocalAsset(item, assetFor(data));
    return item;
  }

  function addNPC(id, name, x, z, form, action, asset = null) {
    const chao = makeChao({ id, name, x, z, form, asset }, true);
    chao.action = action;
    chao.root.parent = world;
    chao.root.position.set(x, 0, z);
    chao.root.scaling.setAll(0.9);
    groundAnchors.push({ node: chao.root });
    for (const mesh of chao.root.getChildMeshes()) mesh.metadata = { ...mesh.metadata, action };
    animatedDecor.push({ node: chao.root, kind: 'npc', phase: x, chao });
  }

  function buildWorld(zone) {
    for (const shot of nativeShots) destroyGroup(shot.root);
    nativeShots.length = 0;
    if (world) destroyGroup(world);
    worldVersion++;
    terrain = null;
    nativeShadowTerrain = null;
    floorCache.clear();
    groundAnchors.length = 0;
    animatedDecor.length = 0;
    canvas.dataset.nativeTrees = '0';
    canvas.dataset.nativeGarden = 'fallback';
    canvas.dataset.nativeGardenExit = 'fallback';
    canvas.dataset.nativeGardenExitMeshes = '';
    canvas.dataset.nativeGardenExitReachable = 'false';
    world = new TransformNode(`world-${zone}`, scene);
    scenery = new TransformNode('procedural-scenery', scene);
    scenery.parent = world;
    const dark = zone === 'dark';
    const mission = zone === 'mission';
    const hero = zone === 'hero';
    const race = zone === 'race';
    const sky = dark ? '#5c638d' : mission ? '#c6b8ae' : hero ? '#dce9e5' : '#bddeed';
    const grass = dark ? '#758181' : mission ? '#a0ad85' : hero ? '#bddb9b' : '#91bf84';
    scene.clearColor = Color4.FromHexString(`${sky}ff`);
    scene.fogColor = Color3.FromHexString(sky);
    sun.intensity = dark ? 0.5 : 0.7;
    hemi.intensity = dark ? 0.46 : 0.6;
    const island = cylinder('floating-island', [0, -2.4, 0], { height: 4.5, diameterTop: 66, diameterBottom: 48, tessellation: 16 }, dark ? '#5b5867' : '#a68d6d');
    island.receiveShadows = true;
    const ground = cylinder('garden-ground', [0, -0.225, 0], { height: 0.45, diameter: 65, tessellation: 64 }, grass);
    ground.receiveShadows = true;
    setNativeShadowTerrain([ground]);
    if (zone === 'hub') applyOriginalGrass(ground);
    for (let i = 0; i < 9; i++) {
      const a = i * Math.PI * 2 / 9;
      const x = Math.sin(a) * 66;
      const z = Math.cos(a) * 66;
      const y = -2 + Math.sin(i * 4) * 5;
      cylinder('distant-island-rock', [x, y, z], { height: 8, diameterTop: 12, diameterBottom: 0, tessellation: 7 }, dark ? '#68667f' : '#b1b69b');
      cylinder('distant-island-grass', [x, y + 4, z], { height: 0.5, diameter: 12, tessellation: 9 }, dark ? '#828aa0' : '#a1c599');
      sphere('distant-tree', [x + 1, y + 7, z], [5, 6, 5], dark ? '#7b6f9a' : '#86b59a', scenery, 5);
      sphere('cloud', [x - 5, y + 16, z + 10], [18, 3, 8], dark ? '#a3a3b6' : '#f8f5e7', scenery, 8);
    }
    if (!mission && !race) {
      const rim = sphere('pond-stone-rim', [-8, -0.08, 0], [12.2, 0.55, 10.7], '#c8c9aa');
      const pond = cylinder('pond-water', [-8, 0.12, 0], { height: 0.05, diameter: 10.5, tessellation: 64 }, dark ? '#7e8ba6' : '#71bbc4');
      pond.scaling.z = 0.91;
      pond.material = material(dark ? '#7e8ba6' : '#71bbc4', 0.12, 0.94);
      rim.receiveShadows = true;
      for (let i = 0; i < 4; i++) {
        const ripple = MeshBuilder.CreateTorus('water-ripple', { diameter: 1 + i * 1.5, thickness: 0.025, tessellation: 40 }, scene);
        ripple.position.set(-8, 0.17, 0);
        ripple.scaling.z = 0.85;
        ripple.material = material('#d1f1dc', 0.1, 0.35);
        ripple.parent = scenery;
        animatedDecor.push({ node: ripple, kind: 'ripple', phase: i });
      }
      for (const [x, z, s] of [[-18, 6, 1.15], [-20, -8, 1], [-13, -15, 1.3], [7, -9, 0.95], [19, 15, 1.25], [-17, 15, 0.9]]) tree(x, z, s, dark ? 'dark' : 'green', false, zone === 'hub');
      const fruitTree = tree(-7, -5, 0.83, dark ? 'dark' : 'green', true, zone === 'hub');
      if (zone === 'hub') fruitTree.parent = world;
      const fruitLabel = label('FRUIT GROVE', [-7, 4.6, -5], '#ffdfa1', 1.6, zone === 'hub' ? world : scenery);
      for (let i = 0; i < 45; i++) {
        const x = Math.sin(i * 15.31) * 23;
        const z = Math.cos(i * 9.7) * 22;
        if ((x + 8) ** 2 + z ** 2 < 45 || Math.abs(x) < 3) continue;
        flower(x, z, ['#f3db9c', '#eee7d4', '#d5aed1'][i % 3], 0.75 + (i % 3) * 0.2);
      }
      for (let i = 0; i < 14; i++) {
        const z = 13 - i * 1.7;
        const stepping = cylinder('stepping-stone', [Math.sin(i * 0.7) * 0.65, -0.005, z], { diameter: 1.15, height: 0.01, tessellation: 8 }, '#d9d9b9');
        stepping.material.zOffset = -2;
        stepping.scaling.z = 0.75;
      }
      if (zone === 'hub') {
        portal(0, -18, 'OUTSKIRTS · MISSION', '#eabf86', 'mission');
        portal(-17, -13, 'HERO GARDEN', '#d9eead', 'hero');
        portal(17, -13, 'DARK GARDEN', '#cbb5e8', 'dark');
        portal(-17, 5, 'NEUTRAL GARDEN', '#b3e2b0', 'garden');
        portal(17, 5, 'GARDEN CIRCUIT', '#b4e3df', 'race');
        addNPC('trainer', 'Knuckles · Trainer', 7, 2, 'normal', 'train-power', 'knuckles');
        addNPC('race-guide', 'Tails · Race Guide', 12, 6, 'hero', 'race', 'tails');
        addNPC('merchant', 'Amy Chao · Market', -4, 8, 'normal', 'shop', 'amy');
      } else portal(0, 8, 'BACK TO HUB', '#f2cf78', 'hub');
      const waterfall = box('waterfall', [-27, -4, -8], [2.2, 10, 0.28], '#a8dadd');
      waterfall.material = material('#a8dadd', 0.12, 0.75);
      for (let i = 0; i < 10; i++) {
        const spark = sphere('garden-mote', [Math.sin(i * 3) * 20, 1.5 + (i % 4), Math.cos(i * 1.8) * 20], [0.08, 0.08, 0.08], '#fff2bd', scenery, 6);
        spark.material = material('#fff2bd', 0.8);
        animatedDecor.push({ node: spark, kind: 'mote', phase: i });
      }
    } else if (mission) {
      box('mission-courtyard', [0, -0.02, -2], [30, 0.04, 33], '#b6b5a0');
      for (let i = 0; i < 9; i++) box('courtyard-paving', [0, 0.005, 13 - i * 3.4], [0.16, 0.01, 1.3], '#f1e1b0');
      for (let i = 0; i < 8; i++) {
        const side = i % 2 === 0 ? -1 : 1;
        const h = 4 + (i % 3) * 2;
        const x = side * (18 + (i % 3) * 2);
        const z = 12 - Math.floor(i / 2) * 10;
        box('outskirts-building', [x, h / 2, z], [4.7, h, 5], ['#c8c8b4', '#9ba7a0', '#b2b8aa'][i % 3]);
        box('building-roof', [x, h + 0.2, z], [5.1, 0.4, 5.4], '#e1d4b2');
        for (let j = 0; j < 3; j++) box('building-window', [x - side * 2.38, 1.5 + j * 1.6, z], [0.04, 0.65, 2.5], '#d9edf0');
      }
      for (const x of [-14, 14]) for (const z of [-14, 12]) {
        cylinder('mission-lamp', [x, 2, z], { height: 4, diameter: 0.18 }, '#667b76');
        sphere('mission-light', [x, 4.1, z], [0.5, 0.6, 0.5], '#fff1b0');
      }
      label('OUTSKIRTS · TRAINING MISSION', [0, 5.3, -18], '#ffe1b1', 6.6);
      portal(0, 20, 'RETURN TO GARDEN', '#c9e6b1', 'garden');
    } else {
      const checkpoints = [[0, 12], [-12, 0], [0, -12], [12, 0], [0, 12]];
      const path = checkpoints.map(([x, z]) => new Vector3(x, 0.04, z));
      const track = MeshBuilder.CreateTube('race-track', { path, radius: 2.6, tessellation: 16 }, scene);
      track.scaling.y = 0.0019;
      track.material = material('#d8cf9c');
      track.parent = scenery;
      track.receiveShadows = true;
      for (let i = 0; i < checkpoints.length - 1; i++) {
        const [x, z] = checkpoints[i];
        const gate = MeshBuilder.CreateTorus(`checkpoint-${i}`, { diameter: 1.6, thickness: 0.06, tessellation: 32 }, scene);
        gate.rotation.x = Math.PI / 2;
        gate.rotation.y = i * Math.PI / 2;
        gate.position.set(x, 0.7, z);
        gate.material = material(i === 0 ? '#f6d886' : '#a0d4d0', 0.18);
        gate.parent = world;
        label(i === 0 ? 'START / FINISH' : `CHECKPOINT ${i}`, [x, 1.85, z], '#f9efc9', 1.5);
      }
      for (const [x, z] of [[-20, -14], [19, 15], [-19, 14], [18, -16]]) tree(x, z, 1.2);
      for (let i = 0; i < 12; i++) {
        const a = i * Math.PI * 2 / 12;
        flower(Math.cos(a) * 6, Math.sin(a) * 6, '#f6e3b1', 1.2);
      }
      label('GARDEN CIRCUIT', [0, 4, 0], '#fff0bc', 4.5);
    }
    if (zone === 'garden' || zone === 'hero' || zone === 'dark') {
      const version = worldVersion;
      loadNativeGarden(zone, version, world, scenery).catch(() => {
        if (disposed || version !== worldVersion) return;
        canvas.dataset.nativeGarden = 'fallback';
        canvas.dataset.nativeGardenExit = 'fallback';
        canvas.dataset.nativeGardenExitMeshes = '';
        canvas.dataset.nativeGardenExitReachable = 'false';
      });
    }
  }

  function makeRing(data) {
    const root = new TransformNode(`ring-${data.id}`, scene);
    root.scaling.setAll(0.25);
    const ring = MeshBuilder.CreateTorus('collectible-ring', { diameter: 0.82, thickness: 0.15, tessellation: 24 }, scene);
    ring.rotation.x = Math.PI / 2;
    ring.material = material('#f3ce70', 0.35);
    ring.parent = root;
    root.position.set(data.x, 0.25, data.z);
    return { root, phase: (data.x + data.z) * 0.2 };
  }

  function makeEnemy(data) {
    const root = new TransformNode(`robot-${data.id}`, scene);
    root.scaling.setAll(0.3);
    root.position.set(data.x, 0, data.z);
    const body = sphere('robot-shell', [0, 1.1, 0], [1.45, 1.4, 1.2], '#889e9c', root);
    const visor = box('robot-visor', [0, 1.3, 0.55], [1.03, 0.3, 0.16], '#4b5c60', root);
    sphere('robot-eye', [0, 1.3, 0.67], [0.32, 0.17, 0.11], '#ee9c7b', root);
    for (const side of [-1, 1]) {
      box('robot-leg', [side * 0.45, 0.32, 0], [0.23, 0.55, 0.23], '#5e7473', root);
      box('robot-foot', [side * 0.45, 0.12, 0.16], [0.55, 0.25, 0.65], '#d5c6aa', root);
      sphere('robot-arm', [side * 0.93, 0.9, 0], [0.5, 0.65, 0.5], '#b8c1ad', root);
    }
    cylinder('robot-aerial', [0, 2.03, 0], { height: 0.45, diameter: 0.06 }, '#697c7d', root);
    sphere('robot-signal', [0, 2.31, 0], [0.18, 0.18, 0.18], '#f8c385', root);
    const hp = box('robot-hp', [0, 2.6, 0], [1.2, 0.09, 0.08], '#e7b17f', root);
    for (const mesh of root.getChildMeshes()) mesh.metadata = { action: 'attack' };
    for (const mesh of root.getChildMeshes()) if (!mesh.metadata?.labelTexture) shadows.addShadowCaster(mesh, false);
    return { root, body, visor, hp, target: new Vector3(data.x, 0, data.z), maxHp: data.maxHp || data.hp || 50 };
  }

  function syncEntities(map, rows, make, update) {
    const keep = new Set();
    for (const data of rows) {
      if (data.zone && data.zone !== activeZone) continue;
      keep.add(data.id);
      let item = map.get(data.id);
      if (!item) { item = make(data); map.set(data.id, item); }
      update?.(item, data);
    }
    for (const [id, item] of map) if (!keep.has(id)) { destroyGroup(item.root); map.delete(id); }
  }

  function setSnapshot(next) {
    snapshot = next;
    const self = next.players?.find(p => p.id === next.selfId);
    const zone = self?.zone || next.zone || 'garden';
    if (zone !== activeZone) {
      activeZone = zone;
      buildWorld(zone);
      for (const map of [playerMeshes, ringMeshes, enemyMeshes]) {
        for (const item of map.values()) destroyGroup(item.root);
        map.clear();
      }
    }
    syncEntities(playerMeshes, next.players || [], makeChao, (item, data) => {
      const appearance = appearanceFor(data);
      const name = `${data.admin ? 'ADMIN • ' : ''}${data.name || 'Chao'}`;
      if (item.appearance !== appearance || item.name !== name) {
        destroyGroup(item.root);
        const replacement = makeChao(data);
        playerMeshes.set(data.id, replacement);
        item = replacement;
      }
      item.bodyState = data.body;
      item.target.set(data.x || 0, data.y || 0, data.z || 0);
      if (item.root.position.lengthSquared() === 0) item.root.position.copyFrom(item.target);
      item.self = data.id === next.selfId;
      item.hp = data.hp;
      item.absoluteY = Number.isFinite(data.groundY);
      item.heading = Number.isFinite(data.facing) ? data.facing : null;
      item.mech = data.mech || null;
      const previous = item.motionSample;
      if (!Number.isFinite(previous?.serverTime) || previous.zone !== data.zone || next.serverTime > previous.serverTime) {
        item.motionSample = observeSkateMotion(previous, data, next.serverTime, performance.now());
      }
      setOwnerGear(item, data.gear);
      setEquipment(item, data.equipped);
      item.root.setEnabled(data.zone === zone || !data.zone);
    });
    syncEntities(ringMeshes, next.rings || [], makeRing);
    syncEntities(enemyMeshes, next.enemies || [], makeEnemy, (item, data) => {
      item.target.set(data.x, 0, data.z);
      item.hp.scaling.x = Math.max(0.01, Math.min(1, data.hp / item.maxHp)) * 1.2;
    });
  }

  function textFocused() {
    const el = document.activeElement;
    return el instanceof HTMLElement && (el.isContentEditable || /INPUT|TEXTAREA|SELECT/.test(el.tagName));
  }
  function keyDown(event) {
    if (paused || textFocused()) return;
    if (['Space', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'AltLeft', 'AltRight'].includes(event.code)) event.preventDefault();
    keys.add(event.code);
    if (!event.repeat && event.code === 'KeyF') onAction('attack');
    if (!event.repeat && event.code === 'KeyE') onAction('feed');
    if (!event.repeat && event.code === 'KeyR') {
      const local = playerMeshes.get(snapshot?.selfId);
      if (local) camera.alpha = -Math.PI / 2 - local.root.rotation.y;
    }
  }
  function keyUp(event) { keys.delete(event.code); }
  function blur() { keys.clear(); touch.dx = 0; touch.dz = 0; inputPace = 0; onInput({ dx: 0, dz: 0, jump: false, boost: false }); }
  function pointerDown(event) {
    if (paused || event.button !== 0) return;
    const pick = scene.pick(scene.pointerX, scene.pointerY);
    if (!pick?.hit) return;
    const action = pick.pickedMesh?.metadata?.nativeGardenExit
      ? nativeGardenExitAction(activeZone, pick) : pick.pickedMesh?.metadata?.action;
    if (action) onAction(action);
  }
  window.addEventListener('keydown', keyDown);
  window.addEventListener('keyup', keyUp);
  window.addEventListener('blur', blur);
  canvas.addEventListener('pointerdown', pointerDown);
  const suppressContextMenu = event => event.preventDefault();
  canvas.addEventListener('contextmenu', suppressContextMenu);
  const resize = () => engine.resize();
  window.addEventListener('resize', resize);
  const resizeObserver = new ResizeObserver(resize);
  resizeObserver.observe(canvas);
  buildWorld('hub');
  activeZone = 'hub';
  scene.onBeforeAnimationsObservable.add(() => {
    for (const item of playerMeshes.values()) item.imported?.steadyHead?.restoreBase();
    for (const item of playerMeshes.values()) item.imported?.skate?.restoreBase();
  });
  scene.onAfterAnimationsObservable.add(() => {
    for (const item of playerMeshes.values()) {
      if (!item.nativeMotion?.available || !Number.isFinite(item.nativeMotion.frame)) item.imported?.skate?.apply();
    }
    for (const item of playerMeshes.values()) groundImportedFeet(item);
    for (const item of playerMeshes.values()) {
      const skate = item.imported?.skate?.diagnostics;
      item.imported?.steadyHead?.update(renderDelta, { mounted: !!item.vehicleKey,
        gait: item.nativeGait || 'Idle', skateBlend: item.nativeMotion?.available && Number.isFinite(item.nativeMotion.frame) ? 0 : skate?.blend || 0, skatePhase: skate?.phase || 0 });
      item.imported?.steadyHead?.apply();
    }
    for (const item of animatedDecor) if (item.kind === 'npc') groundImportedFeet(item.chao);
  });
  scene.onBeforeRenderObservable.add(() => {
    const fitPlate = item => {
      const plate = item.nameplate;
      if (!plate || plate.isDisposed()) return;
      plate.computeWorldMatrix(true);
      const distance = Math.max(0.2, Vector3.Distance(camera.position, plate.getAbsolutePosition()));
      const width = plate.metadata.baseWidth * item.root.scaling.x;
      const worldWidthFor190Pixels = 190 * 2 * distance * Math.tan(camera.fov / 2) / Math.max(1, canvas.clientHeight);
      plate.scaling.setAll(Math.min(1, worldWidthFor190Pixels / width));
    };
    for (const item of playerMeshes.values()) fitPlate(item);
    for (const decor of animatedDecor) if (decor.kind === 'npc') fitPlate(decor.chao);
  });

  engine.runRenderLoop(() => {
    if (disposed) return;
    const dt = Math.min(0.05, engine.getDeltaTime() / 1000);
    renderDelta = dt;
    time += dt;
    if (flameTexture) flameTexture.uOffset = (Math.floor(time * 18) % 16) / 16;
    inputTimer += dt;
    if (inputTimer >= 0.05) {
      const inputDt = inputTimer;
      inputTimer = 0;
      const acceptsInput = !paused && !textFocused();
      let dx = acceptsInput ? (keys.has('KeyD') || keys.has('ArrowRight') ? 1 : 0) - (keys.has('KeyA') || keys.has('ArrowLeft') ? 1 : 0) + touch.dx : 0;
      let dz = acceptsInput ? (keys.has('KeyS') || keys.has('ArrowDown') ? 1 : 0) - (keys.has('KeyW') || keys.has('ArrowUp') ? 1 : 0) + touch.dz : 0;
      const length = Math.hypot(dx, dz);
      if (length > 1) { dx /= length; dz /= length; }
      // Babylon's left-handed camera basis can point screen-right toward -X.
      // Obtain the actual view basis instead of assuming world-axis signs.
      const forward = camera.getDirection(Vector3.Forward());
      const right = camera.getDirection(Vector3.Right());
      forward.y = 0; right.y = 0;
      forward.normalize(); right.normalize();
      let worldDx = right.x * dx - forward.x * dz;
      let worldDz = right.z * dx - forward.z * dz;
      const mounted = !!playerMeshes.get(snapshot?.selfId)?.vehicleKey;
      const paceMode = keys.has('ShiftLeft') || keys.has('ShiftRight') ? 'walk' : keys.has('AltLeft') || keys.has('AltRight') ? 'run' : gaitControls.mode;
      inputPace = stepInputPace(inputPace, paceMode, length > 0, inputDt, mounted);
      worldDx *= inputPace; worldDz *= inputPace;
      canvas.dataset.gaitInputMode = paceMode; canvas.dataset.gaitInputScale = inputPace.toFixed(3);
      onInput({ dx: worldDx, dz: worldDz, jump: acceptsInput && keys.has('Space') && !mounted, boost: acceptsInput && keys.has('Space') && mounted });
    }
    const interpolation = Math.min(1, dt * 13);
    for (const item of playerMeshes.values()) {
      item.lastPosition.copyFrom(item.root.position);
      item.root.position = Vector3.Lerp(item.root.position, item.target, interpolation);
      const movement = item.root.position.subtract(item.lastPosition);
      const horizontalSpeed = Math.hypot(movement.x, movement.z) / Math.max(0.001, dt);
      item.groundSpeed = (item.groundSpeed || 0) + (horizontalSpeed - (item.groundSpeed || 0)) * Math.min(1, dt * 8);
      item.moving = Math.hypot(movement.x, movement.z) > 0.003;
      if (item.moving || item.heading !== null && item.heading !== undefined) {
        const desired = item.heading ?? Math.atan2(movement.x, movement.z);
        let diff = desired - item.root.rotation.y;
        while (diff > Math.PI) diff -= Math.PI * 2;
        while (diff < -Math.PI) diff += Math.PI * 2;
        item.root.rotation.y += diff * Math.min(1, dt * 15);
      }
      const swimming = !terrain && activeZone !== 'mission' && activeZone !== 'race' && (item.root.position.x + 8) ** 2 + item.root.position.z ** 2 < 25;
      const phase = time * (item.moving ? 12 : 2.3);
      item.body.position.y = (0.8 + Math.sin(phase) * (item.moving ? 0.07 : 0.025)) * item.proxyScale;
      item.head.position.y = 1.72 * item.proxyScale;
      item.emotion.position.y = item.emotionBase + Math.sin(time * 2.8) * 0.02;
      if (item.imported) {
        const motion = item.motionSample;
        const fresh = !!motion?.valid && performance.now() - motion.observedAt < 350;
        const imported = item.imported;
        const skate = imported.skate;
        skate?.restoreBase();

        const grounded = !!motion?.grounded;
        const speed = fresh ? motion.speed : 0;
        const swimming = !!motion?.swimming;
        const mounted = !!item.vehicleKey;
        const movingPose = fresh && !swimming && !mounted && (grounded || skate?.diagnostics.blend > 0);
        item.nativeVisualSpeed = movingPose ? smoothObservedSpeed(item.nativeVisualSpeed, speed, dt) : 0;
        const kinematics = { speed, visualSpeed: item.nativeVisualSpeed, grounded,
          jumpOffset: motion?.jumpOffset, verticalVelocity: motion?.verticalVelocity, fresh, swimming, mounted };
        const groundMotion = grounded
          ? updateNativeLocomotion(imported, imported.motionState, dt, kinematics) : null;

        skate?.update(dt, {
          speed, grounded: grounded && groundMotion?.mode !== 'Land',
          airGap: motion?.airGap, descending: motion?.descending, descendingGround: motion?.descendingGround,
          serverTime: motion?.serverTime, swimming, mounted, fresh,
          footwear: item.gear?.footwear === 'shadow-air-shoes' && !!item.ownerGear?.footwear?.length,
        });
        const skating = skate?.diagnostics;
        item.nativeMotion = groundMotion ?? updateNativeLocomotion(imported, imported.motionState, dt, {
          ...kinematics, grounded: !!skating?.coasting,
        });
        item.nativeGait = item.nativeMotion.mode;
        const groundJets = fresh && grounded && !swimming && !mounted && item.nativeMotion.mode !== 'Land';
        for (const entry of item.ownerGear?.footwear || []) {
          updateNativeShoeEffects(entry.nativeEffects, groundJets ? skating?.blend || 0 : 0, motion?.speed);
        }
      }
      if (item.vehicle) animateWalker(item, dt);
      for (const emerald of item.emeralds || []) {
        const phase = time * 0.65 + emerald.phase;
        const radius = item.vehicle ? item.vehicle.radius + 0.4 : 0.4;
        const height = item.vehicle?.height || item.imported?.height || 0.5;
        emerald.root.scaling.setAll(item.vehicle ? 5 : 1);
        emerald.root.position.set(Math.cos(phase) * radius, height * 0.55 + Math.sin(phase * 2) * 0.035, Math.sin(phase) * radius);
        emerald.root.rotation.y = time * 0.8 + emerald.phase;
      }
      for (const { arm, foot, wing, side } of item.limbs) {
        arm.rotation.x = item.moving ? Math.sin(phase + side * Math.PI / 2) * 0.7 : 0;
        foot.rotation.x = item.moving ? Math.sin(phase - side * Math.PI / 2) * 0.45 : 0;
        wing.rotation.z = side * (0.6 + Math.sin(time * (swimming ? 10 : 4)) * 0.12);
      }
      // Interpolate authoritative Y together with X/Z, so a slope's next
      // snapshot height cannot get ahead of the rendered feet horizontally.
      if (!item.absoluteY) item.root.position.y = (floorAt(item.root.position.x, item.root.position.z) ?? 0) + item.target.y - (swimming ? 0.25 : 0);
      if (item.self) {
        const mounted = !!item.vehicle;
        if (mounted !== selfVehicleActive) {
          camera.inertialRadiusOffset = 0;
          if (mounted) { onFootCameraRadius = camera.radius; camera.radius = Math.min(camera.upperRadiusLimit, Math.max(camera.radius, mountedCameraRadius(item.vehicle))); }
          else camera.radius = onFootCameraRadius;
          selfVehicleActive = mounted;
        }
        const target = item.root.position.add(new Vector3(0, mounted ? item.vehicle.height * 0.55 : 0.28, 0));
        // Preserve the player's orbit when translating its focus. The target
        // property setter otherwise rebuilds alpha/beta/radius from the old
        // camera position, immediately undoing the mounted fit and zoom.
        // The actor already interpolates all three authoritative coordinates.
        // Damping its camera a second time makes the fixed head drift on
        // slopes and during acceleration. Track that rendered actor exactly;
        // the player's orbit, zoom and collision controls remain independent.
        camera.setTarget(target, false, false, true);
      }
    }
    for (let index = nativeShots.length - 1; index >= 0; index--) {
      const shot = nativeShots[index];
      shot.age += dt;
      shot.root.position.copyFrom(Vector3.Lerp(shot.start, shot.end, Math.min(1, shot.age / shot.duration)));
      if (shot.age >= shot.duration) { destroyGroup(shot.root); nativeShots.splice(index, 1); }
    }
    for (const item of ringMeshes.values()) {
      item.root.rotation.y += dt * 1.8;
      item.root.position.y = (floorAt(item.root.position.x, item.root.position.z) ?? 0) + 0.25 + Math.sin(time * 2.3 + item.phase) * 0.035;
    }
    for (const item of enemyMeshes.values()) {
      const diff = item.target.subtract(item.root.position);
      item.root.position = Vector3.Lerp(item.root.position, item.target, interpolation);
      if (diff.lengthSquared() > 0.01) item.root.rotation.y = Math.atan2(diff.x, diff.z);
      item.body.position.y = 1.1 + Math.sin(time * 6) * 0.07;
    }
    for (const item of animatedDecor) {
      if (item.kind === 'tree') item.node.rotation.z = Math.sin(time * 0.6 + item.phase) * 0.012;
      if (item.kind === 'mote') item.node.position.y += Math.sin(time * 1.3 + item.phase) * dt * 0.18;
      if (item.kind === 'ripple') {
        const scale = 0.95 + Math.sin(time + item.phase) * 0.06;
        item.node.scaling.x = scale;
        item.node.scaling.z = scale * 0.85;
      }
      if (item.kind === 'portal') item.node.material.alpha = 0.46 + Math.sin(time * 2 + item.phase) * 0.07;
      if (item.kind === 'npc') {
        item.chao.emotion.position.y = item.chao.emotionBase + Math.sin(time * 2 + item.phase) * 0.02;
        if (item.chao.imported && !item.chao.imported.current && item.chao.imported.idle) {
          item.chao.imported.idle.start(true, 1);
          item.chao.imported.current = item.chao.imported.idle;
        }
        item.node.rotation.y = Math.sin(time * 0.4 + item.phase) * 0.15;
      }
    }
    for (const item of playerMeshes.values()) updateChaoShadow(item);
    for (const item of animatedDecor) if (item.kind === 'npc') updateChaoShadow(item.chao);
    scene.render();
    // Read-only QA measurements of the rendered view, including the actual
    // camera position after Babylon has applied its orbit/collision logic.
    const self = playerMeshes.get(snapshot?.selfId);
    canvas.dataset.nativeChaoShadow = self?.nativeShadow?.ready ? self.nativeShadow.mesh.isEnabled() ? 'visible' : 'no-floor' : 'unavailable';
    canvas.dataset.nativeChaoShadowTriangles = String((self?.nativeShadow?.mesh.getTotalIndices() || 0) / 3);
    canvas.dataset.cameraRadius = camera.radius.toFixed(3);
    canvas.dataset.cameraDistance = Vector3.Distance(camera.position, camera.target).toFixed(3);
    canvas.dataset.cameraCollisions = String(camera.checkCollisions);
    canvas.dataset.cameraTargetY = camera.target.y.toFixed(3);
    canvas.dataset.renderActorY = (self?.root.position.y || 0).toFixed(6);
    canvas.dataset.cameraFocusErrorY = (camera.target.y - (self?.root.position.y || 0) - (self?.vehicle ? self.vehicle.height * .55 : .28)).toFixed(6);
    canvas.dataset.observedGrounded = String(self?.motionSample?.grounded || false);
    canvas.dataset.observedAirGap = (self?.motionSample?.airGap ?? Infinity).toFixed(6);
    canvas.dataset.mechHeight = (self?.vehicle?.height || 0).toFixed(3);
    canvas.dataset.mechRadius = (self?.vehicle?.radius || 0).toFixed(3);
    const skate = self?.imported?.skate?.diagnostics;
    const head = self?.imported?.steadyHead?.diagnostics;
    const drawnHead = self?.imported?.steadyHead?.renderedCenter();
    const headScreen = drawnHead && Vector3.Project(drawnHead, Matrix.Identity(), scene.getTransformMatrix(), camera.viewport.toGlobal(engine.getRenderWidth(), engine.getRenderHeight()));
    canvas.dataset.headScreenPosition = headScreen ? `${headScreen.x.toFixed(4)},${headScreen.y.toFixed(4)}` : '';
    canvas.dataset.headStabilized = String(head?.active || false);
    canvas.dataset.headStabilityBlend = (head?.blend || 0).toFixed(3);
    canvas.dataset.headMotionMode = head?.mode || 'native';
    canvas.dataset.headStrokeBob = (head?.bobOffsetY || 0).toFixed(6);
    canvas.dataset.headActorPosition = head?.position?.map(value => value.toFixed(6)).join(',') || '';
    const playingNativeAir = self?.nativeMotion?.available && Number.isFinite(self.nativeMotion.frame);
    const playingSkate = !playingNativeAir && !!skate?.active;
    canvas.dataset.skateActive = String(playingSkate);
    canvas.dataset.skateState = skate?.state || 'unavailable';
    canvas.dataset.skateSpeed = (skate?.speed || 0).toFixed(3);
    canvas.dataset.skateRate = (skate?.rate || 0).toFixed(3);
    canvas.dataset.skateBlend = (playingNativeAir ? 0 : skate?.blend || 0).toFixed(3);
    canvas.dataset.footwearSockets = (self?.ownerGear?.footwear || []).map(entry => entry.fit.socket.name.match(/(\d{3})_object_/)?.[1] || '').join(',');
    canvas.dataset.shoeExhaust = String((self?.ownerGear?.footwear || []).filter(entry => entry.nativeEffects?.some(effect => effect.mesh.isEnabled())).length);
    canvas.dataset.nativeShoeEffects = (self?.ownerGear?.footwear || []).flatMap(entry => entry.nativeEffects || []).filter(effect => effect.mesh.isEnabled()).map(effect => effect.modelId).join(',');
    canvas.dataset.skateClip = skate?.clip || '';
    canvas.dataset.skatePhase = (skate?.phase || 0).toFixed(6);
    canvas.dataset.chaoMotion = playingSkate ? `ShadowSkate${skate.clip}` : self?.nativeMotion?.available && self.nativeMotion.nativeState ? `Native${self.nativeMotion.mode}${self.nativeMotion.nativeState}` : self?.imported?.walk && self.imported.current === self.imported.walk ? 'Walk' : 'Idle';
    canvas.dataset.chaoMotionState = self?.nativeMotion?.mode || 'Idle';
    canvas.dataset.chaoMotionAvailable = String(!!self?.nativeMotion?.available);
    canvas.dataset.chaoMotionFrame = Number.isFinite(self?.nativeMotion?.frame) ? self.nativeMotion.frame.toFixed(3) : '';
    canvas.dataset.chaoMotionRate = (self?.nativeMotion?.rate ?? 1).toFixed(3);
  });
  onReady();

  return {
    setSnapshot,
    handleEvent,
    clearInput: blur,
    setPaused(value) { paused = !!value; gaitControls.setPaused(paused); if (paused) blur(); },
    setTouchInput(dx, dz) { touch.dx = Math.max(-1, Math.min(1, dx)); touch.dz = Math.max(-1, Math.min(1, dz)); },
    dispose() {
      disposed = true;
      window.removeEventListener('keydown', keyDown);
      window.removeEventListener('keyup', keyUp);
      window.removeEventListener('blur', blur);
      window.removeEventListener('resize', resize);
      canvas.removeEventListener('pointerdown', pointerDown);
      canvas.removeEventListener('contextmenu', suppressContextMenu);
      resizeObserver.disconnect();
      gaitControls.dispose();
      scene.dispose();
      engine.dispose();
    },
  };
}
