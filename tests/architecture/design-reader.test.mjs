import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { test } from 'node:test';
import { claimReader, listenReader } from '../../implementation/apps/design-reader/lifecycle.mjs';

const host = '127.0.0.1';

test('an unrelated process keeps its port and the reader listens on a free loopback port', async t => {
  const occupied = createServer((req, res) => res.end('other app')).listen(0, host);
  t.after(() => occupied.close());
  await once(occupied, 'listening');
  const defaultPort = occupied.address().port;
  const reader = createServer((req, res) => res.end('reader'));
  t.after(() => reader.close());
  await listenReader(reader, { host, defaultPort, identity: 'design reader', appDir: '/unused' });

  const address = reader.address();
  assert.equal(address.address, host);
  assert.notEqual(address.port, defaultPort);
  assert.equal(await (await fetch(`http://${host}:${defaultPort}`)).text(), 'other app');
  assert.equal(await (await fetch(`http://${host}:${address.port}`)).text(), 'reader');
});

test('claiming the same checkout shuts down its previous instance, not another checkout', async t => {
  const checkout = randomUUID();
  let stops = 0;
  const prior = await claimReader(checkout, () => { stops++; });
  t.after(() => prior.close());
  const other = await claimReader(randomUUID(), () => assert.fail('stopped another checkout'));
  t.after(() => other.close());

  const next = await claimReader(checkout, () => assert.fail('stopped the replacement'));
  t.after(() => next.close());
  assert.equal(stops, 1);
  assert.equal(prior.listening, false);
  assert.equal(other.listening, true);
  assert.equal(next.listening, true);
});
