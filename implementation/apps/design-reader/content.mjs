import { readdir, readFile, realpath, stat } from 'node:fs/promises';
import { basename, extname, join, posix, relative, resolve } from 'node:path';
import MarkdownIt from 'markdown-it';
import hljs from 'highlight.js/lib/common';
import { loadSeedTests } from './test-data.mjs';
import { attachTestPages } from './test-pages.mjs';

const md = new MarkdownIt({ html: false });
const escape = md.utils.escapeHtml;
const textDocument = /\.(md|ya?ml)$/i;
const referenceRoots = new Set(['design', 'usage', 'implementation', 'tests']);
const sectionLabels = { java: 'Java', yaml: 'YAML', vue: 'Vue', typescript: 'TypeScript', diff: 'Diff', sql: 'SQL', text: '文本' };

function codeBlock(code, lang) {
  const language = lang === 'vue' ? 'xml' : lang;
  let content;
  if (lang === 'diff') {
    content = code.replace(/\n$/, '').split('\n').map(line => {
      let style = '';
      if (/^(--- |\+\+\+ |diff --git |index )/.test(line)) style = 'diff-meta';
      else if (line.startsWith('@@')) style = 'diff-hunk';
      else if (line.startsWith('+')) style = 'diff-add';
      else if (line.startsWith('-')) style = 'diff-del';
      return '<span class="diff-line ' + style + '">' + escape(line) + '</span>';
    }).join('\n');
  } else {
    content = hljs.getLanguage(language) ? hljs.highlight(code, { language }).value : escape(code);
  }
  const label = Object.hasOwn(sectionLabels, lang) ? sectionLabels[lang] : lang || '代码';
  return '<figure class="code-block"><figcaption>' + escape(label) + '</figcaption><pre><code class="hljs language-' +
    escape(lang) + '">' + content + '</code></pre></figure>\n';
}
md.renderer.rules.fence = (tokens, i) => codeBlock(tokens[i].content, tokens[i].info.trim().split(/\s+/)[0]);

// Color only the explicitly annotated before/after columns, in live and exported pages.
md.core.ruler.after('inline', 'rewrite_columns', ({ tokens }) => {
  const styles = new Map([['🟠 原式', 'rewrite-before'], ['🟢 改写后', 'rewrite-after']]);
  let columns, index;
  for (const [i, token] of tokens.entries()) {
    if (token.type === 'table_open') columns = [];
    if (token.type === 'table_close') columns = undefined;
    if (!columns) continue;
    if (token.type === 'tr_open') index = 0;
    if (token.type !== 'th_open' && token.type !== 'td_open') continue;
    if (token.type === 'th_open') columns[index] = styles.get(tokens[i + 1]?.content);
    if (columns[index]) token.attrJoin('class', columns[index]);
    index++;
  }
});

async function designFiles(root, folder) {
  const files = [];
  for (const entry of await readdir(join(root, folder), { withFileTypes: true })) {
    const path = posix.join(folder, entry.name);
    if (entry.isDirectory() && !entry.name.startsWith('.')) files.push(...await designFiles(root, path));
    if (entry.isFile() && textDocument.test(entry.name)) files.push(path);
  }
  return files.sort();
}

function group(path) {
  if (!path.startsWith('design/')) return '参考资料';
  if (path === 'design/examples/seeds.yaml' || path === 'design/examples/seed-manifest.yaml' || path === 'design/examples/seed-plan.yaml') return '种子清单';
  if (path.startsWith('design/examples/topics/')) return '基础主题';
  if (path.startsWith('design/examples/compositions/')) return '联合主题';
  return '设计导览';
}

function textOf(tokens) {
  return (tokens ?? []).map(token => token.children ? textOf(token.children) : token.content).join('');
}

function routePath(path) {
  return path.split('/').map(encodeURIComponent).join('/');
}

// Links from design pages become reference pages; links from those references can open raw files.
function localLink(from, href) {
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('/') || href.startsWith('\\')) return null;
  let decoded;
  try { decoded = decodeURIComponent(href.split('#', 1)[0]); } catch { return null; }
  if (decoded.includes('\\') || decoded.includes('\0') || decoded.startsWith('/')) return null;
  const hash = href.indexOf('#');
  let path = decoded ? posix.join(posix.dirname(from), decoded) : from;
  if (path.endsWith('/')) path += 'README.md';
  if (!referenceRoots.has(path.split('/')[0]) || path.split('/').some(part => part.startsWith('.') || part === 'node_modules')) return null;
  return { path, anchor: hash < 0 ? '' : href.slice(hash) };
}

