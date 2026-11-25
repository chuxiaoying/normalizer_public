import { spawn } from 'node:child_process';
import { readFile, realpath, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { loadContent } from './content.mjs';

const appDir = fileURLToPath(new URL('.', import.meta.url));
const root = await realpath(resolve(appDir, '../../..'));
const host = '127.0.0.1';
const port = 4176;
const address = `http://${host}:${port}`;
const identity = `Normalizer Design Reader\n${root}`;
let linkedSources = new Set();

async function renderPage(offline) {
  const [template, css, js, content] = await Promise.all([
    readFile(join(appDir, 'index.html'), 'utf8'),
    readFile(join(appDir, 'reader.css'), 'utf8'),
    readFile(join(appDir, 'reader.js'), 'utf8'),
    loadContent(root),
  ]);
  const sources = new Set();
  for (const page of content.pages) {
    page.html = page.html.replace(/href="(\/source\/([^"#?]*))([^"]*)"/g, (link, route, path, suffix) => {
      sources.add(route);
      return offline ? `href="${pathToFileURL(join(root, decodeURIComponent(path))).href}${suffix}"` : link;
    });
  }
  linkedSources = sources;
  const json = JSON.stringify(content).replaceAll('<', '\\u003c');
  return template
    .replace('<link rel="stylesheet" href="reader.css">', () => `<style>${css}</style>`)
    .replace('<script src="reader.js" defer></script>', () => `<script>${js}</script>`)
    .replace('__READER_DATA__', () => json);
}

function openBrowser() {
  const command = process.platform === 'darwin' ? 'open' : 'xdg-open';
  const browser = spawn(command, [address], { detached: true, stdio: 'ignore' });
  browser.on('error', () => console.error(`请在浏览器打开 ${address}`));
  browser.unref();
}

async function serve(open) {
  const server = createServer(async (req, res) => {
    const send = (status, type, body) => {
      res.writeHead(status, { 'Content-Type': type + '; charset=utf-8', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
      res.end(req.method === 'HEAD' ? undefined : body);
    };
    if (req.headers.host !== `${host}:${port}`) return send(403, 'text/plain', '仅限本机访问');
    if (!['GET', 'HEAD'].includes(req.method)) return send(405, 'text/plain', '只读');
    try {
      const { pathname } = new URL(req.url, address);
      if (pathname === '/' || pathname === '/index.html') return send(200, 'text/html', await renderPage(false));
      if (pathname === '/_reader') return send(200, 'text/plain', identity);
      if (linkedSources.has(pathname)) {
        const filename = await realpath(join(root, decodeURIComponent(pathname.slice('/source/'.length))));
        const path = relative(root, filename);
        if (isAbsolute(path) || path.startsWith('..' + sep) || path.split(sep).some(part => part.startsWith('.'))) {
          return send(403, 'text/plain', '仅展示仓库文档引用的文件');
        }
        return send(200, 'text/plain', await readFile(filename));
      }
      return send(404, 'text/plain', '未找到此文档');
    } catch (error) {
      console.error(error.message);
      return send(error.code === 'ENOENT' ? 404 : 500, 'text/plain', '读取文档失败，请检查源文件后刷新。');
    }
  });
  try {
    await new Promise((ok, fail) => server.once('error', fail).listen(port, host, ok));
  } catch (error) {
    if (error.code !== 'EADDRINUSE') throw error;
    const existing = await fetch(address + '/_reader', { signal: AbortSignal.timeout(2000) });
    if (await existing.text() !== identity) throw new Error(`端口 ${port} 正被其他程序使用`);
  }
  console.log(address);
  if (open) openBrowser();
}

const args = process.argv.slice(2);
if (args.length === 2 && args[0] === '--export') {
  await writeFile(args[1], await renderPage(true));
  console.log(resolve(args[1]));
} else if (args.length === 0 || (args.length === 1 && args[0] === '--open')) {
  await serve(args[0] === '--open');
} else {
  throw new Error('用法：node reader.mjs [--open | --export 输出文件.html]');
}
