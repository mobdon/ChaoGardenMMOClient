// The picture sheet Tiny Chao Garden on the phone draws a Chao from (src/tiny-avatar.js): 768 x 768,
// transparent, 8 rows of 8 cells of 96 px. Row = facing, 45 degrees a row from the front through the
// Chao's nose pointing screen-left; columns 0-3 stand, 4-7 walk. Drawn here, on the PC, from the model
// the browser converted from the player's own game files (the MMO card's portrait, src/chao-portrait.js).
// shortcut: the model is the portrait's (the form's family, not this Chao's own colours and parts);
// upgrade when the portrait draws the Chao's own look.
import { openChaoScene } from './chao-portrait.js';

export const CELL = 96, GRID = 8;

/** The clip frame for a column: four frames spread evenly over the clip. */
export function sheetFrame(from, to, column) {
  return from + (to - from) * (column % 4) / 4;
}

/** `player`'s sheet as a PNG blob; null when the model or its Idle clip is missing. */
export async function renderTinySheet(player) {
  const cell = Object.assign(document.createElement('canvas'), { width: CELL, height: CELL });
  const opened = await openChaoScene(cell, player, { preserveDrawingBuffer: true });
  if (!opened) return null;
  const { B, scene, visual, imported, dispose } = opened;
  try {
    const clip = name => imported.entries.animationGroups.find(group => group.name.toLowerCase().includes(name));
    const idle = clip('idle'), walk = clip('walk') || idle; // no walk clip: the standing frames again
    if (!idle) return null;
    const sheet = Object.assign(document.createElement('canvas'), { width: CELL * GRID, height: CELL * GRID });
    const context = sheet.getContext('2d');
    await scene.whenReadyAsync(); // a render before the shaders are ready draws nothing
    for (let row = 0; row < GRID; row++) {
      for (let column = 0; column < GRID; column++) {
        const group = column < 4 ? idle : walk;
        // The other clip leaves its first frame behind, not the pose it stopped in.
        for (const other of [idle, walk]) if (other !== group && other.isStarted) { other.goToFrame(other.from); other.stop(); }
        group.start(true); group.pause();
        group.goToFrame(sheetFrame(group.from, group.to, column));
        scene.render();
        context.drawImage(cell, column * CELL, row * CELL);
      }
      // The camera is on the Chao's front (+z) and screen-left is +x there: a positive turn about Y
      // brings the nose from the front to screen-left (unverified against a rendered sheet).
      visual.rotate(B.Axis.Y, Math.PI / 4);
    }
    return await new Promise(resolve => sheet.toBlob(resolve, 'image/png'));
  } finally { dispose(); }
}
