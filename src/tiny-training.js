// Tiny Garden presentation only. The server publishes every option, timestamp,
// phase and durable result; this module never calculates a skill award.
export const TRAINING_PROTOCOL_VERSION = 1;
export const TRAINING_STATS = Object.freeze(['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence']);

const integer = value => Number.isSafeInteger(value) && value >= 0;
const identifier = value => typeof value === 'string' && value.length > 0 && value.length <= 128;
const title = value => value[0].toUpperCase() + value.slice(1);

export function normalizeTrainingSnapshot(value, serverTime) {
  if (!value || !integer(value.revision) || !identifier(value.rulesVersion) || !Array.isArray(value.options)
    || !Object.hasOwn(value, 'job')) return null;
  const optionIds = new Set();
  const options = value.options.filter(option => {
    if (!option || !identifier(option.id) || optionIds.has(option.id) || !TRAINING_STATS.includes(option.stat)
      || !integer(option.durationMs) || option.durationMs === 0 || !integer(option.costRings) || !integer(option.maxPoints)) return false;
    optionIds.add(option.id); return true;
  }).map(({ id, stat, durationMs, costRings, maxPoints }) => ({ id, stat, durationMs, costRings, maxPoints }));
  let job = null;
  if (value.job !== null) {
    const candidate = value.job;
    if (!candidate || !identifier(candidate.id) || !TRAINING_STATS.includes(candidate.stat)
      || !['active', 'ready', 'saving', 'settled', 'canceled'].includes(candidate.phase)
      || !integer(candidate.startedAt) || !integer(candidate.endsAt) || candidate.endsAt <= candidate.startedAt
      || !integer(candidate.costRings)) return null;
    job = { id: candidate.id, stat: candidate.stat, phase: candidate.phase,
      startedAt: candidate.startedAt, endsAt: candidate.endsAt, costRings: candidate.costRings };
    const result = candidate.result;
    if (result?.saved === true && identifier(result.id) && integer(result.points) && integer(result.savedAt)) {
      job.result = { id: result.id, points: result.points, savedAt: result.savedAt };
    }
  }
  return { revision: value.revision, rulesVersion: value.rulesVersion, options, job,
    serverTime: integer(serverTime) ? serverTime : null };
}

export function trainingView({ player, capabilities, connected = false, reconciled = false, busy = false,
  serverTime, pending = null, selectedOptionId = null } = {}) {
  const supported = capabilities?.training?.protocolVersion === TRAINING_PROTOCOL_VERSION;
  const snapshot = supported ? normalizeTrainingSnapshot(player?.training, serverTime) : null;
  const current = connected && reconciled;
  const job = snapshot?.job || null;
  const selected = snapshot?.options.find(option => option.id === selectedOptionId) || snapshot?.options[0] || null;
  const terminal = !!job && (job.phase === 'canceled' || (job.phase === 'settled' && !!job.result));
  const usable = supported && !!snapshot && current && !busy && !pending;
  const canAfford = selected && integer(player?.rings) && player.rings >= selected.costRings;
  const clockValid = snapshot?.serverTime !== null && snapshot?.serverTime >= (job?.startedAt || 0);
  const remainingMs = job && clockValid ? Math.max(0, job.endsAt - snapshot.serverTime) : null;
  let status = !supported ? 'Training unavailable' : !snapshot ? 'Waiting for training state'
    : !current ? 'Last synced training' : pending ? 'Waiting for saved confirmation'
      : !job ? selected ? 'Choose a skill' : 'No sessions available' : ({ active: 'Training', ready: 'Ready to claim', saving: 'Saving training',
        settled: job.result ? 'Result saved' : 'Awaiting saved result', canceled: 'Training canceled' })[job.phase];
  if (current && job?.phase === 'active' && remainingMs === 0) status = 'Waiting for server completion';
  return { supported, snapshot, job, selected, status, remainingMs,
    canStart: !!(usable && (!job || terminal) && selected && canAfford),
    canCancel: !!(usable && job?.phase === 'active'),
    canClaim: !!(usable && job?.phase === 'ready'),
    result: terminal && job.result ? job.result : null,
    blocksActivity: !!pending || (supported && !snapshot) || !!(job && !terminal) };
}

export function trainingIntent(view, action, operationId) {
  if (!identifier(operationId)) return null;
  if (action === 'start' && view.canStart) return { type: 'action', action: 'training-start', operationId,
    optionId: view.selected.id, revision: view.snapshot.revision };
  if ((action === 'cancel' && view.canCancel) || (action === 'claim' && view.canClaim)) {
    return { type: 'action', action: `training-${action}`, operationId, jobId: view.job.id, revision: view.snapshot.revision };
  }
  return null;
}

export function confirmsTrainingOperation(pending, message) {
  return !!pending && message?.kind === 'training-saved' && message.saved === true
    && message.operationId === pending.operationId && message.profileId === pending.profileId
    && integer(message.revision) && message.revision > pending.revision
    && (!pending.jobId || message.jobId === pending.jobId);
}

