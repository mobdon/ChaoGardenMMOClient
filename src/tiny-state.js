// Cached presentation is never sent back as gameplay state or used for rewards.
export const TINY_CACHE_KEY = 'chao-tiny-view-v1';
export const TINY_GAMES = ['memory', 'rock_paper_scissors', 'bounce', 'ball', 'search'];
const skills = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];

export function cachedProfile(profile) {
  if (!profile || typeof profile.id !== 'string') return null;
  const view = { id: profile.id, stats: {} };
  for (const key of ['name', 'form', 'equipped']) if (typeof profile[key] === 'string') view[key] = profile[key].slice(0, 100);
  for (const key of ['alignment', 'rings', 'energy', 'hunger', 'happiness']) if (Number.isFinite(profile[key])) view[key] = profile[key];
  for (const name of skills) {
    const stat = profile.stats?.[name];
    if (stat && ['level', 'points', 'grade'].every(key => Number.isFinite(stat[key]))) {
      view.stats[name] = { level: stat.level, points: stat.points, grade: stat.grade };
    }
  }
  if (Array.isArray(profile.ownedCosmetics)) view.ownedCosmetics = profile.ownedCosmetics.filter(id => typeof id === 'string').slice(0, 100);
  if (profile.gear && typeof profile.gear === 'object') view.gear = Object.fromEntries(['footwear', 'hands'].filter(key => typeof profile.gear[key] === 'string').map(key => [key, profile.gear[key]]));
  return view;
}

export function readTinyCache(storage, profileId) {
  try {
    const entry = JSON.parse(storage.getItem(TINY_CACHE_KEY));
    if (entry?.version !== 1 || entry.player?.id !== profileId || !Number.isFinite(entry.syncedAt)) return null;
    const player = cachedProfile(entry.player);
    return player ? { player, syncedAt: entry.syncedAt } : null;
  } catch { return null; }
}

export function writeTinyCache(storage, profile, syncedAt) {
  const player = cachedProfile(profile);
  if (!player || !Number.isFinite(syncedAt)) return;
  try { storage.setItem(TINY_CACHE_KEY, JSON.stringify({ version: 1, player, syncedAt })); } catch { /* Private mode/quota: live state still works. */ }
}

export function durableTinyResult(result, round = null) {
  return result?.saved === true && typeof result.id === 'string' && result.id.length > 0
    && TINY_GAMES.includes(result.gameId) && Number.isSafeInteger(result.rings) && result.rings >= 0
    && (!round || (result.id === round.id && result.gameId === round.gameId));
}

export function observedTime(value) {
  return Number.isFinite(value) ? new Date(value).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : 'not yet received';
}
