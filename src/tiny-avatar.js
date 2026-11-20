// These are new phone atlases rendered from the original SA2 Chaos models and
// original Idle/Walk poses. They are not original GBA sprites or new Chao rigs.
const variants = ['neutral', 'hero', 'dark'];
const atlases = new Map(variants.map(variant => {
  const image = new Image();
  image.src = `/api/assets/tiny-${variant}-chaos.png`;
  return [variant, image];
}));
const flame = new Image();
flame.src = '/api/assets/chao-flame.png';
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

export function chaosVariant(profile) {
  const form = String(profile?.form || '').toLowerCase();
  if (!form.includes('chaos')) return null;
  if (form.includes('hero')) return 'hero';
  if (form.includes('dark')) return 'dark';
  if (form.includes('neutral')) return 'neutral';
  if (profile.alignment >= 60) return 'hero';
  if (profile.alignment <= -60) return 'dark';
  return 'neutral';
}

export function chaoFormLabel(profile) {
  const variant = chaosVariant(profile);
  if (variant) return `${variant[0].toUpperCase()}${variant.slice(1)} Chaos Chao`;
  const form = String(profile?.form || '').toLowerCase();
  return ({ normal: 'Neutral Chao', hero: 'Hero Chao', dark: 'Dark Chao' })[form] || 'Chao';
}

export function nativeDirection(x, y, previous = 0) {
  if (!x && !y) return previous;
  // Source "right" faces screen-left in the rendered atlas. Map the visible
  // nose to the server's screen-space movement rather than mirroring the art.
  if (y > 0) return x > 0 ? 7 : x < 0 ? 1 : 0;
  if (y < 0) return x > 0 ? 5 : x < 0 ? 3 : 4;
  return x > 0 ? 6 : 2;
}

export function drawTinyAvatar(context, profile, x, y, size, options = {}) {
  const variant = chaosVariant(profile);
  const image = atlases.get(variant);
  const time = reducedMotion.matches ? 0 : options.time || 0;
  if (!image?.complete || image.naturalWidth !== 768 || image.naturalHeight !== 768) {
    // A neutral UI marker is used until verified art is available; a normal
    // saved Chao must never be represented by the Dark Chaos brand mascot.
    context.save();
    context.fillStyle = '#24445a'; context.strokeStyle = '#94b7c7'; context.lineWidth = 1.5;
    context.beginPath(); context.arc(x, y, size * 0.27, 0, Math.PI * 2); context.fill(); context.stroke();
    context.fillStyle = '#c9e0e5'; context.font = `${Math.max(12, size * 0.3)}px system-ui`;
    context.textAlign = 'center'; context.textBaseline = 'middle'; context.fillText('◈', x, y);
    context.restore();
    return false;
  }
  const walking = !!options.walking;
  const column = (walking ? 4 : 0) + Math.floor(time / (walking ? 145 : 240)) % 4;
  const direction = Number.isInteger(options.direction) && options.direction >= 0 && options.direction < 8 ? options.direction : 0;
  context.save();
  context.imageSmoothingEnabled = true;
  context.drawImage(image, column * 96, direction * 96, 96, 96, x - size / 2, y - size / 2, size, size);
  if (variant === 'dark' && flame.complete && flame.naturalWidth === 512 && flame.naturalHeight === 32) {
    const frame = Math.floor(time * 18 / 1000) % 16;
    context.globalCompositeOperation = 'lighter';
    context.drawImage(flame, frame * 32, 0, 32, 32, x - size * 0.17, y - size * 0.66, size * 0.34, size * 0.34);
  }
  context.restore();
  return true;
}
