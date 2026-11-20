// SA2's torch_blue atlas stores additive radiance in RGB with mostly binary
// alpha. On a transparent phone canvas that alpha creates a dark matte.
// Convert only the in-memory presentation to coverage, as the native Android
// widget also does. Normalize straight RGB so the emitted premultiplied color
// is retained rather than multiplying the dim native RGB by coverage twice.
export function flameEmissionCoverage(pixels) {
  if (!(pixels instanceof Uint8Array || pixels instanceof Uint8ClampedArray) || pixels.length % 4 !== 0) {
    throw new TypeError('Flame pixels must be an RGBA byte buffer');
  }
  const result = new Uint8ClampedArray(pixels.length);
  for (let i = 0; i < pixels.length; i += 4) {
    const peak = Math.max(pixels[i], pixels[i + 1], pixels[i + 2]);
    if (!peak || !pixels[i + 3]) continue;
    result[i] = Math.round(pixels[i] * 255 / peak);
    result[i + 1] = Math.round(pixels[i + 1] * 255 / peak);
    result[i + 2] = Math.round(pixels[i + 2] * 255 / peak);
    result[i + 3] = Math.round(pixels[i + 3] * peak / 255);
  }
  return result;
}

const layers = new WeakMap();

// Decode the verified original Image once, retaining all sixteen 32px frames,
// their orientation and source URL. No native PNG or shared 3D material changes.
export function getTinyFlameLayer(image) {
  if (!image?.complete || image.naturalWidth !== 512 || image.naturalHeight !== 32) return null;
  const source = image.currentSrc || image.src;
  const cached = layers.get(image);
  if (cached?.source === source) return cached.layer;
  const document = image.ownerDocument || globalThis.document;
  if (!document?.createElement) return null;
  let layer = null;
  try {
    const canvas = document.createElement('canvas');
    canvas.width = 512; canvas.height = 32;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (context) {
      context.drawImage(image, 0, 0);
      const decoded = context.getImageData(0, 0, 512, 32);
      decoded.data.set(flameEmissionCoverage(decoded.data));
      context.putImageData(decoded, 0, 0);
      layer = canvas;
    }
  } catch {
    // An unavailable/tainted source cannot produce a fabricated replacement.
  }
  layers.set(image, { source, layer });
  return layer;
}
