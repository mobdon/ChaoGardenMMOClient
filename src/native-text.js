// The game's own 22px bitmap font (ad_22ascii, served as chao-stat-font.png);
// its CSS is in stat-labels.css.
// Foreground left bearing/width measured from unchanged ad_22ascii.png.
// Three native pixels between foregrounds retain room for the original shadow.
const glyphMetrics = { F: [5, 13], I: [6, 10], L: [5, 12], P: [4, 13], R: [4, 14], S: [4, 13],
  a: [6, 11], c: [5, 11], e: [5, 12], g: [5, 11], i: [9, 3], k: [5, 12], l: [9, 4], m: [4, 15],
  n: [5, 11], o: [4, 12], r: [5, 9], t: [6, 8], u: [5, 12], w: [3, 16], y: [4, 13],
  0: [4, 12], 1: [6, 7], 2: [5, 10], 3: [5, 10], 4: [4, 14], 5: [6, 9], 6: [5, 11], 7: [6, 10], 8: [5, 11], 9: [5, 11], x: [5, 12] };
const glyphScale = 16 / 22;
let fontReady;
function nativeFontReady() {
  return fontReady ??= new Promise(resolve => {
    if (typeof Image === 'undefined') return resolve(false);
    const image = new Image();
    image.onload = () => resolve(true); image.onerror = () => resolve(false);
    image.src = '/api/assets/chao-stat-font.png';
  });
}

// Text in the game's own 22px font where every glyph is measured, else plain text.
export function nativeText(text, className, native = [...text].every(character => glyphMetrics[character]), document = globalThis.document) {
  const label = document.createElement('span'); label.className = className;
  const fallback = document.createElement('span'); fallback.className = 'native-stat-fallback'; fallback.textContent = text;
  label.append(fallback);
  if (!native) return label;
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
