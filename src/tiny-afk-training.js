const RULES = 'adapted-afk-training-v1';
const valid = value => Number.isSafeInteger(value) && value >= 0;
const skills = ['swim', 'fly', 'run', 'power', 'stamina'];
const title = value => value ? value[0].toUpperCase() + value.slice(1) : '';

export function tinyAfkView(care, connected) {
  const state = care?.training;
  const supported = care?.rulesVersion === 'adapted-care-v1' && valid(care.revision)
    && [RULES,'adapted-afk-training-v2'].includes(state?.rulesVersion) && state.enabled === true;
  const job = supported && state.job && valid(state.job.jobId) && skills.includes(state.job.stat) ? state.job : null;
  const active = !!job && ['training', 'ready'].includes(job.phase);
  return { supported, state: supported ? state : null, job, active,
    revision: supported ? care.revision : null,
    canStart: !!(supported && connected && state.eligibility?.canStart === true),
    canCancel: !!(supported && connected && state.eligibility?.canCancel === true && job),
    canSettle: !!(supported && connected && state.eligibility?.canSettle === true && job),
  };
}

// Platform adapter only. Durations/options come from Rust policy. Every mutation
// goes through the real durable care envelope; no local points or trusted clock.
export function createTinyTraining({ root, send, operationId = () => crypto.randomUUID() }) {
  const doc = root.ownerDocument;
  const make = (tag, text, className) => { const node = doc.createElement(tag); node.textContent = text || ''; if (className) node.className = className; return node; };
  const heading = make('h2', 'AFK training');
  const status = make('p', '', 'tiny-panel-copy'); status.setAttribute('role', 'status');
  const skillLabel = make('label', 'Train a skill', 'tiny-training-select');
  const select = make('select'); select.setAttribute('aria-label', 'Training skill');
  skills.forEach(stat => { const option = make('option', title(stat)); option.value = stat; select.append(option); }); skillLabel.append(select);
  const durationLabel = make('label', 'Session length', 'tiny-training-select');
  const duration = make('select'); duration.setAttribute('aria-label', 'Training duration'); durationLabel.append(duration);
  const detail = make('p', '', 'tiny-panel-copy');
  const result = make('p', '', 'tiny-training-result');
  const feedback = make('p', '', 'tiny-panel-copy'); feedback.setAttribute('role', 'status');
  const actions = make('div', '', 'tiny-panel-actions');
  const start = make('button', 'Start training', 'tiny-primary'), cancel = make('button', 'Cancel training', 'small-button'), settle = make('button', 'Finish training', 'small-button');
  for (const button of [start, cancel, settle]) button.type = 'button'; actions.append(start, cancel, settle);
  const retry = make('button', 'Check the same request again', 'small-button'); retry.type = 'button';
  root.replaceChildren(heading, status, skillLabel, durationLabel, detail, result, actions, feedback, retry);
  let profileId, care, connected = false, pending = null, ackRevision = null, view, policySignature = '';
  let rings=null;
  const cost=()=>view?.state?.rulesVersion===RULES ? 0 : view?.state?.quotes?.[select.value]?.[duration.selectedIndex];
  function render() {
    view = tinyAfkView(care, connected);
    const state = view.state, job = view.job;
    if (pending && ackRevision !== null && care?.revision >= ackRevision) { pending = null; ackRevision = null; feedback.textContent = 'Training request saved.'; }
    status.textContent = !view.supported ? 'Training will appear when your garden enables it.' : !connected ? 'Reconnect to see your current training.'
      : pending ? 'Saving your request…' : view.active ? job.phase === 'ready' ? 'Finishing your session…' : 'Training in progress' : 'Pick a skill and a session.';
    const policy = state?.policy;
    const signature = JSON.stringify(policy);
    if (signature !== policySignature) {
      policySignature = signature; duration.replaceChildren();
      if (valid(policy?.tickMs) && policy.tickMs > 0 && valid(policy.maxDurationMs)) {
        const choices = [...new Set((Array.isArray(policy.menuMinutes)?policy.menuMinutes.map(n=>n*60000):[policy.tickMs, 15 * 60000, 60 * 60000, policy.maxDurationMs]))].filter(ms => ms > 0 && ms <= policy.maxDurationMs && ms % policy.tickMs === 0);
        for (const ms of choices) { const option = make('option', `${ms / 60000} minutes`); option.value = String(ms); duration.append(option); }
      }
    }
    skillLabel.hidden = durationLabel.hidden = !view.supported || view.active;
    select.disabled = duration.disabled = !view.canStart || !!pending;
    const price=cost();
    detail.textContent=view.active ? `${title(job.stat)} - ${Math.ceil(job.remainingMs / 1000)} seconds remaining.` : policy && valid(price) ? `${price} rings. Wallet: ${rings ?? '-'}. ${price?'Paid up front. Canceling returns no rings.':''}` : '';
    start.textContent=valid(price)?`Train for ${price} rings`:'Start training';
    const saved = state?.lastResult;
    result.hidden = !saved;
    result.textContent = saved ? saved.outcome === 'canceled' ? 'Last session canceled.' : `${title(saved.stat)} +${saved.appliedPoints} points · Saved${saved.appliedPoints < saved.awardedPoints ? ' · Skill cap reached' : ''}` : '';
    start.hidden = !view.supported || view.active; start.disabled = !view.canStart || !!pending || !duration.value || !valid(price) || (price>0 && (!valid(rings)||rings<price));
    cancel.hidden = !view.canCancel; cancel.disabled = !!pending;
    settle.hidden = !view.canSettle; settle.disabled = !!pending;
    retry.hidden = !pending; retry.disabled = !connected;
  }
  function submit(kind) {
    if(kind==='training_start' && start.disabled)return;
    if (pending || (kind === 'training_start' ? !view.canStart : kind === 'training_cancel' ? !view.canCancel : !view.canSettle)) return;
    const intent = kind === 'training_start' ? { kind, stat: select.value, durationMs: Number(duration.value), ...(view.state.rulesVersion===RULES?{}:{quotedCostRings:cost()}) } : { kind, jobId: view.job.jobId };
    const message = { type: 'care', requestId: operationId(), expectedRevision: view.revision, intent };
    if (send(message) === false) return;
    pending = message; ackRevision = null; feedback.textContent = ''; render();
  }
  select.addEventListener('change',render);duration.addEventListener('change',render);
  start.addEventListener('click', () => submit('training_start')); cancel.addEventListener('click', () => submit('training_cancel')); settle.addEventListener('click', () => submit('training_settle'));
  retry.addEventListener('click', () => { if (pending && connected) send(pending); }); render();
  return {
    update(profile, connection) {
      if (profileId !== profile?.id) { profileId = profile?.id; pending = null; ackRevision = null; feedback.textContent = ''; }
      rings=profile?.rings;care = connection.care; connected = connection.connected && connection.reconciled; render();
    },
    handleEvent(event) {
      if (!pending || event?.kind !== 'care-result' || event.requestId !== pending.requestId || event.rulesVersion !== 'adapted-care-v1' || !valid(event.revision)) return false;
      if (event.accepted === true) { ackRevision = event.revision; feedback.textContent = 'Saved. Refreshing your training…'; }
      else { pending = null; ackRevision = null; feedback.textContent = `Training was not changed (${event.code || 'unavailable'}).`; }
      render(); return true;
    },
    get blocksActivity() { return !!pending || view.active; }, get busy() { return !!pending || view.active; }, get supported() { return view.supported; },
  };
}
