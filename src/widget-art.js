// Presentation subjects only. Preview artwork has no gameplay identity, owner
// role, wallet, needs or stats; a canonical profile always takes precedence.
const previews = Object.freeze({
  dark: Object.freeze({ form: 'dark-chaos', alignment: -100 }),
  neutral: Object.freeze({ form: 'neutral-chaos', alignment: 0 }),
  hero: Object.freeze({ form: 'hero-chaos', alignment: 100 }),
});
export const WIDGET_EMERALD_COLORS = Object.freeze(['cyan', 'blue', 'yellow', 'red', 'green', 'purple', 'white']);

export function widgetArtSubject(profile, variant = 'dark') {
  return profile !== null && profile !== undefined
    ? { subject: profile, preview: false }
    : { subject: Object.hasOwn(previews, variant) ? previews[variant] : previews.dark, preview: true };
}

export function widgetCanvasPlacement(width, height, variant) {
  const mapScale = Math.min(width / 176, height / 160);
  const groundX = width / 2 + 176 * mapScale * 0.12;
  const groundY = (height - 160 * mapScale) / 2 + 160 * mapScale * 0.68;
  // Front Idle alpha ends at row84 in Dark/Hero and row83 in Neutral. The
  // shared renderer draws the whole96px cell into [34,50,326,342]. Pin the
  // original feet to grass. Scale the complete portrait with the displayed
  // garden, keeping the body, flame and full emerald orbit inside the scene.
  const footRatio = (50 + (variant === 'neutral' ? 83 : 84) / 96 * 292) / 360;
  // A 32px presentation footprint at the garden's native resolution keeps
  // the visible body around 20px tall, comparable to a small handheld sprite.
  const size = 32 * mapScale;
  return { left: groundX, top: Math.max(0, groundY - size * footRatio), size, groundX, groundY, footRatio };
}

export function widgetEmeraldLayout(profile, { canonical = false, x, y, size, time = 0 } = {}) {
  if (!canonical || profile?.admin !== true) return [];
  if (![x, y, size, time].every(Number.isFinite) || size <= 0) return [];
  // Same front-portrait anchors as the native Android original-art renderer.
  // Every mesh render keeps a square cell; transparent padding preserves the
  // original geometry's aspect. Chest stays fixed while the seven orbit.
  const scale = size / 208;
  const orbit = WIDGET_EMERALD_COLORS.map((color, index) => {
    const angle = index * Math.PI * 2 / 7 + time / 1500;
    const depth = Math.sin(angle);
    return { color, layer: depth < 0 ? 'far' : 'near', x: x + Math.cos(angle) * 106 * scale,
      y: y + (32 + depth * 48) * scale, size: 24 * scale };
  });
  return [...orbit, { color: 'cyan', layer: 'chest', x, y: y + 20 * scale, size: 22 * scale }];
}

export function widgetEmeraldImagesReady(images) {
  return WIDGET_EMERALD_COLORS.every(color => {
    const image = images.get(color);
    return image?.complete && image.naturalWidth === 64 && image.naturalHeight === 64;
  });
}

export function drawWidgetEmeraldLayer(context, images, layout, layer) {
  context.save(); context.globalCompositeOperation = 'source-over'; context.imageSmoothingEnabled = true;
  for (const prop of layout) {
    if (prop.layer !== layer) continue;
    context.drawImage(images.get(prop.color), prop.x - prop.size / 2, prop.y - prop.size / 2, prop.size, prop.size);
  }
  context.restore();
}
