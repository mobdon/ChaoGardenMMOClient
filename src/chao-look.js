import { Color3 } from '@babylonjs/core';

// Native Chao colour (SA2B GC ChaoMain.rel): AL_MaterialInit calls
// AL_MatSetChaoColor (fn_60_17BF4) only when ColorNum != 0, and it replaces the
// first diffuse word of the materials on a fixed node list with
// colorTable[ColorNum] (rodata 0x1D60). Eyes and mouth are not coloured.
//
// The table and node list are read from the owner's own disc by
// scripts/extract-chao-look.py into .local/imports/chao-look.json; nothing from
// the game is in Git. Jewel, shiny and monotone drawing are not traced yet and
// are not presented.

let manifest = null;
let manifestTask = null;

export function loadChaoLookManifest(fetcher = fetch) {
  if (manifestTask) return manifestTask;
  manifestTask = (async () => {
    try {
      const response = await fetcher('/api/assets/chao-look.json', { cache: 'no-store' });
      manifest = response.ok ? await response.json() : null;
    } catch {
      manifest = null;
    }
    return manifest;
  })();
  return manifestTask;
}

// Native tree index of the node a mesh hangs from (SAIO names nodes "NNN_...").
export function nodeIndexOf(node) {
  for (let current = node; current; current = current.parent) {
    const match = /(?:^|-)(\d{3})_/.exec(current.name || '');
    if (match) return Number(match[1]);
  }
  return null;
}

export function chaoColour(look, table = manifest?.colorTable) {
  const index = look?.color;
  if (!Number.isInteger(index) || index === 0 || !Array.isArray(table)) return null;
  const entry = table[index];
  return Array.isArray(entry) && entry.length >= 3 ? entry.map(value => value / 255) : null;
}

export function applyChaoLook(root, look) {
  const colour = chaoColour(look);
  const nodes = new Set(manifest?.colorNodes || []);
  if (!root || !colour || !nodes.size) return false;
  for (const mesh of root.getChildMeshes(false)) {
    if (!mesh.material || !('albedoColor' in mesh.material) || !nodes.has(nodeIndexOf(mesh))) continue;
    if (!mesh.metadata?.chaoLookOwnMaterial) {
      mesh.material = mesh.material.clone(`${mesh.material.name}-colour`);
      mesh.metadata = { ...(mesh.metadata || {}), chaoLookOwnMaterial: true };
      (root.metadata ||= {}).ownedMaterials ||= [];
      root.metadata.ownedMaterials.push(mesh.material);
    }
    mesh.material.albedoColor = new Color3(colour[0], colour[1], colour[2]);
  }
  return true;
}
