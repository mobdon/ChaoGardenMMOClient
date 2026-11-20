// Original A2NE background from the fingerprint-gated Rust exporter.
// This adapter only draws authoritative data and calls the existing Chao renderer.
// It owns no simulation, rewards, training, credentials, requests or asset routes.
export function drawNativeGarden(context, { background, profile, drawAvatar, x = 120, y = 85,
  time = 0, direction = 0, walking = false, panel = true }) {
  if (!background?.complete || background.naturalWidth !== 240 || background.naturalHeight !== 160) return false;
  const width = panel ? 240 : 171;
  const scale = Math.min(context.canvas.width / width, context.canvas.height / 160);
  const left = (context.canvas.width - width * scale) / 2;
  const top = (context.canvas.height - 160 * scale) / 2;
  context.save();
  context.clearRect(0, 0, context.canvas.width, context.canvas.height);
  context.translate(left, top); context.scale(scale, scale);
  context.imageSmoothingEnabled = false;
  context.drawImage(background, 0, 0, width, 160, 0, 0, width, 160);
  // A Chao is only shown when an actual snapshot and its renderer are supplied.
  // Positions are supplied by the existing adapter; there is no local wandering.
  if (profile && drawAvatar) {
    context.save(); context.beginPath(); context.rect(0, 0, 171, 160); context.clip();
    drawAvatar(context, profile, x, y, 54, { time, direction, walking });
    context.restore();
  }
  if (panel) {
    context.fillStyle = '#091c30'; context.font = '6px monospace';
    context.textAlign = 'right'; context.textBaseline = 'middle';
    const value = (number, suffix = '') => Number.isFinite(number) ? `${Math.trunc(number)}${suffix}` : '—';
    context.fillText(value(profile?.happiness, '%'), 234, 36);
    context.fillText(value(profile?.hunger, '%'), 234, 52);
    for (const [index, stat] of ['swim', 'fly', 'run', 'power', 'stamina'].entries()) {
      context.fillText(value(profile?.stats?.[stat]?.level), 234, 68 + index * 16);
    }
    context.fillText(value(profile?.rings), 234, 155);
  }
  context.restore();
  return true;
}
