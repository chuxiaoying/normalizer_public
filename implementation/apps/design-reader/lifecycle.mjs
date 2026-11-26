import { execFile } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { lstat, mkdir, readdir, readFile, rename, unlink, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';

const run = promisify(execFile);

function listen(server, ...args) {
  return new Promise((resolve, reject) => {
    const onError = error => {
      server.off('listening', onListening);
      reject(error);
    };
    const onListening = () => {
      server.off('error', onError);
      resolve();
    };
    server.once('error', onError);
    server.once('listening', onListening);
    server.listen(...args);
  });
}

const controlHost = '127.0.0.1';
const remove = path => unlink(path).catch(() => {});

async function stopPrevious(path) {
  try {
    const { port, token } = JSON.parse(await readFile(path, 'utf8'));
    if (!Number.isInteger(port) || port < 1 || port > 65535 ||
        typeof token !== 'string' || !/^[a-f0-9]{64}$/.test(token)) return;
    const response = await fetch(`http://${controlHost}:${port}/stop`, {
      method: 'POST',
      headers: { authorization: 'Bearer ' + token },
      redirect: 'error',
      signal: AbortSignal.timeout(1800),
    });
    await response.body?.cancel();
    if (response.status === 204 || response.status === 403 || response.status === 404) await remove(path);
  } catch (error) {
    if (error.cause?.code === 'ECONNREFUSED') await remove(path);
  }
}

// Each checkout records only its own private, authenticated control listeners.
// No PID is needed, and the browser-facing reader stays read-only.
export async function claimReader(root, stop) {
  const uid = process.getuid();
  const dir = join('/tmp', 'normalizer-reader-' + uid);
  await mkdir(dir, { mode: 0o700 }).catch(error => {
    if (error.code !== 'EEXIST') throw error;
  });
  const stat = await lstat(dir);
  if (!stat.isDirectory() || stat.uid !== uid || (stat.mode & 0o077)) {
    throw new Error('阅览器控制目录不安全：' + dir);
  }
  const prefix = createHash('sha256').update(root).digest('hex') + '-';
  const records = await readdir(dir, { withFileTypes: true });
  await Promise.all(records.filter(file => file.isFile() &&
    file.name.startsWith(prefix) && file.name.endsWith('.json'))
    .map(file => stopPrevious(join(dir, file.name))));

  const token = randomBytes(32).toString('hex');
  const record = join(dir, prefix + randomUUID() + '.json');
  let stopping;
  const control = createServer((req, res) => {
    if (req.method !== 'POST' || req.url !== '/stop' || req.headers.authorization !== 'Bearer ' + token) {
      res.writeHead(403).end();
      return;
    }
    stopping ??= Promise.resolve().then(stop);
    stopping.then(() => {
      res.writeHead(204, { connection: 'close' }).end();
      control.close();
    }, () => res.writeHead(500).end());
  });
  control.once('close', () => void remove(record));
  await listen(control, 0, controlHost);
  const temp = record + '.tmp';
  try {
    await writeFile(temp, JSON.stringify({ port: control.address().port, token }), { mode: 0o600, flag: 'wx' });
    await rename(temp, record);
  } catch (error) {
    control.close();
    await remove(temp);
    throw error;
  }
  return control;
}

// The pre-control-socket launcher ran from appDir with a relative script name.
// Confirm its HTTP identity, listening PID, UID, command, and cwd before asking
// it to exit. Missing process tools or ambiguous ownership mean leave it alone.
async function stopLegacyReader(address, identity, appDir) {
  const options = { timeout: 700, env: { ...process.env, LC_ALL: 'C' } };
  const port = new URL(address).port;
  const listener = async () => (await run('lsof', ['-nP', '-t', '-iTCP:' + port, '-sTCP:LISTEN'], options)).stdout.trim();
  try {
    const response = await fetch(address + '/_reader', { redirect: 'error', signal: AbortSignal.timeout(600) });
    if (!response.ok || await response.text() !== identity) return false;
    const pidText = await listener();
    if (!/^[1-9]\d*$/.test(pidText)) return false;
    const pid = Number(pidText);
    if (pid <= 1 || pid === process.pid) return false;
    const info = async () => (await run('ps', ['-p', pidText, '-o', 'uid=', '-o', 'lstart=', '-o', 'command='], options)).stdout.trim();
    const details = await info();
    const match = /^(\d+)\s+\S+\s+\S+\s+\d+\s+[\d:]+\s+\d+\s+(.+)$/.exec(details);
    if (!match || Number(match[1]) !== process.getuid() ||
        !['node reader.mjs', 'node reader.mjs --open', process.execPath + ' reader.mjs', process.execPath + ' reader.mjs --open'].includes(match[2])) return false;
    const cwd = await run('lsof', ['-a', '-p', pidText, '-d', 'cwd', '-Fn'], options);
    if (!cwd.stdout.split('\n').includes('n' + appDir) || await listener() !== pidText || await info() !== details) return false;
    process.kill(pid, 'SIGTERM');
    await delay(80);
    return true;
  } catch {
    return false;
  }
}

export async function listenReader(server, { host, defaultPort, identity, appDir }) {
  try {
    await listen(server, defaultPort, host);
  } catch (error) {
    if (error.code !== 'EADDRINUSE') throw error;
    if (await stopLegacyReader(`http://${host}:${defaultPort}`, identity, appDir)) {
      try {
        await listen(server, defaultPort, host);
        return;
      } catch (retryError) {
        if (retryError.code !== 'EADDRINUSE') throw retryError;
      }
    }
    await listen(server, 0, host);
  }
}
