import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';
import { loadSeries } from './business_example_series.mjs';
import { loadCases } from './catalog.mjs';

export function* readSeedBlocks(root, name) {
  const text = readFileSync(join(root, name), 'utf8');
  const lines = text.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const opening = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(lines[i]);
    if (!opening) continue;
    const fence = opening[1];
    const language = opening[2].trim();
    const closing = new RegExp('^ {0,3}' + fence[0] + '{' + fence.length + ',}\\s*$');
    const start = ++i;
    while (i < lines.length && !closing.test(lines[i])) i++;
    assert.ok(i < lines.length, name + ':' + start + ': unclosed code fence');
    const body = lines.slice(start, i).join('\n') + '\n';
    yield { language, body, start };
  }
}

function loadExamples(root, name) {
  const cases = [];
  let source;
  let patch;
  let sourceLanguage;
  let hasResult = false;
  for (const { language, body, start } of readSeedBlocks(root, name)) {
    if (language === 'java' || language === 'vue' || language === 'javascript') {
      assert.ok(!source || hasResult, 'Source has no documented result in ' + name);
      sourceLanguage = language;
      source = body;
      patch = undefined;
      hasResult = false;
    } else if (language === 'diff') {
      assert.ok(source && !patch && !hasResult, 'Preprocessing diff must follow its original source');
      patch = body;
    } else if (language === 'yaml' || (language === 'text' && /^[A-Z][A-Z0-9_]*\n/.test(body))) {
      assert.ok(source, 'Missing original source in ' + name);
      hasResult = true;
      cases.push({
        sourceLanguage, source, patch,
        expected: language === 'yaml' ? YAML.parse(body) : null,
        rejection: language === 'text' ? body : null,
      });
    } else {
      assert.fail(name + ':' + start + ': unsupported seed block ' + JSON.stringify(language));
    }
  }
  assert.ok(hasResult, 'Source has no documented result in ' + name);
  return cases;
}

// One document may contain several entry results (SQL statements or Vue projections).
// Bind by entry identity so changing the order of cases cannot move an oracle.
export function* loadSeedEntries(root, seeds) {
  for (const [seedId, registrations] of Map.groupBy(loadCases(root, seeds), item => item.seedId)) {
    const { document } = registrations[0];
    const fixtures = loadExamples(root, document);
    assert.equal(fixtures.length, registrations.length, 'Full entry inventory for ' + document);
    for (const registration of registrations) {
      const { entry, group, seriesFile } = registration;
      const javascript = group.split('/')[0] === 'javascript';
      const exportName = javascript ? entry.split('#')[1] : entry;
      const matches = fixtures.filter(({ expected, rejection }) => expected
        ? expected.entry.function.slice(expected.entry.function.indexOf('#') + 1) === exportName
        : rejection.includes('\n' + exportName.split('.').at(-1) + ':\n'));
      assert.equal(matches.length, 1, 'Expected one seed example for ' + document + ' at ' + entry);
      const fixture = matches[0];
      const sourceFile = fixture.patch?.match(/^--- a\/(.+)$/m)?.[1] ??
        (javascript ? 'src/' + entry.split('#')[0] : 'src/' + entry.split('.')[0] + '.java');
      assert.equal(fixture.sourceLanguage === 'javascript', javascript, 'JavaScript seed must be in a JavaScript test group');
      assert.ok(sourceFile.endsWith(javascript ? '.js' : '.' + fixture.sourceLanguage), 'Source language and filename disagree: ' + sourceFile);
      if (fixture.expected) {
        assert.equal(fixture.expected.entry.function, sourceFile + '#' + exportName, 'Documented entry identity');
      }
      fixture.sourceFile = sourceFile;
      fixture.mainFile = sourceFile.replace(/\.vue$/, '.mjs');
      const { variants, reject } = loadSeries(join(root, seriesFile));
      assert.equal(Boolean(reject), Boolean(fixture.rejection),
        'Seed result and series rejection contract disagree: ' + seedId + ' / ' + entry);
      if (reject) {
        assert.ok(reject.some(text => !text.startsWith('kind=')), 'Seed rejection requires a specific reason: ' + entry);
      }
      yield { ...registration, entry: exportName, fixture, variants, reject };
    }
  }
}
