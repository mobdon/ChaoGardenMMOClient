import './stat-labels.css';

const labels = { swim: 'Swim', fly: 'Fly', run: 'Run', power: 'Power', stamina: 'Stamina', luck: 'Luck', intelligence: 'Intelligence' };
// Foreground left bearing/width measured from unchanged ad_22ascii.png.
// Three native pixels between foregrounds retain room for the original shadow.
const glyphMetrics = { F: [5, 13], I: [6, 10], L: [5, 12], P: [4, 13], R: [4, 14], S: [4, 13],
  a: [6, 11], c: [5, 11], e: [5, 12], g: [5, 11], i: [9, 3], k: [5, 12], l: [9, 4], m: [4, 15],
  n: [5, 11], o: [4, 12], r: [5, 9], t: [6, 8], u: [5, 12], w: [3, 16], y: [4, 13] };
const glyphScale = 16 / 22;
let fontReady;
function nativeFontReady() {
  return fontReady ??= new Promise(resolve => {
    const image = new Image();
    image.onload = () => resolve(true); image.onerror = () => resolve(false);
    image.src = '/api/assets/chao-stat-font.png';
  });
}

function nativeLabel(name) {
  const label = document.createElement('span'); label.className = 'stat-name native-stat-name';
  const text = labels[name] || name;
  const fallback = document.createElement('span'); fallback.className = 'native-stat-fallback'; fallback.textContent = text;
  label.append(fallback);
  if (!labels[name]) return label;
  const glyphs = document.createElement('span'); glyphs.className = 'native-stat-glyphs'; glyphs.setAttribute('aria-hidden', 'true');
  for (const character of text) {
    const index = character.codePointAt(0) - 33, [left, width] = glyphMetrics[character];
    const glyph = document.createElement('span'); glyph.className = 'native-stat-glyph';
    glyph.style.setProperty('--glyph-x', `${-(index % 23) * 16}px`);
    glyph.style.setProperty('--glyph-y', `${-Math.floor(index / 23) * 16}px`);
    glyph.style.setProperty('--glyph-left', `${(1 - left) * glyphScale}px`);
    glyph.style.setProperty('--glyph-advance', `${(width + 3) * glyphScale}px`);
    glyphs.append(glyph);
  }
  label.append(glyphs);
  void nativeFontReady().then(loaded => label.classList.toggle('native-font-ready', loaded));
  return label;
}
const grades = ['E', 'D', 'C', 'B', 'A', 'S'];
const gradeCells = { S: [88, 44], A: [198, 22], B: [220, 22], C: [242, 22], D: [264, 22], E: [286, 22] };

function nativeFrame() {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 165 76');
  svg.setAttribute('preserveAspectRatio', 'none');
  svg.setAttribute('aria-hidden', 'true');
  svg.classList.add('native-stat-frame');
  const image = document.createElementNS(ns, 'image');
  image.setAttribute('href', '/api/assets/chao-stat-panel.png');
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
  const hidden = name === 'luck' || name === 'intelligence';
  row.querySelector('.stat-bar>span').style.width = `${level / 99 * 100}%`;
  row.querySelector('.stat-bar').setAttribute('aria-label', `${name} level ${level} of 99`);
  row.querySelector('.stat-level').textContent = `Lv. ${String(level).padStart(2, '0')}`;
  const grade = row.querySelector('.stat-grade');
  grade.textContent = hidden ? (stat.grade === 255 ? 'MAX' : String(stat.grade)) : grades[stat.grade] || 'E';
  grade.setAttribute('aria-label', hidden ? `${name} hidden grade ${stat.grade} of 255` : `${name} grade ${grade.textContent}`);
  grade.classList.toggle('native-grade', !hidden);
  if (!hidden) {
    const [x, y] = gradeCells[grade.textContent];
    grade.style.setProperty('--grade-x', `${-x}px`); grade.style.setProperty('--grade-y', `${-y}px`);
  }
  row.querySelector('.stat-points').textContent = `${String(Math.max(0, Number(stat.points) || 0)).padStart(4, '0')} pts`;
}
