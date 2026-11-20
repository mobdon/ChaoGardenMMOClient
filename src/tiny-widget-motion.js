// Maps received garden positions onto the widget's grass. This is a view of
// Rust movement, with no wandering AI, position extrapolation or offline gains.
export function createTinyWidgetMotion() {
  let origin = null, latest = null;
  return {
    observe(profile) {
      if (!profile?.id || !Number.isFinite(profile.x) || !Number.isFinite(profile.z)) {
        origin = latest = null; return;
      }
      if (origin?.id !== profile.id || origin.zone !== profile.zone) origin = { id: profile.id, zone: profile.zone, x: profile.x, z: profile.z };
      latest = { x: profile.x, z: profile.z };
    },
    placement(base, width, height) {
      if (!origin || !latest || width <= 0 || height <= 0) return base;
      const scale = Math.min(width / 176, height / 160);
      const x = Math.max(width * .38, Math.min(width * .84, base.groundX + (latest.x - origin.x) * 8.8 * scale));
      const y = Math.max(height * .35, Math.min(height * .79, base.groundY + (latest.z - origin.z) * 5.5 * scale));
      return { ...base, left: x, top: base.top + y - base.groundY, groundX: x, groundY: y };
    },
  };
}
