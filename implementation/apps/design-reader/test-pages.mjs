import YAML from '../../tools/frontend/node_modules/yaml/dist/index.js';

function escape(value) {
  return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;').replaceAll('"', '&quot;');
}

function link(path, text, anchor = '') {
  const route = path.split('/').map(encodeURIComponent).join('/');
  const section = anchor ? '#' + encodeURIComponent(anchor) : '';
  return '<a href="#/' + escape(route + section) + '">' + escape(text) + '</a>';
}

function countTests(entries) {
  return entries.reduce((sum, entry) => sum + entry.tests.length, 0);
}

function caseTitle(name) {
  return name === 'original' ? '原例（original）' : name;
}

function addSeedLinks(seed, metadata, entries) {
  const total = countTests(entries);
  const summary = entries.length ? link(seed.path, '关联测试 · ' + entries.length + ' 个入口 · ' + total + ' 个用例', 'seed-tests')
    : '关联测试 · 待派生';
  const shortcuts = entries.map(({entry, seriesFile}) => link(seriesFile, entry)).join(' ');
  const meta = '<p class="seed-test-meta">主主题 <code>' + escape(metadata.topic) + '</code> · 类型 <code>' +
    escape(metadata.kind) + '</code>' + (metadata.related.length ? ' · 关联 <code>' + escape(metadata.related.join(', ')) + '</code>' : '') + '</p>';
  const nav = '<nav class="seed-test-jump" aria-label="关联测试">' + summary + (shortcuts ? ' ' + shortcuts : '') + '</nav>';
  const intent = /(<h2[^>]*>设计意图<\/h2>[\s\S]*?)(?=<h[12]\b|$)/;
  if (intent.test(seed.html)) {
    seed.html = seed.html.replace(intent, (_, content) => '<section class="seed-intent">' + content + '</section>' + meta + nav);
  } else {
    seed.html = seed.html.replace('</h1>', () => '</h1>' + meta + nav);
  }
  if (!entries.length) return;

  seed.headings.push({ id: 'seed-tests', text: '关联测试用例', level: 2 });
  const groups = entries.map(({entry, seriesFile, tests}) => {
    const links = tests.map(({name}) => '<li>' + link(seriesFile, caseTitle(name), 'case-' + name) + '</li>');
    return '<h3>' + link(seriesFile, entry) + '</h3><ul class="case-links">' + links.join('') + '</ul>';
  });
  seed.html += '<section class="seed-test-index"><h2 id="seed-tests">关联测试用例</h2>' + groups.join('') + '</section>';
}

function yaml(value) {
  return YAML.stringify(value, { lineWidth: 0 });
}

function renderOutcome(test, codeBlock) {
  if (test.reject) {
    const { fragments, kind, location } = test.reject;
    const list = fragments.map(fragment => '<li><code>' + escape(fragment) + '</code></li>').join('');
    const identity = kind ? '<p>诊断位置 <code>' + escape(location ?? test.entry)
      + '</code>，并标明 <code>kind=' + escape(kind) + '</code>。</p>' : '';
    return '<h3>预期拒绝</h3><p>CLI 非零退出，标准输出为空，首条诊断和原因必须匹配。</p><ul>'
      + list + '</ul>' + identity;
  }
  return '<h3>完整预期 PERS</h3>' + codeBlock(yaml(test.expected), 'yaml');
}

function testPage(seed, {entry, seriesFile, tests}, codeBlock) {
  const sections = tests.map(test => {
    const input = Object.entries(test.input).map(([file, source]) => {
      const language = {java:'java', ts:'typescript'}[file.split('.').at(-1)];
      return '<p class="test-path"><code>' + escape(file) + '</code></p>' + codeBlock(source, language);
    }).join('');
    const heading = '<h2 id="case-' + escape(test.name) + '">' + escape(caseTitle(test.name)) + '</h2>';
    return '<section class="test-case">' + heading + '<p>CLI 入口：<code>' + escape(test.entry)
      + '</code></p><h3>预处理后输入</h3>' + input + renderOutcome(test, codeBlock) + '</section>';
  });
  const back = '<p class="test-backlink">' + link(seed.path, '← ' + seed.title, 'seed-tests') + '</p>';
  return {
    path: seriesFile,
    title: entry,
    group: '测试用例',
    html: back + '<h1>' + escape(entry) + '</h1>' + sections.join(''),
    headings: tests.map(({ name }) => ({ id: 'case-' + name, text: caseTitle(name), level: 2 })),
  };
}

export function attachTestPages(pages, seedTests, codeBlock) {
  const byPath = new Map(pages.map(page => [page.path, page]));
  const rows = [];
  for (const [path, metadata] of seedTests) {
    const {seedId, entries} = metadata;
    const seed = byPath.get(path);
    addSeedLinks(seed, metadata, entries);
    for (const entry of entries) pages.push(testPage(seed, entry, codeBlock));
    const tests = entries.length ? link(path, entries.length + ' 个入口 · ' + countTests(entries) + ' 个用例', 'seed-tests') : '待派生';
    rows.push('<tr><td>' + link(path, seed.title) + '</td><td><code>' + escape(seedId) + '</code></td><td><code>' + escape(metadata.kind) + '</code></td><td><code>' + escape(metadata.topic) + '</code></td><td>' + tests + '</td></tr>');
  }
  const registry = byPath.get('design/examples/seeds.yaml');
  const table = '<div class="seed-test-index"><table><thead><tr><th>设计种子</th><th>种子 ID</th><th>类型</th><th>主主题</th><th>关联测试</th></tr></thead>' +
    '<tbody>' + rows.join('') + '</tbody></table></div>';
  registry.html = registry.html.replace('</h1>', () => '</h1>' + table + '<details class="test-expectation"><summary>查看种子清单 YAML</summary>') + '</details>';
  return pages;
}
