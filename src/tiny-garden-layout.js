import { createTinyNativeSidebar } from './tiny-native-sidebar.js';
import { addTinySidebarToggle } from './tiny-sidebar-toggle.js';

export function createTinyGardenLayout(canvas) {
  const stage = document.createElement('div'); stage.className = 'tiny-garden-stage';
  const column = document.createElement('div'); column.className = 'tiny-garden-column';
  const garden = document.createElement('div'); garden.className = 'tiny-garden-crop';
  const sidebar = createTinyNativeSidebar();
  canvas.before(stage); garden.append(canvas); column.append(garden);
  const care = document.querySelector('#tiny-watch-care'); if (care) column.append(care);
  stage.append(column, sidebar.element);
  addTinySidebarToggle(stage, sidebar.element);
  return { update: sidebar.update };
}
