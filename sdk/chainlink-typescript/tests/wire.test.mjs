import assert from 'node:assert/strict';
import test from 'node:test';
import {
  ADAPTER_IDS,
  encodeConfig,
  encodeReport,
  decodeReport,
  retryableFailure,
  hex,
} from '../dist/wire.js';
import { fixture, binding, bytes } from './helpers.mjs';

test('one final report commitment is sufficient for a verdict and extra payload bytes are rejected', () => {
  const report = {
    version: 3,
    stage: 1,
    requestDigest: '11'.repeat(32),
    attemptIndex: 0,
    executionId: '22'.repeat(32),
    reportHash: '33'.repeat(32),
    answer: 0,
    reason: 0,
  };
  const encoded = encodeReport(report);
  assert.equal(encoded.length, 102);
  assert.deepEqual(decodeReport(encoded), report);
  assert.throws(() => decodeReport(new Uint8Array([...encoded, ...new Uint8Array(32)])));
});

for (const network of ['mainnet', 'devnet']) {
  test(`${network}: independent v3 codecs reproduce Rust/SBF binding and execution reports`, () => {
    const vector = fixture(network);
    for (const stage of ['prepared', 'claimed', 'ready', 'applied']) {
      const view = binding(vector, stage);
      assert.equal(view.request.digest, vector.requestDigest);
      assert.equal(view.profile.version, 3);
      assert.equal(view.request.version, 3);
      assert.equal('evidenceHash' in view.request, false);
      assert.equal('aiInputHash' in view.request, false);
      assert.equal('body' in view, false);
      assert.equal('spec' in view, false);
    }
    for (const report of [vector.claim, vector.verdict]) {
      assert.equal(hex(encodeReport(decodeReport(bytes(report)))), report);
    }
    assert.equal(bytes(vector.claim).length, 68);
    assert.equal(bytes(vector.verdict).length, 102);
  });
}

test('Case, Policy, configuration and network substitutions cannot retain a valid binding', () => {
  const vector = fixture();
  const changedAuthor = structuredClone(vector);
  const account = bytes(changedAuthor.snapshots.prepared[2].data);
  account[8 + 32 + 32] ^= 1;
  changedAuthor.snapshots.prepared[2].data = hex(account);
  assert.throws(() => binding(changedAuthor), /Invalid Case/);

  const changedConfig = structuredClone(vector);
  const profile = bytes(changedConfig.snapshots.prepared[0].data);
  profile[profile.length - 3] ^= 1; // Agent configuration commitment.
  changedConfig.snapshots.prepared[0].data = hex(profile);
  assert.throws(() => binding(changedConfig), /digest/);

  assert.equal(ADAPTER_IDS.mainnet, ADAPTER_IDS.devnet);
  assert.throws(() => binding({ ...vector, network: 'devnet' }), /binding|domain|digest/);
  assert.notEqual(fixture('mainnet').requestDigest, fixture('devnet').requestDigest);
});

test('v3 reports preserve execution identity and reject malformed or ambiguous results', () => {
  const vector = fixture();
  const report = decodeReport(bytes(vector.verdict));
  assert.equal(report.version, 3);
  assert.equal('aiInputHash' in report, false);
  assert.equal('taskDigest' in report, false);
  const otherExecution = { ...report, executionId: '02'.repeat(32) };
  assert.notEqual(hex(encodeReport(otherExecution)), hex(encodeReport(report)));
  assert.throws(() => decodeReport(encodeReport(report).slice(0, -1)));
  assert.throws(() => decodeReport(new Uint8Array([...encodeReport(report), 0])));
  assert.throws(() => encodeReport({ ...report, version: 2 }));
  assert.throws(() => encodeReport({ ...report, answer: 0, reason: 5 }));
  assert.throws(() => encodeReport({ ...report, attemptIndex: 65536 }));
  assert.throws(() => encodeReport({ ...report, executionId: '00'.repeat(32) }));
  assert.throws(() => retryableFailure(5));
  assert.throws(() => retryableFailure(1.5));
  for (const code of [1, 2, 3, 4]) assert.equal(retryableFailure(code), true);
  for (const code of [64, 65, 66, 67, 68, 69, 70]) assert.equal(retryableFailure(code), false);
});

test('profile configuration pins workflow and agent identity with bounded retry limits', () => {
  const config = binding(fixture()).profile.config;
  assert.equal(encodeConfig(config).length, 130);
  assert.throws(() => encodeConfig({ ...config, maxAttempts: 0 }));
  assert.throws(() => encodeConfig({ ...config, maxAttempts: 65536 }));
  assert.throws(() => encodeConfig({ ...config, agentConfigHash: '00'.repeat(32) }));
  assert.throws(() => encodeConfig({ ...config, workflowId: '00'.repeat(32) }));
  assert.throws(() => encodeConfig({ ...config, workflowOwner: '00'.repeat(20) }));
});

test('the audit trigger is part of the immutable Chainlink Policy commitment', () => {
  const vector = structuredClone(fixture());
  const policy = bytes(vector.snapshots.prepared[1].data);
  // Policy identity, one-member committee, quorum, and five economic/time terms.
  const trigger = 8 + 32 + 8 + 32 + 4 + 64 + 1 + 5 * 8;
  assert.equal(policy[trigger], 0);
  policy[trigger] = 1;
  vector.snapshots.prepared[1].data = hex(policy);
  assert.throws(() => binding(vector), /Policy changed|Policy requires a direct audit/);
});
