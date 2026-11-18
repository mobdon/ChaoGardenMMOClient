// Shared by browser rendering and the offline authoritative terrain builder.
// Positions are in Babylon's left-handed glTF coordinate system, not Blender Z-up.
export const NATIVE_GARDENS = {
  garden: {
    asset: 'garden', scale: 0.1,
    position: [-2.0968506171830086, 0.060817185301374366, 8.529567845606323],
    excludedNames: ['0010_object_01803DEC', '0011_object_0180388C'],
  },
  hero: {
    asset: 'hero-garden', scale: 0.1,
    position: [-3.6033796135917697, -3.3800047945335905, 15.041726085593204],
    excludedNames: ['0004_object_0176459C', '0006_object_017640E4', '0005_object_0176442C', '0020_object_0175FE7C', '0021_object_017602D4'],
  },
  dark: {
    asset: 'dark-garden', scale: 0.1,
    // The Dark stage's decorative bounds are far from its playable courtyard.
    // This places that courtyard's broad flat area around player spawn (0,6).
    position: [-5.737651115485207, 0, 0.987035526051784],
    excludedNames: ['0000_object_01724B2C', '0002_object_01724674', '0001_object_017249BC'],
  },
};

export function isNativeTerrainMesh(name, sizeX, sizeZ, vertexCount, zone) {
  const layout = NATIVE_GARDENS[zone];
  return !!layout && vertexCount > 0 && sizeX <= 80 && sizeZ <= 80
    && !layout.excludedNames.some(excluded => name.includes(excluded));
}