function updateLink(token, attribute, from, pages) {
  const href = token.attrGet(attribute);
  const link = localLink(from, href);
  if (link && attribute === 'href' && pages.has(link.path)) {
    token.attrSet(attribute, '#/' + routePath(link.path) + link.anchor);
  } else if (link) {
    token.attrSet(attribute, '/source/' + routePath(link.path) + link.anchor);
  } else if (!/^https?:\/\//i.test(href) && !(attribute === 'href' && /^mailto:/i.test(href))) {
    token.attrs = token.attrs.filter(([key]) => key !== attribute);
  }
}

async function readDocument(root, path) {
  const content = await readFile(join(root, path), 'utf8');
  const markdown = extname(path).toLowerCase() === '.md';
  const tokens = markdown ? md.parse(content, {}) : [];
  const headings = [];
  const used = new Set();
  let title = path.endsWith('/seeds.yaml') ? '种子清单' : basename(path);
  let hasTitle = false;
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].type !== 'heading_open') continue;
    const level = Number(tokens[i].tag.slice(1));
    const text = textOf(tokens[i + 1].children);
    const base = text.toLowerCase().replace(/[^\p{L}\p{N}\p{M}\s_-]/gu, '').trim().replace(/\s+/g, '-') || 'section';
    let id = base;
    let n = 0;
    while (used.has(id)) id = base + '-' + ++n;
    used.add(id);
    tokens[i].attrSet('id', id);
    if (!hasTitle && level === 1) {
      title = text;
      hasTitle = true;
    } else headings.push({ id, text, level });
  }
  return { path, title, headings, tokens, content, markdown };
}

export async function loadContent(repoRoot) {
  const root = await realpath(resolve(repoRoot));
  const design = await designFiles(root, 'design');
  let seedTests;
  let catalogError;
  try {
    seedTests = loadSeedTests(root);
  } catch (error) {
    // Design drafts stay readable while the test catalog is being edited.
    catalogError = String(error.message).split('\n', 1)[0];
  }
  const seriesFiles = new Set(Array.from(seedTests?.values() ?? []).flatMap(seed => seed.entries.map(entry => entry.seriesFile)));
  const pages = new Map();
  const aliases = new Map();
  const refs = new Set();
  for (const path of design) {
    const doc = await readDocument(root, path);
    pages.set(path, doc);
    for (const token of doc.tokens) {
      const children = token.children ?? [];
      for (let i = 0; i < children.length; i++) {
        if (children[i].type !== 'link_open') continue;
        const link = localLink(path, children[i].attrGet('href'));
        if (!link) continue;
        if (!link.path.startsWith('design/') && textDocument.test(link.path)) refs.add(link.path);
        if (path.endsWith('/README.md') && link.path.startsWith('design/') && !aliases.has(link.path)) {
          const end = children.findIndex((item, index) => index > i && item.type === 'link_close');
          if (end !== -1) aliases.set(link.path, textOf(children.slice(i + 1, end)));
        }
      }
    }
  }
  for (const path of refs) {
    if (seriesFiles.has(path)) continue;
    const file = join(root, path);
    const physicalPath = await realpath(file).catch(() => null);
    if (physicalPath && relative(root, physicalPath) === path && (await stat(file)).isFile()) {
      pages.set(path, await readDocument(root, path));
    }
  }
  const order = [...new Set(['design/README.md', 'design/architecture.md', ...aliases.keys(), ...pages.keys()])];
  const routes = new Set([...pages.keys(), ...seriesFiles]);
  const documents = order.filter(path => pages.has(path)).map(path => {
    const doc = pages.get(path);
    for (const block of doc.tokens) {
      for (const inline of block.children ?? []) {
        if (inline.type === 'link_open') updateLink(inline, 'href', path, routes);
        if (inline.type === 'image') updateLink(inline, 'src', path, routes);
      }
    }
    const html = doc.markdown ? md.renderer.render(doc.tokens, md.options, {}) :
      '<h1>' + escape(doc.title) + '</h1>' + codeBlock(doc.content, 'yaml');
    return { path, group: group(path), title: aliases.get(path) || doc.title.replace(/^Hello World[：:]\s*/i, ''), headings: doc.headings, html };
  });
  if (seedTests) return { pages: attachTestPages(documents, seedTests, codeBlock) };

  const notice = '<p class="seed-test-jump" role="status">关联测试暂不可用。' + escape(catalogError) + '</p>';
  for (const page of documents.filter(page => page.path.startsWith('design/'))) {
    page.html = page.html.replace('</h1>', () => '</h1>' + notice);
  }
  return { pages: documents };
}
