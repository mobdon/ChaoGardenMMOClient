import catalog from '../shared/stage-catalog.json';

const familyNames = { speed: 'Speed', hunt: 'Treasure hunt', mech: 'Mech', boss: 'Boss', chao: 'Chao', kart: 'Kart', other: 'Other' };

export function createStageSelect(dialog, { onAction, onChange }) {
  const search = dialog.querySelector('input');
  const filters = dialog.querySelector('.stage-filters');
  const list = dialog.querySelector('.stage-grid');
  const count = dialog.querySelector('.stage-count');
  let family = 'all';
  const stages = Array.isArray(catalog.stages) ? catalog.stages : [];

  function render() {
    const query = search.value.trim().toLowerCase();
    const matches = stages.filter(stage => (family === 'all' || stage.family === family)
      && `${stage.name} ${stage.character} ${stage.id}`.toLowerCase().includes(query));
    count.textContent = `${matches.length} of ${stages.length} stages and activities`;
    list.replaceChildren();
    for (const stage of matches) {
      const card = document.createElement('article');
      card.className = `stage-card stage-family-${stage.family}`;
      const badge = document.createElement('small');
      badge.textContent = familyNames[stage.family] || 'Adventure';
      const title = document.createElement('h3');
      title.textContent = stage.name;
      const character = document.createElement('p');
      character.textContent = stage.character || 'Chao adventure';
      const status = document.createElement('span');
      status.className = 'stage-coming';
      status.textContent = 'Coming later';
      card.append(badge, title, character, status);
      list.append(card);
    }
    if (!matches.length) {
      const empty = document.createElement('p');
      empty.className = 'stage-empty';
      empty.textContent = 'No adventures match that search.';
      list.append(empty);
    }
  }

  for (const [value, label] of [['all', 'All'], ...Object.entries(familyNames)]) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = label;
    button.className = value === family ? 'selected' : '';
    button.setAttribute('aria-pressed', String(value === family));
    button.addEventListener('click', () => {
      family = value;
      for (const sibling of filters.children) {
        sibling.classList.toggle('selected', sibling === button);
        sibling.setAttribute('aria-pressed', String(sibling === button));
      }
      render();
    });
    filters.append(button);
  }
  search.addEventListener('input', render);
  dialog.querySelector('[data-close-stages]').addEventListener('click', () => dialog.close());
  for (const button of dialog.querySelectorAll('[data-stage-action]')) {
    button.addEventListener('click', () => { dialog.close(); onAction(button.dataset.stageAction); });
  }
  dialog.addEventListener('close', onChange);
  render();
  return { open() { dialog.showModal(); onChange(); search.focus(); } };
}
