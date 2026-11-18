export const SESSION_KEY = 'chao-garden-session-v1';
export const SAVE_SLOTS_KEY = 'chao-garden-slots-v1';

// This index remembers credentials for this browser. Gameplay data stays on the server.
export function createSaveSlots(storage) {
  function list() {
    let rows = [];
    try {
      const value = JSON.parse(storage.getItem(SAVE_SLOTS_KEY) || '[]');
      if (Array.isArray(value)) rows = value.filter(row => row && typeof row.token === 'string' && /^[a-f0-9]{64}$/i.test(row.token));
    } catch { /* A legacy session can still recover an unreadable index. */ }
    const legacy = storage.getItem(SESSION_KEY);
    if (legacy && /^[a-f0-9]{64}$/i.test(legacy) && !rows.some(row => row.token === legacy)) {
      rows.push({ token: legacy, name: 'Your saved Chao', lastPlayedAt: 0 });
    }
    return [...new Map(rows.map(row => [row.token, row])).values()].sort((a, b) => (b.lastPlayedAt || 0) - (a.lastPlayedAt || 0));
  }

  function record(token, player, savedAt) {
    const rows = list();
    const previous = rows.find(row => row.token === token) || {};
    const row = {
      ...previous, token, id: player.id, name: player.name,
      form: player.form, alignment: player.alignment, rings: player.rings,
      admin: !!player.admin, lastPlayedAt: Date.now(),
      ...(savedAt === undefined ? {} : { savedAt }),
    };
    storage.setItem(SAVE_SLOTS_KEY, JSON.stringify([row, ...rows.filter(item => item.token !== token)]));
    storage.setItem(SESSION_KEY, token);
    return row;
  }
  return { list, record };
}