export function trainingDuration(milliseconds) {
  if (!integer(milliseconds)) return 'Time not confirmed';
  const seconds = Math.ceil(milliseconds / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.ceil(seconds / 60);
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export function createTinyTraining({ root, send, operationId = () => globalThis.crypto.randomUUID() }) {
  const doc = root.ownerDocument;
  const make = (tag, className, text) => { const element = doc.createElement(tag); element.className = className; if (text) element.textContent = text; return element; };
  const heading = make('div', 'tiny-panel-heading');
  const name = make('h2', '', 'AFK training');
  const status = make('span', 'phase-pill', 'Training unavailable'); status.setAttribute('role', 'status');
  heading.append(name, status);
  const explanation = make('p', 'tiny-panel-copy');
  const label = make('label', 'tiny-training-select', 'Skill and duration');
  const select = make('select', ''); select.setAttribute('aria-label', 'Training skill and duration'); label.append(select);
  const details = make('p', 'tiny-panel-copy tiny-training-details');
  const jobDetails = make('p', 'tiny-panel-copy tiny-training-job');
  const result = make('p', 'tiny-training-result'); result.setAttribute('role', 'status');
  const actions = make('div', 'tiny-panel-actions');
  const buttons = Object.fromEntries([['start', 'Start training'], ['cancel', 'Cancel training'], ['claim', 'Claim saved result']].map(([action, text]) => {
    const button = make('button', action === 'start' ? 'tiny-primary' : 'small-button', text);
    button.type = 'button'; button.dataset.trainingAction = action; actions.append(button); return [action, button];
  }));
  const feedback = make('p', 'tiny-panel-copy'); feedback.setAttribute('role', 'status');
  root.replaceChildren(heading, explanation, label, details, jobDetails, result, actions, feedback);
  let profile = null, connection = {}, pending = null, selectedOptionId = null, optionKey = '', currentView;

  function render() {
    currentView = trainingView({ player: profile, ...connection, pending, selectedOptionId });
    const view = currentView;
    status.textContent = view.status;
    explanation.textContent = !view.supported
      ? 'This server has not enabled durable AFK training. Costs, durations and rewards will appear when training is available.'
      : !connection.connected || !connection.reconciled
        ? 'Reconnect and sync with the server before starting, canceling or claiming training. A confirmed job can continue while the app is closed.'
        : 'Choose a server-listed session. Training time and skill gains are confirmed by the garden server.';
    const options = view.snapshot?.options || [];
    const nextOptionKey = JSON.stringify(options);
    if (optionKey !== nextOptionKey) {
      optionKey = nextOptionKey; select.replaceChildren();
      options.forEach(option => { const item = make('option', '', `${title(option.stat)} · ${trainingDuration(option.durationMs)}`); item.value = option.id; select.append(item); });
    }
    if (view.selected) { selectedOptionId = view.selected.id; select.value = selectedOptionId; }
    label.hidden = !options.length;
    select.disabled = !connection.connected || !connection.reconciled || !!pending || !!(view.job && !['settled', 'canceled'].includes(view.job.phase));
    details.hidden = !view.selected;
    details.textContent = view.selected ? `Cost: ${view.selected.costRings} rings · Server cap: ${view.selected.maxPoints} points · Rules ${view.snapshot.rulesVersion}` : '';
    jobDetails.hidden = !view.job;
    jobDetails.textContent = view.job ? `${title(view.job.stat)} · ${view.remainingMs === null ? 'Server time unavailable' : `${trainingDuration(view.remainingMs)} at last sync`} · Reserved ${view.job.costRings} rings` : '';
    result.hidden = !view.result;
    result.textContent = view.result ? `${view.result.points} ${title(view.job.stat)} points · Saved by the server` : '';
    buttons.start.hidden = !!view.job && !['settled', 'canceled'].includes(view.job.phase);
    buttons.cancel.hidden = view.job?.phase !== 'active';
    buttons.claim.hidden = view.job?.phase !== 'ready';
    buttons.start.disabled = !view.canStart; buttons.cancel.disabled = !view.canCancel; buttons.claim.disabled = !view.canClaim;
    if (pending) feedback.textContent = 'Waiting for the server to save this request. Reconnect to reconcile an interrupted request.';
  }
  const selectOption = () => { selectedOptionId = select.value; render(); };
  select.addEventListener('change', selectOption);
  const perform = event => {
    const button = event.target.closest('[data-training-action]');
    if (!button || !actions.contains(button)) return;
    const message = trainingIntent(currentView, button.dataset.trainingAction, operationId());
    if (!message) return;
    pending = { ...message, profileId: profile.id }; feedback.textContent = '';
    render(); send(message);
  };
  actions.addEventListener('click', perform);
  render();
  return {
    update(nextProfile, nextConnection) {
      const changedProfile = nextProfile?.id !== profile?.id;
      const wasDisconnected = !connection.connected || !connection.reconciled;
      profile = nextProfile; connection = nextConnection;
      // A fresh canonical snapshot after reconnect resolves uncertain delivery;
      // requests are never silently re-sent or queued for later upload.
      if (changedProfile || (wasDisconnected && connection.connected && connection.reconciled)) {
        pending = null; feedback.textContent = '';
      }
      if (pending?.ackRevision !== undefined && profile?.training?.revision >= pending.ackRevision) {
        pending = null; feedback.textContent = 'Training request saved.';
      }
      render();
    },
    handleEvent(message) {
      if (confirmsTrainingOperation(pending, message)) { pending.ackRevision = message.revision; feedback.textContent = 'Training request saved. Syncing your Chao…'; render(); }
      else if (pending && message?.kind === 'training-error' && message.operationId === pending.operationId) {
        pending = null; feedback.textContent = message.text || 'The server could not save that training request.'; render();
      }
    },
    get busy() { return !!currentView?.blocksActivity; },
    get blocksActivity() { return !!currentView?.blocksActivity; },
    get supported() { return !!currentView?.supported; },
    destroy() { select.removeEventListener('change', selectOption); actions.removeEventListener('click', perform); },
  };
}
