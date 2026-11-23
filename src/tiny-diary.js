const gameNames = { memory: 'Chao Memory', rock_paper_scissors: 'Rock, Paper, Chao!', bounce: 'Chao Bounce', ball: 'Chao Ball', search: 'Chao Search' };
export function careHistoryText(entry, items = []) {
  const item = items.find(item => item.itemId === entry.itemId)?.name || entry.itemId || 'an item';
  switch (entry.kind) {
    case 'consume': return `Enjoyed ${item}.`;
    case 'tiny_purchase': case 'purchase': return `Bought ${item}.`;
    case 'tiny_enter': return 'Arrived in Tiny Chao Garden.';
    case 'tiny_toy_buy': return 'Bought the '+['trumpet','rubber duck','television'][entry.result.toyId]+'.';
    case 'tiny_toy_play': return ['Played the trumpet.','Watched the rubber duck.','Watched television.'][entry.result.toyId];
    case 'tiny_weed_pull': return 'Pulled a weed in the garden.';
    case 'tiny_egg_purchase': return 'Brought home a new egg.';
    case 'tiny_egg_hatch': return 'Hatched a new Chao!';
    case 'training_start': return 'Started a training session.';
    case 'training_cancel': return 'Stopped training.';
    case 'training_settle': return 'Finished training. Progress saved.';
    case 'equip': return `Equipped ${item}.`;
    case 'unequip': return `Put away ${item}.`;
    case 'pet': return 'Enjoyed a little attention.';
    default: return null;
  }
}

export function createTinyDiary({ root }) {
  const section = document.createElement('section'); section.className = 'tiny-phone-activity';
  const title = document.createElement('h3'); title.textContent = 'Garden diary';
  const empty = document.createElement('p'); empty.textContent = 'Your next garden moments will appear here.';
  const list = document.createElement('ol'); list.setAttribute('aria-label', 'Garden activity');
  section.append(title, empty, list); root.append(section);
  let profileId, previous = null, local = [], rendered = '';
  function observed(key, text) {
    if (!local.some(entry => entry.id === key)) local.unshift({ id: key, text, observedAt: Date.now() });
    local = local.slice(0, 5);
  }
  return {
    update(profile, { care, game, connected }) {
      if (profileId !== profile?.id) { profileId = profile?.id; previous = null; local = []; rendered = ''; }
      if (profile && connected) {
        const next = { routine: profile.watch?.activity, care: profile.watch?.lastCare, game: game?.id, gameName: game?.gameId || game?.view?.game, phase: game?.phase, result: game?.result };
        if (previous) {
          if (next.routine && next.routine !== previous.routine) observed(`routine:${next.routine}`, { wander: 'Went exploring.', rest: 'Settled down for a rest.', idle: 'Paused to look around.' }[next.routine] || 'Returned to the garden.');
          if (next.care?.at !== previous.care?.at && ['pet', 'rest', 'wake'].includes(next.care?.kind)) observed(`care:${next.care.at}:${next.care.kind}`, { pet: 'Enjoyed a pet.', rest: 'You tucked your Chao in.', wake: 'You woke your Chao up.' }[next.care.kind]);
          if (next.game && next.game !== previous.game) observed(`game:${next.game}`, `Started ${gameNames[next.gameName] || 'a minigame'}.`);
          if (next.result?.saved === true && previous.result?.saved !== true) observed(`result:${next.game}`, 'Finished a minigame. Result saved.');
          if (!next.game && previous.game && previous.result?.saved !== true) observed(`stop:${previous.game}`, 'Returned from a minigame.');
        }
        previous = next;
      } else previous = null;
      const entries = [
        ...(Array.isArray(care?.history) ? [...care.history].reverse() : []).flatMap(entry => {
          const text = careHistoryText(entry, care.tinyShop?.items || []);
          return text && typeof entry.id === 'string' && Number.isSafeInteger(entry.at) ? [{ id: entry.id, text, at: entry.at }] : [];
        }), ...local,
      ].sort((a, b) => (b.at || b.observedAt) - (a.at || a.observedAt)).slice(0, 5);
      const key = JSON.stringify(entries); if (key === rendered) return; rendered = key;
      empty.hidden = entries.length > 0;
      list.replaceChildren(...entries.map(entry => {
        const line = document.createElement('li'); const time = document.createElement('time');
        time.textContent = `${new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(entry.at || entry.observedAt)}${entry.at ? '' : ' · This visit'}`;
        line.append(time, document.createTextNode(entry.text)); return line;
      }));
    },
  };
}
