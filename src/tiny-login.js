/**
 * Log in with a password, or with a memory card code. Resolves to { token, player } of the Chao picked
 * on the PC: the one whose model the PC stored on the server. The phone has no picker of its own.
 */
export async function tinyLogin(fetch, { login, password, code }) {
  if (code !== undefined) {
    code = code.replace(/\s+/g, '').toLowerCase();
    if (!/^[a-f0-9]{64}$/.test(code)) throw new Error('That memory card code isn’t complete. It has 64 letters and numbers.');
  }
  const response = code
    ? await fetch('/api/card', { headers: { Authorization: `Bearer ${code}` }, cache: 'no-store' })
    : await fetch('/api/account/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, cache: 'no-store', body: JSON.stringify({ login, password }) });
  const data = await response.json().catch(() => ({}));
  if (!response.ok || !Array.isArray(data.chao) || !data.chao.length) throw new Error(data.error || 'Could not log in.');
  for (const chao of data.chao) {
    const model = await fetch('/api/tiny-chao', { method: 'HEAD', headers: { Authorization: `Bearer ${chao.token}` }, cache: 'no-store' });
    if (model.ok) return chao;
  }
  throw new Error('Play once on your PC first. Then your Chao is here.');
}
