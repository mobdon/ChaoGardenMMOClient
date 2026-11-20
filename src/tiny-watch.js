import { drawTinyAvatar, nativeDirection } from './tiny-avatar.js';

// Presentation only; the canonical server supplies positions, needs and care.
export function createTinyWatch({ send }) {
  const canvas = document.querySelector('#tiny-watch-canvas');
  const context = canvas.getContext('2d');
  const status = document.querySelector('#tiny-watch-status');
  const copy = document.querySelector('#tiny-watch-copy');
  const toggle = document.querySelector('#tiny-watch-toggle');
  const care = document.querySelector('#tiny-watch-care');
  const needs = document.querySelector('#tiny-watch-needs');
  const careButtons = [...care.querySelectorAll('button')];
  const needRows = [['energy', 'Energy'], ['hunger', 'Fullness'], ['happiness', 'Happiness']].map(([key, label]) => {
    const line = document.createElement('span');
    const name = document.createElement('span'); name.textContent = label;
    const meter = document.createElement('meter'); meter.min = 0; meter.max = 100; meter.setAttribute('aria-label', label);
    const value = document.createElement('small');
    line.append(name, meter, value); line.hidden = true; needs.append(line);
    return { key, line, meter, value, previous: null };
  });
  let profile = null, connected = false, busy = false, pending = null;
  let profileId = null, origin = null, session = null, previous = null, direction = 0;
  let pendingTimer, lastMovedAt = -Infinity, feedback = '', canvasLabel = '';

  // Cache the static garden bitmap; frame drawing creates no patterns/gradients.
  const background = document.createElement('canvas');
  background.width = canvas.width; background.height = canvas.height;
  const backgroundContext = background.getContext('2d');
  const grass = new Image();
  function paintBackground() {
    backgroundContext.fillStyle = '#24473a'; backgroundContext.fillRect(0, 0, background.width, background.height);
    if (grass.complete && grass.naturalWidth) {
      backgroundContext.save(); backgroundContext.globalAlpha = 0.48;
      backgroundContext.fillStyle = backgroundContext.createPattern(grass, 'repeat');
      backgroundContext.fillRect(0, 0, background.width, background.height); backgroundContext.restore();
    }
    const fade = backgroundContext.createRadialGradient(320, 180, 40, 320, 180, 380);
    fade.addColorStop(0, '#10243400'); fade.addColorStop(1, '#071c2be0');
    backgroundContext.fillStyle = fade; backgroundContext.fillRect(0, 0, background.width, background.height);
  }
  grass.onload = paintBackground;
  grass.src = '/api/assets/hub-grass.png';
  paintBackground();

  function setText(element, value) {
    if (element.textContent !== value) element.textContent = value;
  }
  function supported() { return !!profile && Object.hasOwn(profile, 'watch'); }
  function clearPending() { clearTimeout(pendingTimer); pending = null; }

  function render() {
    const watching = !!profile?.watch && connected;
    const pendingStatus = pending?.action === 'watch-start' ? 'Joining the garden…'
      : pending?.action === 'watch-stop' ? 'Taking control…' : pending ? 'Waiting for your Chao…' : null;
    setText(status, !connected ? 'Visit paused' : !supported() ? 'Server update needed'
      : pendingStatus || (watching ? ({ idle: 'Taking it in', wander: 'Exploring', rest: 'Resting' })[profile.watch.activity] || 'In the garden' : 'Ready to visit'));
    toggle.disabled = !connected || !supported() || busy || !!pending;
    setText(toggle, pending ? 'One moment…' : !connected ? 'Connect a saved Chao' : !supported() ? 'Server update needed'
      : watching ? 'Take control' : busy ? 'Finish your round first' : 'Let my Chao play');
    care.hidden = !watching;
    for (const button of careButtons) button.disabled = !!pending || !connected || busy;
    const description = !connected
      ? 'Reconnect to check on your Chao. Their routine can continue while the garden server is running.'
      : !supported() ? 'This garden server needs an update before it can host your Chao’s routine.'
        : watching ? profile.watch.activity === 'rest'
          ? 'A little rest. Your Chao can stay here while you are away, as long as the garden server is running.'
          : 'Your Chao keeps their own little routine and stays visible to others while you are away. The garden server must stay running.'
          : 'Let your Chao wander, pause, and rest in the shared garden, even after you close the app.';
    setText(copy, feedback ? `${feedback} ${description}` : description);
    for (const row of needRows) {
      const value = profile?.[row.key];
      row.line.hidden = !Number.isFinite(value);
      if (!Number.isFinite(value) || value === row.previous) continue;
      row.previous = value; row.meter.value = value;
      setText(row.value, `${Math.round(value)}%`);
    }
    const label = `${profile?.name || 'Your Chao'} · ${status.textContent}`;
    if (label !== canvasLabel) { canvas.setAttribute('aria-label', label); canvasLabel = label; }
  }

  function action(action, extra = {}) {
    if (!connected || !supported() || pending || busy) return;
    feedback = '';
    pending = { action, profileId, sessionId: profile.watch?.id || null,
      previousCareAt: profile.watch?.lastCare?.at, kind: extra.kind };
    render(); send({ type: 'action', action, ...extra });
    clearTimeout(pendingTimer);
    pendingTimer = setTimeout(() => {
      pending = null;
      feedback = 'The garden has not confirmed that action. Wait a moment and try again.';
      render();
    }, 2500);
  }
  toggle.addEventListener('click', () => {
    if (!profile || busy) return;
    action(profile.watch ? 'watch-stop' : 'watch-start', profile.watch ? { sessionId: profile.watch.id } : {});
  });
  care.addEventListener('click', event => {
    const button = event.target.closest('[data-care]');
    if (button && profile?.watch) action('watch-care', { sessionId: profile.watch.id, kind: button.dataset.care });
  });

  function confirmed(next) {
    if (!pending || next?.id !== pending.profileId || !Object.hasOwn(next, 'watch')) return false;
    if (pending.action === 'watch-start') return !!next.watch && next.watch.id !== pending.sessionId;
    if (pending.action === 'watch-stop') return next.watch === null;
    const lastCare = next.watch?.lastCare;
    return next.watch?.id === pending.sessionId && lastCare?.kind === pending.kind
      && Number.isFinite(lastCare.at) && lastCare.at !== pending.previousCareAt;
  }

  return {
    update(next, connection) {
      const nextId = next?.id || null, nextSession = next?.watch?.id || null;
      const changedProfile = nextId !== profileId, disconnected = connected && !connection.connected;
      if (connection.connected && confirmed(next)) {
        const kind = pending.kind;
        clearPending();
        feedback = kind === 'pet' ? 'Pet received.' : kind === 'rest' ? 'Rest requested.' : kind === 'wake' ? 'Wake-up received.' : '';
      }
      if (changedProfile || nextSession !== session || disconnected || !origin) {
        origin = next && Number.isFinite(next.x) && Number.isFinite(next.z) ? { x: next.x, z: next.z } : null;
        previous = null; direction = 0; lastMovedAt = -Infinity;
        clearPending();
        if (changedProfile || disconnected) feedback = '';
      }
      profileId = nextId; session = nextSession;
      if (connection.connected && previous && next && Number.isFinite(next.x) && Number.isFinite(next.z)
        && (next.x !== previous.x || next.z !== previous.z)) {
        direction = nativeDirection(next.x - previous.x, next.z - previous.z, direction);
        lastMovedAt = performance.now();
      }
      previous = connection.connected && next ? { x: next.x, z: next.z } : null;
      profile = next; connected = connection.connected; busy = connection.busy;
      render();
    },
    handleNotice(message) {
      if (!pending || message.kind !== 'error') return;
      clearPending(); feedback = message.text || 'The garden could not complete that action.'; render();
    },
    draw(time) {
      context.drawImage(background, 0, 0);
      const x = Math.max(70, Math.min(570, 320 + ((profile?.x || 0) - (origin?.x || 0)) * 32));
      const y = Math.max(85, Math.min(285, 190 + ((profile?.z || 0) - (origin?.z || 0)) * 22));
      context.fillStyle = '#0004'; context.beginPath(); context.ellipse(x, y + 49, 31, 9, 0, 0, Math.PI * 2); context.fill();
      const walking = connected && profile?.watch?.activity === 'wander' && time >= lastMovedAt && time - lastMovedAt < 350;
      drawTinyAvatar(context, profile, x, y, 145, { time: connected ? time : 0, direction, walking });
      if (connected && profile?.watch?.activity === 'rest') {
        context.fillStyle = '#f5dfa4'; context.font = '22px system-ui'; context.fillText('z z', x + 38, y - 46);
      }
    },
  };
}
