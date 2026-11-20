import { chaosVariant, drawTinyAvatar, nativeDirection } from './tiny-avatar.js';
import { drawWidgetEmeraldLayer, WIDGET_EMERALD_COLORS, widgetEmeraldImagesReady, widgetEmeraldLayout } from './widget-art.js';

// Presentation only; the canonical server supplies positions, needs and care.
export function createTinyWatch({ send }) {
  const canvas = document.querySelector('#tiny-watch-canvas');
  // The original 240x160 screen contains the 176px garden and the sidebar
  // beginning at x171. A larger backing store keeps the native atlas clear.
  canvas.width = 720; canvas.height = 480;
  canvas.style.aspectRatio = '3 / 2'; canvas.style.imageRendering = 'pixelated';
  const context = canvas.getContext('2d');
  const bodyLayer = document.createElement('canvas');
  bodyLayer.width = canvas.width; bodyLayer.height = canvas.height;
  const bodyContext = bodyLayer.getContext('2d');
  const reducedMotion = globalThis.matchMedia?.('(prefers-reduced-motion: reduce)');
  const status = document.querySelector('#tiny-watch-status');
  const copy = document.querySelector('#tiny-watch-copy');
  const toggle = document.querySelector('#tiny-watch-toggle');
  const care = document.querySelector('#tiny-watch-care');
  const needs = document.querySelector('#tiny-watch-needs');
  const artNote = document.createElement('p');
  artNote.id = 'tiny-watch-art-note'; artNote.className = 'tiny-watch-note';
  artNote.setAttribute('role', 'status'); canvas.after(artNote);
  const careButtons = [...care.querySelectorAll('button')];
  const needRows = [['energy', 'Energy'], ['hunger', 'Fullness'], ['happiness', 'Happiness']].map(([key, label]) => {
    const line = document.createElement('span');
    const name = document.createElement('span'); name.textContent = label;
    const meter = document.createElement('meter'); meter.min = 0; meter.max = 100; meter.setAttribute('aria-label', label);
    const value = document.createElement('small');
    line.append(name, meter, value); line.hidden = true; needs.append(line);
    return { key, line, meter, value, previous: null };
  });
  let profile = null, connected = false, reconciled = false, busy = false, pending = null;
  let profileId = null, origin = null, session = null, previous = null, direction = 0;
  let pendingTimer, lastMovedAt = -Infinity, feedback = '', canvasLabel = '', avatarDrawn = null;

  // These privately staged images are exported by the Rust format reader.
  // They contain labels and empty value fields, never a substitute save.
  const background = document.createElement('canvas');
  background.width = 240; background.height = 160;
  const backgroundContext = background.getContext('2d');
  const garden = new Image();
  const sidebar = new Image();
  const emeraldImages = new Map(WIDGET_EMERALD_COLORS.map(color => {
    const image = new Image(); image.onload = image.onerror = () => render();
    image.src = `/__widget-art/widget-emerald-${color}.png`; return [color, image];
  }));
  const imageReady = (image, width, height) => image.complete && image.naturalWidth === width && image.naturalHeight === height;
  const gardenReady = () => imageReady(garden, 176, 160);
  const sidebarReady = () => imageReady(sidebar, 72, 160);
  function paintBackground() {
    backgroundContext.imageSmoothingEnabled = false;
    backgroundContext.fillStyle = '#132632'; backgroundContext.fillRect(0, 0, 240, 160);
    if (gardenReady()) backgroundContext.drawImage(garden, 0, 0);
    if (sidebarReady()) backgroundContext.drawImage(sidebar, 171, 0);
    render();
  }
  garden.onload = garden.onerror = sidebar.onload = sidebar.onerror = paintBackground;
  garden.src = '/__widget-art/tiny-garden-a2ne.png';
  sidebar.src = '/__widget-art/tiny-garden-a2ne-sidebar-1.png';
  paintBackground();

  function setText(element, value) {
    if (element.textContent !== value) element.textContent = value;
  }
  function supported() { return !!profile && Object.hasOwn(profile, 'watch'); }
  function clearPending() { clearTimeout(pendingTimer); pending = null; }

  function render() {
    const watching = !!profile?.watch && reconciled;
    const pendingStatus = pending?.action === 'watch-start' ? 'Joining the garden…'
      : pending?.action === 'watch-stop' ? 'Taking control…' : pending ? 'Waiting for your Chao…' : null;
    setText(status, !reconciled ? 'Visit paused' : !supported() ? 'Server update needed'
      : pendingStatus || (watching ? ({ idle: 'Taking it in', wander: 'Exploring', rest: 'Resting' })[profile.watch.activity] || 'In the garden' : 'Ready to visit'));
    toggle.disabled = !connected || !supported() || busy || !!pending;
    setText(toggle, pending ? 'One moment…' : !reconciled ? 'Connect a saved Chao' : !connected ? 'Control is on another device' : !supported() ? 'Server update needed'
      : watching ? 'Take control' : busy ? 'Finish your round first' : 'Let my Chao play');
    care.hidden = !watching;
    for (const button of careButtons) button.disabled = !!pending || !connected || busy;
    const description = !reconciled
      ? 'Reconnect to check on your Chao. Their routine can continue while the garden server is running.'
      : !supported() ? 'This garden server needs an update before it can host your Chao’s routine.'
        : watching ? profile.watch.activity === 'rest'
          ? 'A little rest. Your Chao can stay here while you are away, as long as the garden server is running.'
          : 'Your Chao keeps their own little routine and stays visible to others while you are away. The garden server must stay running.'
          : 'Let your Chao wander, pause, and rest in the shared garden, even after you close the app.';
    setText(copy, feedback ? `${feedback} ${description}` : description);
    let art = !gardenReady() ? 'Original garden art unavailable.'
      : !sidebarReady() ? 'Original menu art unavailable.' : 'Original Tiny Garden art.';
    if (avatarDrawn === false) art += ' Original Chao art unavailable for this form.';
    if (profile?.admin === true && !widgetEmeraldImagesReady(emeraldImages)) art += ' Owner ornaments unavailable; the complete emerald group is hidden.';
    const positionAvailable = !!profile?.id && Number.isFinite(profile.x) && Number.isFinite(profile.z);
    setText(artNote, `${art} ${!profile?.id ? 'Connect to view your actual Chao.' : !positionAvailable ? 'Chao position unavailable; only reported stats are shown.'
      : reconciled ? 'Live values use the server’s adapted rules.' : 'Last synced Chao and values; reconnect before care.'}`);
    for (const row of needRows) {
      const value = profile?.[row.key];
      row.line.hidden = !Number.isFinite(value);
      if (!Number.isFinite(value) || value === row.previous) continue;
      row.previous = value; row.meter.value = value;
      setText(row.value, `${Math.round(value)}%`);
    }
    const label = `${profile?.name || 'Chao unavailable'} · ${status.textContent} · ${art}${positionAvailable ? '' : ' Canonical position unavailable.'}`;
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
      const nextReconciled = connection.reconciled ?? connection.connected;
      const changedProfile = nextId !== profileId, disconnected = reconciled && !nextReconciled;
      const lostControl = connected && !connection.connected;
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
      if (lostControl) clearPending();
      profileId = nextId; session = nextSession;
      if (nextReconciled && previous && next && Number.isFinite(next.x) && Number.isFinite(next.z)
        && (next.x !== previous.x || next.z !== previous.z)) {
        direction = nativeDirection(next.x - previous.x, next.z - previous.z, direction);
        lastMovedAt = performance.now();
      }
      previous = nextReconciled && next ? { x: next.x, z: next.z } : null;
      profile = next; connected = connection.connected; reconciled = nextReconciled; busy = connection.busy;
      render();
    },
    handleNotice(message) {
      if (!pending || message.kind !== 'error') return;
      clearPending(); feedback = message.text || 'The garden could not complete that action.'; render();
    },
    draw(time) {
      context.save(); context.scale(canvas.width / 240, canvas.height / 160);
      context.imageSmoothingEnabled = false;
      context.drawImage(background, 0, 0);
      if (sidebarReady()) {
        // Mood/Belly and these five LV fields are presentation projections of
        // the actual MMO profile, not native GBA progression calculations.
        context.fillStyle = '#16243a'; context.font = '6px monospace';
        context.textAlign = 'right'; context.textBaseline = 'alphabetic';
        const value = number => Number.isFinite(number) && number >= 0 ? String(Math.round(number)) : '—';
        context.fillText(profile?.id && typeof profile.name === 'string' ? profile.name.slice(0, 10) : '—', 236, 12);
        context.fillText(profile?.id ? reconciled ? 'LIVE' : 'CACHED' : 'NO CHAO', 236, 23);
        context.fillText(profile?.id ? value(profile.happiness) : '—', 236, 39);
        context.fillText(profile?.id ? value(profile.hunger) : '—', 236, 55);
        ['swim', 'fly', 'run', 'power', 'stamina'].forEach((stat, index) => {
          context.fillText(profile?.id ? value(profile.stats?.[stat]?.level) : '—', 236, 71 + index * 16);
        });
        context.fillText(profile?.id ? value(profile.rings) : '—', 236, 158);
      }
      if (profile?.id && origin && Number.isFinite(profile.x) && Number.isFinite(profile.z)) {
        // Project only received positions into the small garden; this does
        // not run the original movement engine or extrapolate offline motion.
        const x = Math.max(20, Math.min(152, 109.12 + (profile.x - origin.x) * 8.8));
        const y = Math.max(64, Math.min(142, 108.8 + (profile.z - origin.z) * (22 * 160 / 360)));
        const size = 32, foot = (chaosVariant(profile) === 'neutral' ? 83 : 84) / 96;
        const avatarY = y + size * (0.5 - foot);
        context.fillStyle = '#0004'; context.beginPath(); context.ellipse(x, y, 7, 2, 0, 0, Math.PI * 2); context.fill();
        const walking = reconciled && profile.watch?.activity === 'wander' && time >= lastMovedAt && time - lastMovedAt < 350;
        const artTime = reconciled && !reducedMotion?.matches ? time : 0;
        const owner = profile.admin === true;
        bodyContext.clearRect(0, 0, bodyLayer.width, bodyLayer.height);
        bodyContext.save(); bodyContext.scale(canvas.width / 240, canvas.height / 160);
        // Keep the owner's native front anchor so its fixed cyan chest gem
        // remains on the body while the original Idle/Walk poses loop.
        const drawn = drawTinyAvatar(bodyContext, profile, x, avatarY, size, { time: artTime, direction: owner ? 0 : direction, walking });
        bodyContext.restore();
        const ornaments = drawn && owner && widgetEmeraldImagesReady(emeraldImages)
          ? widgetEmeraldLayout(profile, { canonical: true, x, y: avatarY, size, time: artTime }) : [];
        drawWidgetEmeraldLayer(context, emeraldImages, ornaments, 'far');
        context.drawImage(bodyLayer, 0, 0, 240, 160);
        drawWidgetEmeraldLayer(context, emeraldImages, ornaments, 'chest');
        drawWidgetEmeraldLayer(context, emeraldImages, ornaments, 'near');
        if (avatarDrawn !== drawn) { avatarDrawn = drawn; render(); }
        if (reconciled && profile.watch?.activity === 'rest') {
          context.fillStyle = '#f5dfa4'; context.font = '6px system-ui'; context.fillText('z z', x + 9, y - 23);
        }
      }
      context.restore();
    },
  };
}
