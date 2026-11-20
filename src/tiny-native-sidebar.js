import './tiny-native-sidebar.css';

// One presentation of the received profile, shared by Garden and Widget.
// The native label column is widened at a blank pixel, never through Stamina.
export function createTinyNativeSidebar() {
  const element = document.createElement('aside'); element.className = 'tiny-native-sidebar';
  element.setAttribute('aria-label', 'Tiny Chao Garden stats');
  const canvas = document.createElement('canvas'); canvas.width = 288; canvas.height = 624;
  canvas.setAttribute('role', 'img'); element.append(canvas);
  const context = canvas.getContext('2d');
  const image = new Image(); image.src = '/__widget-art/tiny-garden-a2ne-sidebar-1.png';
  const font = new Image(); font.src = '/phone-tiny/__tiny-shop/tiny-native-sidebar-ascii-a2ne.png';
  let current = null, live = false, glyphs = null;
  const number = value => Number.isFinite(value) && value >= 0 ? String(Math.round(value)) : '-';
  function text(value, x, y, right = false, maxWidth = 80) {
    if (!glyphs) return;
    const cells = []; let width = -1;
    for (const char of String(value)) {
      const cell = glyphs.get(char) || glyphs.get('?');
      if (width + cell.width + 1 > maxWidth) break;
      cells.push(cell); width += cell.width + 1;
    }
    context.save(); context.translate(right ? x - Math.max(0, width) : x, y);
    let offset = 0;
    for (const cell of cells) {
      context.drawImage(font, cell.x, cell.y, cell.width, 8, offset, 0, cell.width, 8);
      offset += cell.width + 1;
    }
    context.restore();
  }
  function draw() {
    context.clearRect(0, 0, canvas.width, canvas.height);
    if (!image.complete || image.naturalWidth !== 72 || image.naturalHeight !== 160) {
      canvas.setAttribute('aria-label', 'Stat artwork unavailable'); return;
    }
    context.save(); context.scale(3, 3); context.imageSmoothingEnabled = false;
    const strip = (sourceY, height, targetY) => {
      context.drawImage(image, 0, sourceY, 46, height, 0, targetY, 46, height);
      context.drawImage(image, 46, sourceY, 1, height, 46, targetY, 24, height);
      context.drawImage(image, 46, sourceY, 26, height, 70, targetY, 26, height);
    };
    // Use the native blank parchment and one original font atlas for every
    // label and value, including the added hidden stats, Rings and Energy.
    for (let y = 0; y < 208; y += 8) strip(136, 8, y);
    text(current?.name || 'No Chao', 89, 5, true, 77);
    text(current ? live ? 'LIVE' : 'CACHED' : 'OFFLINE', 89, 17, true);
    text('Mood', 10, 32); text(number(current?.happiness), 89, 32, true, 22);
    text('Belly', 10, 48); text(number(current?.hunger), 89, 48, true, 22);
    const names = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];
    names.forEach((name, index) => {
      const y = 64 + index * 16;
      text(name[0].toUpperCase() + name.slice(1), 10, y, false, 52);
      text('Lv', 64, y, false, 12);
      text(number(current?.stats?.[name]?.level), 90, y, true, 14);
    });
    text('Rings', 10, 180); text(number(current?.rings), 89, 180, true, 44);
    text('Energy', 10, 196); text(number(current?.energy), 89, 196, true, 22);
    context.restore();
    canvas.setAttribute('aria-label', current ? `${current.name}: Mood ${number(current.happiness)}, Fullness ${number(current.hunger)}. ${names.map(name => `${name} level ${number(current.stats?.[name]?.level)}`).join(', ')}. Rings ${number(current.rings)}. ${live ? 'Live' : 'Last synced'}.` : 'Connect your Chao for stats');
  }
  font.onload = () => {
    if (font.naturalWidth !== 128 || font.naturalHeight !== 48) return;
    const atlas = document.createElement('canvas'); atlas.width = 128; atlas.height = 48;
    const pixels = atlas.getContext('2d', { willReadFrequently: true }); pixels.drawImage(font, 0, 0);
    const data = pixels.getImageData(0, 0, 128, 48).data; glyphs = new Map();
    for (let code = 32; code <= 126; code++) {
      const index = code - 32, x = index % 16 * 8, y = Math.floor(index / 16) * 8;
      let left = 8, right = -1;
      for (let cy = 0; cy < 8; cy++) for (let cx = 0; cx < 8; cx++) if (data[((y + cy) * 128 + x + cx) * 4 + 3]) { left = Math.min(left, cx); right = Math.max(right, cx); }
      glyphs.set(String.fromCharCode(code), { x: x + (right < 0 ? 0 : left), y, width: right < 0 ? 3 : right - left + 1 });
    }
    draw();
  };
  image.onload = image.onerror = font.onerror = draw;
  return { element, update(profile, connected) { current = profile; live = connected; draw(); } };
}
