// Shared by browser rendering and the offline authoritative terrain builder.
// Positions are in Babylon's left-handed glTF coordinate system, not Blender Z-up.
export const NATIVE_GARDENS = {
  garden: {
    asset: 'garden', scale: 0.1,
    position: [-2.0968506171830086, 0.060817185301374366, 8.529567845606323],
    excludedNames: ['0010_object_01803DEC', '0011_object_0180388C'],
    exit: {
      meshes: ['0007_object_0180555C_primitive0', '0024_object_017FE994_primitive0'],
      bounds: { min: [-11.9132, 0.0406, -12.2833], max: [-9.8232, 3.6206, -11.3995] },
      reachable: true,
    },
  },
  hero: {
    asset: 'hero-garden', scale: 0.1,
    position: [-3.6033796135917697, -3.3800047945335905, 15.041726085593204],
    excludedNames: ['0004_object_0176459C', '0006_object_017640E4', '0005_object_0176442C', '0020_object_0175FE7C', '0021_object_017602D4'],
    exit: {
      meshes: ['0022_object_0175FA24_primitive0'],
      bounds: { min: [14.0326, -3.4, 32.679], max: [16.2452, 0.37, 34.8912] },
      // The native doorway is beyond the current authoritative +/-22 field.
      // Its click works when visible; Home remains the accessible return.
      reachable: false,
    },
  },
  dark: {
    asset: 'dark-garden', scale: 0.1,
    // The Dark stage's decorative bounds are far from its playable courtyard.
    // This places that courtyard's broad flat area around player spawn (0,6).
    position: [-5.737651115485207, 0, 0.987035526051784],
    excludedNames: ['0000_object_01724B2C', '0002_object_01724674', '0001_object_017249BC'],
    exit: {
      meshes: ['0028_object_0171BA54_primitive1'],
      bounds: { min: [10.8088, -0.1, 7.4675], max: [15.8049, 2.5, 9.2987] },
      reachable: true,
    },
  },
};

// Exit bounds are measured from the native dark recess triangles after these
// exact stage transforms. They adapt a click target, not SA2 trigger volumes.
// See archive/old-hub/docs/NATIVE-GARDEN-EXITS.md (git history, last at 420f1eb); all coordinates remain canonical world units.

export function isNativeTerrainMesh(name, sizeX, sizeZ, vertexCount, zone) {
  const layout = NATIVE_GARDENS[zone];
  return !!layout && vertexCount > 0 && sizeX <= 80 && sizeZ <= 80
    && !layout.excludedNames.some(excluded => name.includes(excluded));
}
