/** Log in with a password, or with a memory card code. Resolves to the account's Chao: [{ token, player }]. */
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
  return data.chao;
}
