import YAML from '../../tools/frontend/node_modules/yaml/dist/index.js';
import { applyExpectedChanges } from '../../../tests/e2e/business_example_series.mjs';

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

function diffSnippet(before, after) {
  const left = before ? before.split('\n') : [];
  const right = after ? after.split('\n') : [];
  let start = 0;
  let end = 0;
  const length = Math.min(left.length, right.length);
  while (start < length && left[start] === right[start]) start++;
  while (end < length - start && left.at(-end - 1) === right.at(-end - 1)) end++;
  return [
    ...left.slice(0, start).map(line => ' ' + line),
    ...left.slice(start, left.length - end).map(line => '-' + line),
    ...right.slice(start, right.length - end).map(line => '+' + line),
    ...right.slice(right.length - end).map(line => ' ' + line),
  ].join('\n');
}

function fieldName(path) {
  return path.map((key, index) => typeof key === 'number' ? '[' + key + ']' : (index ? '.' : '') + key).join('');
}

function renderOutcome(test, original, entry, codeBlock) {
  const isOriginal = test === original;
  if (test.reject) {
    const fragments = '<ul>' + test.reject.map(fragment => '<li><code>' + escape(fragment) + '</code></li>').join('') + '</ul>';
    const outcome = test.inheritsRejection ? '沿用原例的拒绝预期。' : '预期拒绝。';
    const explanation = outcome + 'CLI 非零退出，标准输出为空，首条诊断必须匹配下面的错误标记，标准错误还须包含具体原因。';
    const identity = (isOriginal || test.inheritsRejection) && test.rejection ?
      '<p>诊断还须包含入口 <code>' + escape(entry) + '</code>，并以独立词标明 <code>kind=' +
        escape(test.rejection.split('\n', 1)[0]) + '</code>。</p>' : '';
    const seedReason = isOriginal && test.rejection ? '<details class="test-expectation"><summary>种子中的拒绝说明</summary>' +
      codeBlock(test.rejection, 'text') + '</details>' : '';
    return '<p>' + explanation + '</p>' + fragments + identity + seedReason;
  }

  let differences = '';
  if (test.changes.length) {
    let snapshot = original.expected;
    const changes = test.changes.map(change => {
      const { path, absent } = change;
      const addition = Object.hasOwn(change, 'add');
      const lookup = object => path.reduce((node, field) => node?.[field], object);
      const before = addition ? '' : yaml(lookup(snapshot)).replace(/\n$/, '');
      snapshot = applyExpectedChanges(snapshot, [change]);
      const after = absent ? '' : yaml(lookup(snapshot)).replace(/\n$/, '');
      const label = '<p><code>' + escape(fieldName(path)) + '</code>' +
        (absent ? '：该字段不存在' : addition ? '：新增字段' : '') + '</p>';
      return label + codeBlock(diffSnippet(before, after), 'diff');
    });
    differences = '<h3>预期 PERS 变更</h3>' + changes.join('');
  } else if (!isOriginal) {
    differences = '<p>完整预期 PERS 与原例相同。</p>';
  }
  return differences + '<details class="test-expectation"><summary>查看完整预期 PERS</summary>' +
    codeBlock(yaml(test.expected), 'yaml') + '</details>';
}

function testPage(seed, {entry, seriesFile, tests}, codeBlock) {
  const original = tests[0];
  const sections = tests.map(test => {
    let edits = '';
    if (test.edits.length) {
      edits = '<h3>测试输入改动</h3>' + test.edits.map(({file, from, to}) =>
        '<p class="test-path"><code>' + escape(file) + '</code></p>' + codeBlock(diffSnippet(from, to), 'diff')).join('');
    } else {
      edits = '<p>输入取自' + link(seed.path, '种子中的原始代码与 AI 预处理结果') + '。</p>';
    }
    const heading = '<h2 id="case-' + escape(test.name) + '">' + escape(caseTitle(test.name)) + '</h2>';
    return '<section class="test-case">' + heading + edits + renderOutcome(test, original, entry, codeBlock) + '</section>';
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
