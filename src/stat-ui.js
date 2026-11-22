import './stat-labels.css';
import { nativeText } from './native-text.js';

const labels = { swim: 'Swim', fly: 'Fly', run: 'Run', power: 'Power', stamina: 'Stamina', luck: 'Luck', intelligence: 'Intelligence' };
function nativeLabel(name) {
  return nativeText(labels[name] || name, 'stat-name native-stat-name', !!labels[name]);
}

export const grades = ['E', 'D', 'C', 'B', 'A', 'S'];
export const gradeCells = { S: [88, 44], A: [198, 22], B: [220, 22], C: [242, 22], D: [264, 22], E: [286, 22] };

function nativeFrame() {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 165 76');
  svg.setAttribute('preserveAspectRatio', 'none');
  svg.setAttribute('aria-hidden', 'true');
  svg.classList.add('native-stat-frame');
  const image = document.createElementNS(ns, 'image');
  import('./game-assets.js').then(m => m.gameTextureUrl('chao-stat-panel.png')).then(url => url && image.setAttribute('href', url), () => {});
  image.setAttribute('width', '256'); image.setAttribute('height', '128');
  svg.append(image);
  return svg;
}

export function createStatRow(name) {
  const row = document.createElement('div');
  row.className = 'stat-row'; row.dataset.stat = name;
  const label = nativeLabel(name);
  const bar = document.createElement('div');
  bar.className = 'stat-bar'; bar.append(document.createElement('span'));
  const level = document.createElement('span'); level.className = 'stat-level';
  const grade = document.createElement('span'); grade.className = 'stat-grade';
  const points = document.createElement('span'); points.className = 'stat-points';
  row.append(nativeFrame(), label, level, grade, bar, points);
  return row;
}

export function updateStatRow(row, name, stat) {
  const level = Math.max(0, Math.min(99, Number(stat.level) || 0));
  // Luck/Intelligence have a hidden 0-255 grade with no native letter; a maxed one shows the S icon.
  const hidden = (name === 'luck' || name === 'intelligence') && stat.grade !== 255;
  row.querySelector('.stat-bar>span').style.width = `${level / 99 * 100}%`;
  row.querySelector('.stat-bar').setAttribute('aria-label', `${name} level ${level} of 99`);
  row.querySelector('.stat-level').textContent = `Lv. ${String(level).padStart(2, '0')}`;
  const grade = row.querySelector('.stat-grade');
  grade.textContent = hidden ? String(stat.grade) : stat.grade === 255 ? 'S' : grades[stat.grade] || 'E';
  grade.setAttribute('aria-label', hidden ? `${name} hidden grade ${stat.grade} of 255` : `${name} grade ${grade.textContent}`);
  grade.classList.toggle('native-grade', !hidden);
  if (!hidden) {
    const [x, y] = gradeCells[grade.textContent];
    grade.style.setProperty('--grade-x', `${-x}px`); grade.style.setProperty('--grade-y', `${-y}px`);
  }
  row.querySelector('.stat-points').textContent = `${String(Math.max(0, Number(stat.points) || 0)).padStart(4, '0')} pts`;
}
