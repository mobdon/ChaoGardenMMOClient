import { spawn } from 'node:child_process';
import { readFile, writeFile, mkdtemp, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, relative, isAbsolute } from 'node:path';
import { createServer, loadConfigFromFile } from 'vite';

// Explicit, isolated acceptance preview. Never start against live profile data.
const root = process.cwd();
const executable = process.env.CHAO_TINY_QA_SERVER;
const payload = process.env.CHAO_TINY_GARDEN_PAYLOAD;
if (!executable || !isAbsolute(executable) || !payload || !isAbsolute(payload)) throw Error('Set absolute CHAO_TINY_QA_SERVER and CHAO_TINY_GARDEN_PAYLOAD paths.');
const prefix = resolve(root, '.local/tiny-phone-qa-');
const data = process.env.CHAO_TINY_QA_DATA || await mkdtemp(prefix);
if (!isAbsolute(data) || !resolve(data).startsWith(prefix) || relative(root, data).startsWith('..')) throw Error('Disposable Tiny QA data boundary failed.');
const port = Number(process.env.CHAO_TINY_QA_PORT || 5178);
if (!Number.isInteger(port) || port < 1024 || port > 65535 || [3001, 5173, 5176, 5177].includes(port)) throw Error('Choose a separate explicit QA frontend port.');
const executableSha256 = createHash('sha256').update(await readFile(executable)).digest('hex');
const origin = `http://127.0.0.1:${port}`;
let frontend, stopping = false;
const backend = spawn(executable, [], { cwd: data, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
  env: { ...process.env, CHAO_DATA_DIR: data, GAME_HOST: '127.0.0.1', GAME_PORT: '0', CHAO_OWNER_BOOTSTRAP: 'true',
    CHAO_SHUTDOWN_STDIN: '1', CHAO_ALLOWED_ORIGINS: origin, CHAO_TINY_GARDEN_PAYLOAD: payload } });
backend.stderr.on('data', bytes => process.stderr.write(bytes));
async function stop() {
  if (stopping) return; stopping = true;
  await frontend?.close(); backend.stdin.end('shutdown\n');
}
backend.on('exit', () => process.exit());
process.stdin.on('data', () => void stop()); process.on('SIGINT', () => void stop());
try {
  const target = await new Promise((done, reject) => {
    let output = '';
    const timeout = setTimeout(() => reject(Error('Rust QA startup timed out')), 10000);
    backend.once('error', reject);
    backend.stdout.on('data', bytes => {
      output += bytes; const match = output.match(/http:\/\/127\.0\.0\.1:(\d+)/);
      if (match) { clearTimeout(timeout); done(match[0]); }
    });
  });
  const status = await (await fetch(`${target}/api/status`)).json();
  if (status?.tinyGarden?.autoResident !== true || status.tinyGarden.observerSockets !== true
    || status.tinyGarden.shop?.rulesVersion !== 'adapted-tiny-shop-v1') throw Error('QA Rust protocol does not match the Tiny adapter.');
  const { config } = await loadConfigFromFile({}, resolve(root, 'vite.config.js'));
  config.root = root; config.server.port = port;
  config.cacheDir = resolve(root, `node_modules/.vite-tiny-qa-${port}`);
  config.server.proxy = { '/api/assets': { target: 'http://127.0.0.1:3001' }, '/api': { target }, '/ws': { target: target.replace('http:', 'ws:'), ws: true } };
  frontend = await createServer({ ...config, configFile: false }); await frontend.listen();
  await mkdir(resolve(root, '.local/reviews'), { recursive: true });
  await writeFile(resolve(root, port===5178?'.local/reviews/tiny-phone-qa-runtime.json':`.local/reviews/tiny-phone-qa-runtime-${port}.json`), JSON.stringify({ origin, target, data, executable, executableSha256,
    payloadSha256: createHash('sha256').update(await readFile(payload)).digest('hex'), nativeFruitCount: status.tinyGarden.shop.items.filter(item => item.source === 'native-a2ne').length }, null, 2));
  console.log(`Isolated Tiny Chao Garden ${origin}/phone.html`);
} catch (error) { await stop(); throw error; }
