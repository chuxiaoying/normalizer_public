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
  const sourceMarkdown = readFileSync(join(root, name), 'utf8');
  assert.match(sourceMarkdown, /^# [^\r\n]+\r?\n\r?\n## 设计意图\r?\n\r?\n\S/, 'Missing top design intent: ' + name);
  const cases = [];
  let source;
  let patch;
  let sourceLanguage;
  let hasResult = false;
  for (const { language, body, start } of readSeedBlocks(root, name)) {
    if (language === 'java' || language === 'vue' || language === 'javascript' || language === 'typescript') {
      assert.ok(!source || hasResult, 'Source has no documented result in ' + name);
      sourceLanguage = language;
      source = body;
      patch = undefined;
      hasResult = false;
    } else if (language === 'diff-example') {
      continue; // Explicitly non-executable comparison of source layout.
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
      const language = group.split('/')[0];
      assert.ok(['java', 'typescript', 'vue'].includes(language), 'No formal CLI route for ' + language);
      const extension = language === 'typescript' ? '.ts' : undefined;
      const exportName = extension ? entry.split('#')[1] : entry;
      const matches = fixtures.filter(({ expected, rejection }) => expected
        ? expected.entry.function.slice(expected.entry.function.indexOf('#') + 1) === exportName
        : rejection.includes('\n' + exportName.split('.').at(-1) + ':\n'));
      assert.equal(matches.length, 1, 'Expected one seed example for ' + document + ' at ' + entry);
      const fixture = matches[0];
      const expectedLocator = fixture.expected?.entry?.function;
      const expectedFile = expectedLocator?.split('#', 1)[0];
      const patchOriginal = fixture.patch?.match(/^--- a\/(.+)$/m)?.[1];
      const patchPrepared = fixture.patch?.match(/^\+\+\+ b\/(.+)$/m)?.[1];
      const rejectionFile = fixture.rejection?.match(/^\s*location: (src\/[^#\s]+)#/m)?.[1];
      const sourceFile = patchOriginal ?? expectedFile ?? rejectionFile ??
        (extension ? 'src/' + entry.split('#')[0] : 'src/' + entry.split('.')[0] + '.java');
      const mainFile = patchPrepared ?? sourceFile;
      const sourceExtension = { java: '.java', javascript: '.js', typescript: '.ts', vue: '.vue' }[fixture.sourceLanguage];
      assert.ok(sourceExtension && sourceFile.endsWith(sourceExtension),
        'Raw source language and filename disagree: ' + sourceFile);
      const preparedExtension = language === 'vue' ? '.ts' : extension ?? '.java';
      assert.ok(mainFile.endsWith(preparedExtension),
        'Prepared source language and test group disagree: ' + mainFile);
      assert.ok((language === 'vue') === (fixture.sourceLanguage === 'vue'),
        'A Vue scenario must retain its original Vue source');
      if (extension) {
        assert.equal(mainFile, 'src/' + entry.split('#')[0], 'Prepared module matches its registered entry');
      }
      const entryFunction = mainFile + '#' + exportName;
      if (expectedLocator) assert.equal(expectedLocator, entryFunction, 'Documented entry identity');
      fixture.sourceFile = sourceFile;
      fixture.mainFile = mainFile;
      fixture.entryFunction = entryFunction;
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
