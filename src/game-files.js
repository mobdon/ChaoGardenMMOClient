// The player's own SA2 files, picked once on the login page. The folder (or disc image)
// handle is kept in this site's IndexedDB; the game page (/sa2/, same site) reads its files
// from it, so the files never leave the computer and nobody is asked twice.
const DB = 'sa2-game-files', STORE = 'handles';

function open() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(STORE);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
async function run(mode, use) {
  const db = await open();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, mode), result = use(tx.objectStore(STORE));
    tx.oncomplete = () => resolve(result.result);
    tx.onerror = () => reject(tx.error);
  });
}

export const canPickFolders = typeof window.showDirectoryPicker === 'function';

/** The saved handles: { steam?: FileSystemDirectoryHandle, disc?: FileSystemFileHandle }. */
export async function savedHandles() {
  try {
    const [steam, disc] = await Promise.all(['steam', 'disc'].map(key => run('readonly', s => s.get(key))));
    return { ...(steam ? { steam } : {}), ...(disc ? { disc } : {}) };
  } catch { return {}; }
}

/** Opens the system picker (needs a click) and keeps what the player chose. */
export async function pickGameFiles(kind) {
  const handle = kind === 'disc'
    ? (await window.showOpenFilePicker({ id: 'sa2-disc', types: [{ description: 'SA2 Battle disc image', accept: { 'application/octet-stream': ['.iso', '.gcm', '.rvz'] } }] }))[0]
    : await window.showDirectoryPicker({ id: 'sa2-steam', mode: 'read' });
  await run('readwrite', s => s.put(handle, kind));
  return handle;
}

/** Inside a click: renews read access to the saved files so the game page can open them. */
export async function allowSavedFiles() {
  for (const handle of Object.values(await savedHandles())) {
    try {
      if (await handle.queryPermission({ mode: 'read' }) !== 'granted') await handle.requestPermission({ mode: 'read' });
    } catch { /* the game page says what it can't read */ }
  }
}
