import assert from 'node:assert/strict';
import { test } from 'node:test';

import { normalizePairingCode, readSetupProgress, setupReducer } from '../src/lib/onboarding.ts';

const initial = {
  step: 'connect',
  serverUrl: 'https://printer.example.test/',
  computerName: 'Office',
  code: '',
  identity: null,
};
const identity = {
  name: 'Office',
  serverId: 'office',
  serverVersion: '1',
  pairingUrl: 'https://printer.example.test/pair',
  gatewayUrl: 'wss://printer.example.test/gateway',
};

test('only successful discovery and pairing advance setup; Back preserves entered information', () => {
  let state = setupReducer(initial, { type: 'code', value: ' abcd- efgh\n' });
  state = setupReducer(state, { type: 'discovered', identity, serverUrl: initial.serverUrl });
  assert.equal(state.step, 'pair');
  assert.equal(state.code, 'ABCD-EFGH');
  assert.equal(setupReducer(state, { type: 'back' }).computerName, 'Office');
  assert.equal(setupReducer(state, { type: 'back' }).code, 'ABCD-EFGH');
  state = setupReducer(state, { type: 'paired' });
  assert.equal(state.step, 'printers');
  assert.equal(state.code, '');
});

test('deep links skip address entry but require a newly discovered identity', () => {
  const state = setupReducer(
    { ...initial, identity },
    { type: 'link', serverUrl: 'https://other.example.test/', code: 'link-Code' },
  );
  assert.equal(state.step, 'pair');
  assert.equal(state.identity, null);
  assert.equal(state.code, 'link-Code');
});

test('persisted progress accepts only bounded non-secret view state', () => {
  assert.equal(readSetupProgress('broken'), null);
  assert.equal(readSetupProgress(JSON.stringify({ ...initial, step: 'unknown' })), null);
  assert.equal(readSetupProgress(JSON.stringify({ ...initial, computerName: 'a'.repeat(129) })), null);
  assert.deepEqual(readSetupProgress(JSON.stringify({ ...initial, code: 'secret', identity })), {
    step: 'connect',
    serverUrl: initial.serverUrl,
    computerName: 'Office',
  });
});

test('code normalization preserves separators and strips paste whitespace', () => {
  assert.equal(normalizePairingCode('\n aB12- cD34 \t'), 'AB12-CD34');
});

test('codes cannot follow an edited server address and pairing requires a verified identity', () => {
  assert.equal(
    setupReducer({ ...initial, code: 'CODE' }, { type: 'address', value: 'https://other.example.test/' }).code,
    '',
  );
  assert.equal(setupReducer(initial, { type: 'paired' }).step, 'connect');
});
