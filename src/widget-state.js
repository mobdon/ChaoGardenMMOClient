// Display projection only. Missing canonical fields remain unknown.
export const WIDGET_SKILLS = [
  ['swim', 'Swim'], ['fly', 'Fly'], ['run', 'Run'], ['power', 'Power'],
  ['stamina', 'Stam'], ['luck', 'Luck'], ['intelligence', 'Int'],
];
const number = value => Number.isFinite(value) && value >= 0 ? value : null;

export function widgetSnapshotView(profile) {
  const needs = Object.fromEntries(['energy', 'hunger', 'happiness'].map(key => {
    const value = number(profile?.[key]);
    return [key, value !== null && value <= 100 ? value : null];
  }));
  const stats = WIDGET_SKILLS.map(([key, label]) => {
    const source = profile?.stats?.[key];
    return { key, label, level: number(source?.level), points: number(source?.points), grade: number(source?.grade) };
  });
  const activity = profile?.watch?.activity;
  const routine = !profile ? 'Connect to see your Chao'
    : ({ wander: 'Exploring the garden', rest: 'Resting in the garden', idle: 'Pausing in the garden' })[activity]
      || (profile.watch === null ? 'Garden routine is off' : 'Garden activity not reported');
  // The legacy Rust service has no agreed durable AFK snapshot. Do not guess
  // one from a similarly named field or turn omitted data into "no job".
  return { needs, stats, rings: number(profile?.rings), routine, training: 'AFK status unavailable' };
}

export async function requestWidgetJson(request, path, options = {}, { signal, timeoutMs = 10000 } = {}) {
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  let timer;
  try {
    const response = await Promise.race([
      request(path, { ...options, signal: controller.signal, cache: 'no-store', credentials: 'omit', redirect: 'error' })
        .then(async result => ({ ok: result.ok, status: result.status, data: await result.json() })),
      new Promise((_, reject) => {
        timer = setTimeout(() => {
          controller.abort();
          const error = new Error('The garden did not respond in time. Retry when it is reachable.');
          error.name = 'TimeoutError'; reject(error);
        }, timeoutMs);
      }),
    ]);
    return response;
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  }
}

export async function readWidgetSession(request, token, options = {}) {
    const response = await requestWidgetJson(request, '/api/session', { method: 'POST',
      headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token, resumeOnly: true }) }, options);
    if (!response.ok || typeof response.data?.player?.id !== 'string' || typeof response.data?.token !== 'string') {
      const error = new Error(response.status === 401 || response.status === 404
        ? 'This save could not be resumed. Load it at the garden title, then return here.'
        : 'The garden could not confirm this saved Chao. Retry in a moment.');
      error.retry = response.status >= 500;
      throw error;
    }
    return response.data;
}
