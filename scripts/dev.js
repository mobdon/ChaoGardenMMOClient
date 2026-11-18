import { fork, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const filename = fileURLToPath(import.meta.url);
const root = resolve(dirname(filename), '..');

// Workers keep an IPC connection to their launcher. Losing that connection
// also stops them when a launcher is terminated directly on Windows.
async function runWorker(kind) {
  let stopAction;
  let stopRequested = false;
  let stopping = false;
  const stop = async () => {
    stopRequested = true;
    if (!stopAction || stopping) return;
    stopping = true;
    try { await stopAction(); }
    catch (error) { console.error(`${kind} shutdown failed: ${error.message}`); process.exitCode = 1; }
    finally { if (process.connected) process.disconnect(); }
  };
  process.on('message', message => { if (message?.type === 'shutdown') void stop(); });
  process.on('disconnect', () => { void stop(); });
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { void stop(); });

  if (kind === 'frontend') {
    const { createServer } = await import('vite');
    const frontend = await createServer({ root });
    stopAction = () => frontend.close();
    if (!stopRequested) {
      await frontend.listen();
      frontend.printUrls();
    }
  } else if (kind === 'backend' && process.env.CHAO_BACKEND_EXE) {
    const backend = spawn(process.env.CHAO_BACKEND_EXE, [], {
      cwd: root, env: { ...process.env, CHAO_SHUTDOWN_STDIN: '1' }, stdio: ['pipe', 'inherit', 'inherit'], windowsHide: true,
    });
    const exited = new Promise(resolveExit => backend.once('exit', (code, signal) => {
      if (!stopRequested) {
        process.exitCode = signal ? 1 : code ?? 1;
        if (process.connected) process.disconnect();
      }
      resolveExit();
    }));
    backend.once('error', error => {
      console.error(`Backend could not start: ${error.message}`);
      process.exitCode = 1;
      if (process.connected) process.disconnect();
    });
    stopAction = async () => {
      if (backend.exitCode === null && backend.signalCode === null && backend.stdin.writable) backend.stdin.end('shutdown\n');
      const force = setTimeout(() => backend.kill('SIGKILL'), 2000);
      force.unref();
      await exited;
      clearTimeout(force);
    };
  } else if (kind === 'backend') {
    // Only this worker imports the Node reference server. Vite and the
    // launcher never load authority, profile storage, or native import code.
    const { server } = await import('../server/index.js');
    stopAction = async () => { process.emit('SIGTERM'); };
    server.once('error', () => { process.exitCode = 1; void stop(); });
    server.once('close', () => {
      if (!stopRequested) {
        process.exitCode = 1;
        if (process.connected) process.disconnect();
      }
    });
  } else {
    throw new Error(`Unknown development worker: ${kind}`);
  }
  if (stopRequested || !process.connected) await stop();
}

async function runLauncher() {
  const releasePath = resolve(root, 'server', 'target', 'release', process.platform === 'win32' ? 'chao-game-server.exe' : 'chao-game-server');
  const configured = process.env.CHAO_BACKEND === 'node' ? '' : process.env.CHAO_BACKEND_EXE || (existsSync(releasePath) ? releasePath : '');
  const backendExecutable = configured ? resolve(root, configured) : '';
  if (backendExecutable && !existsSync(backendExecutable)) {
    throw new Error(`CHAO_BACKEND_EXE does not exist: ${backendExecutable}`);
  }
  const childEnvironment = {
    ...process.env,
    GAME_PORT: process.env.GAME_PORT || '3001',
    GAME_HOST: process.env.GAME_HOST || '127.0.0.1',
    CHAO_DATA_DIR: process.env.CHAO_DATA_DIR || resolve(root, '.local'),
    CHAO_OWNER_BOOTSTRAP: process.env.CHAO_OWNER_BOOTSTRAP || 'true',
    ...(backendExecutable ? { CHAO_BACKEND_EXE: backendExecutable } : {}),
  };
  const workers = [];
  let stopping = false;
  const stop = async (code = 0) => {
    if (stopping) return;
    stopping = true;
    process.exitCode = code;
    for (const { child } of workers) if (child.connected) child.send({ type: 'shutdown' }, () => {});
    const force = setTimeout(() => {
      for (const { child } of workers) if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    }, 4000);
    force.unref();
    await Promise.all(workers.map(worker => worker.exited));
    clearTimeout(force);
  };
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { void stop(); });
  for (const kind of ['backend', 'frontend']) {
    const child = fork(filename, ['--worker', kind], {
      cwd: root, env: childEnvironment, stdio: ['inherit', 'inherit', 'inherit', 'ipc'], windowsHide: true,
    });
    const exited = new Promise(resolveExit => {
      child.once('exit', (code, signal) => {
        resolveExit();
        if (!stopping) void stop(signal ? 1 : code ?? 1);
      });
      child.once('error', error => {
        console.error(`${kind} worker could not start: ${error.message}`);
        resolveExit();
        void stop(1);
      });
    });
    workers.push({ child, exited });
  }
  console.log(`Backend: ${backendExecutable || 'Node reference (set CHAO_BACKEND_EXE for a tested Rust build)'}`);
  console.log('The first local profile is the owner. Other browser profiles join as regular Chao.');
}

try {
  if (process.argv[2] === '--worker') await runWorker(process.argv[3]);
  else await runLauncher();
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
  if (process.connected) process.disconnect();
}
