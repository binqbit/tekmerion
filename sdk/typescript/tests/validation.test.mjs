import assert from 'node:assert/strict';
import test from 'node:test';
import { PublicKey } from '@solana/web3.js';
import { createClient } from '../dist/index.js';
import {
  account,
  accounts,
  identity,
  loadFixture,
  mutateAccount,
  observation,
  pubkey,
  readSnapshot,
  snapshot,
} from './helpers.mjs';

const client = createClient();
const fixture = loadFixture();
const evaluate = (
  current,
  supplied = accounts(current.accounts),
  observed = observation(current.observation),
) => client.readCase(observed, identity(current.caseAddress), supplied);
const fail = (callback, code) => assert.throws(callback, { code });

test('builders preserve full u64 values and reject number coercion or out-of-range bigint', () => {
  const caseAddress = identity(fixture.caseAddress);
  const input = {
    payer: pubkey(fixture.actors.payer),
    owner: pubkey(fixture.actors.alice),
    source: pubkey(fixture.instructions.find((value) => value.name === 'contribute').input.source),
  };
  for (const value of [1, Number.MAX_SAFE_INTEGER, -1n, 1n << 64n])
    fail(() => client.contribute(caseAddress, input, value), 'InvalidArgument');
  fail(() => client.caseAddress({ ...caseAddress, nonce: -1n }), 'InvalidArgument');
  fail(() => createClient('testnet'), 'InvalidArgument');
  const instruction = client.contribute(caseAddress, input, (1n << 64n) - 1n);
  assert.equal(instruction.data.readBigUInt64LE(8), (1n << 64n) - 1n);
});

test('author identity separates same-subject cases and authenticates observed state', async () => {
  const current = snapshot('funded');
  const original = identity(current.caseAddress);
  const other = { ...original, author: pubkey(fixture.actors.alice) };
  assert.notDeepEqual(client.caseAddress(original), client.caseAddress(other));
  assert.notDeepEqual(client.vaultAddress(original), client.vaultAddress(other));
  fail(
    () => client.readCase(observation(current.observation), other, accounts(current.accounts)),
    'InvalidRelationship',
  );
  const supplied = accounts(current.accounts);
  supplied.case = await mutateAccount(current.accounts.case, 'Case', (state) => {
    state.author = other.author;
  });
  fail(() => evaluate(current, supplied), 'InvalidRelationship');
});

test('authenticated snapshots cannot be modified through source bytes or returned getters', () => {
  const current = snapshot('yes-partial-vote');
  const supplied = accounts(current.accounts);
  const view = evaluate(current, supplied);
  const member = pubkey(fixture.actors.auditor2);
  const initial = view.quoteAudit(member, 'yes', 'decisive');
  assert.equal(initial.length, 2);
  supplied.case.data.fill(0);
  view.case.audit.votes.length = 0;
  view.policy.rules.auditFee = 100n;
  view.policy.rules.auditors.length = 0;
  view.observation.unixTimestamp = 99999n;
  view.authorReceipt.claimed = true;
  const returned = view.quoteAudit(member, 'yes', 'decisive');
  returned[0].feeRecipient._bn.words.fill(0);
  assert.deepEqual(view.quoteAudit(member, 'yes', 'decisive'), initial);
});

test('subject is optional while the canonical Case still authenticates its identity', () => {
  const current = snapshot('unknown-finalized');
  const supplied = accounts(current.accounts);
  const original = evaluate(current, supplied);
  delete supplied.subject;

  const withoutSubject = evaluate(current, supplied);

  assert.deepEqual(withoutSubject.case, original.case);
  assert.deepEqual(withoutSubject.authorClaim(), original.authorClaim());
  assert.deepEqual(
    withoutSubject.contributorClaim(pubkey(fixture.actors.alice), supplied.aliceReceipt),
    original.contributorClaim(pubkey(fixture.actors.alice), supplied.aliceReceipt),
  );
  fail(
    () =>
      client.readCase(
        observation(current.observation),
        { ...identity(current.caseAddress), subject: PublicKey.default },
        supplied,
      ),
    'InvalidRelationship',
  );
});

test('an explicitly supplied subject must retain the expected address', () => {
  const current = snapshot('funded');
  const supplied = accounts(current.accounts);
  supplied.subject.key = PublicKey.default;

  fail(() => evaluate(current, supplied), 'InvalidRelationship');
});

for (const role of ['case', 'policy', 'authorReceipt', 'vault', 'subject']) {
  test(`rejects mixed observation slots and executable ${role}`, () => {
    const current = snapshot('funded');
    const supplied = accounts(current.accounts);
    supplied[role].slot--;
    fail(() => evaluate(current, supplied), 'InconsistentObservation');
    supplied[role].slot++;
    supplied[role].executable = true;
    fail(() => evaluate(current, supplied), 'InvalidAccount');
  });
}
for (const role of ['case', 'policy', 'authorReceipt', 'vault']) {
  test(`rejects foreign owner, truncated allocation and substituted ${role} address`, () => {
    const current = snapshot('funded');
    let supplied = accounts(current.accounts);
    supplied[role].owner = PublicKey.default;
    fail(() => evaluate(current, supplied), 'InvalidAccount');
    supplied = accounts(current.accounts);
    supplied[role].data = supplied[role].data.subarray(0, -1);
    fail(() => evaluate(current, supplied), 'InvalidAccount');
    supplied = accounts(current.accounts);
    supplied[role].key = PublicKey.default;
    fail(() => evaluate(current, supplied), 'InvalidRelationship');
  });
}

test('rejects discriminator, noncanonical bool and noncanonical option encodings', () => {
  const current = snapshot('funded');
  let supplied = accounts(current.accounts);
  supplied.case.data[0] ^= 1;
  fail(() => evaluate(current, supplied), 'InvalidAccount');
  supplied = accounts(current.accounts);
  // StakeReceipt has fixed fields; claimed is the byte before its final bump.
  supplied.authorReceipt.data[supplied.authorReceipt.data.length - 2] = 2;
  fail(() => evaluate(current, supplied), 'InvalidAccount');
  supplied = accounts(current.accounts);
  // First Case option follows policy/subject/author/nonce; None -> invalid tag.
  supplied.case.data[8 + 32 + 32 + 32 + 8] = 2;
  fail(() => evaluate(current, supplied), 'InvalidAccount');
});

for (const [name, role, type, mutation, code] of [
  [
    'wrong Case bump',
    'case',
    'Case',
    (state) => {
      state.bump ^= 1;
    },
    'InvalidRelationship',
  ],
  [
    'foreign Case policy',
    'case',
    'Case',
    (state) => {
      state.policy = PublicKey.default;
    },
    'InvalidRelationship',
  ],
  [
    'invalid policy threshold',
    'policy',
    'Policy',
    (state) => {
      state.rules.threshold = 1;
    },
    'InvalidState',
  ],
  [
    'duplicate committee authority',
    'policy',
    'Policy',
    (state) => {
      state.rules.auditors[1].authority = state.rules.auditors[0].authority;
    },
    'InvalidState',
  ],
  [
    'author receipt role',
    'authorReceipt',
    'StakeReceipt',
    (state) => {
      state.side = 1;
    },
    'InvalidState',
  ],
  [
    'author receipt principal',
    'authorReceipt',
    'StakeReceipt',
    (state) => {
      state.principal = state.principal.addn(1);
    },
    'InvalidState',
  ],
  [
    'author receipt owner',
    'authorReceipt',
    'StakeReceipt',
    (state) => {
      state.owner = PublicKey.default;
    },
    'InvalidRelationship',
  ],
  [
    'premature author claim marker',
    'authorReceipt',
    'StakeReceipt',
    (state) => {
      state.claimed = true;
    },
    'InvalidState',
  ],
]) {
  test(`synthetic corruption: ${name}`, async () => {
    const current = snapshot('funded');
    const supplied = accounts(current.accounts);
    supplied[role] = await mutateAccount(current.accounts[role], type, mutation);
    fail(() => evaluate(current, supplied), code);
  });
}
for (const [name, mutation] of [
  [
    'duplicate vote',
    (state) => {
      state.audit.votes.push(state.audit.votes[0]);
    },
  ],
  [
    'foreign auditor index',
    (state) => {
      state.audit.votes[0].auditor_index = 9;
    },
  ],
  [
    'invalid vote reason',
    (state) => {
      state.audit.votes[0].response.reason = { MissingEvidence: {} };
    },
  ],
]) {
  test(`synthetic corruption: ${name}`, async () => {
    const current = snapshot('yes-partial-vote');
    const supplied = accounts(current.accounts);
    supplied.case = await mutateAccount(current.accounts.case, 'Case', mutation);
    fail(() => evaluate(current, supplied), 'InvalidState');
  });
}
for (const [name, mutation] of [
  [
    'missing quorum',
    (state) => {
      state.audit.votes.shift();
    },
  ],
  [
    'fees not paid',
    (state) => {
      state.fee_claimed = false;
    },
  ],
  [
    'incorrect finalization time',
    (state) => {
      state.finalized_at = state.audit.deadline;
    },
  ],
]) {
  test(`synthetic terminal corruption: ${name}`, async () => {
    const current = snapshot('yes-finalized');
    const supplied = accounts(current.accounts);
    supplied.case = await mutateAccount(current.accounts.case, 'Case', mutation);
    fail(() => evaluate(current, supplied), 'InvalidState');
  });
}

test('vault rejects wrong mint, authority, delegate, frozen state and insufficient backing', () => {
  const current = snapshot('funded');
  for (const mutate of [
    (bytes) => bytes.fill(0, 0, 32),
    (bytes) => bytes.fill(0, 32, 64),
    (bytes) => bytes.writeUInt32LE(1, 72),
    (bytes) => {
      bytes[108] = 2;
    },
  ]) {
    const supplied = accounts(current.accounts);
    mutate(supplied.vault.data);
    fail(() => evaluate(current, supplied), 'InvalidAccount');
  }
  const supplied = accounts(current.accounts);
  supplied.vault.data.writeBigUInt64LE(224n, 64);
  fail(() => evaluate(current, supplied), 'InsufficientBacking');
});

test('author receipt cannot impersonate contributor; foreign or premature contributor claim rejected', async () => {
  const current = snapshot('funded');
  const view = readSnapshot(client, current);
  fail(
    () =>
      view.contributorClaim(pubkey(fixture.actors.author), account(current.accounts.authorReceipt)),
    'InvalidState',
  );
  fail(
    () => view.contributorClaim(pubkey(fixture.actors.bob), account(current.accounts.aliceReceipt)),
    'InvalidRelationship',
  );
  const changed = await mutateAccount(current.accounts.aliceReceipt, 'StakeReceipt', (receipt) => {
    receipt.claimed = true;
  });
  fail(() => view.contributorClaim(pubkey(fixture.actors.alice), changed), 'InvalidState');
});

test('quotes enforce exact deadline, valid response pairs and zero-fee quorum destinations', async () => {
  const current = snapshot('yes-partial-vote');
  const supplied = accounts(current.accounts);
  const member = pubkey(fixture.actors.auditor2);
  const valid = evaluate(current, supplied);
  fail(() => valid.quoteAudit(member, 'yes', 'missingEvidence'), 'ActionUnavailable');
  fail(() => valid.quoteAudit(member, 'unknown', 'decisive'), 'ActionUnavailable');
  fail(
    () =>
      evaluate(current, supplied, {
        ...observation(current.observation),
        unixTimestamp: valid.case.audit.deadline,
      }).quoteAudit(member, 'yes', 'decisive'),
    'ActionUnavailable',
  );
  supplied.policy = await mutateAccount(current.accounts.policy, 'Policy', (policy) => {
    policy.rules.audit_fee = policy.rules.audit_fee.sub(policy.rules.audit_fee);
  });
  const zero = evaluate(current, supplied).quoteAudit(member, 'yes', 'decisive');
  assert.equal(zero.length, 2);
  assert.deepEqual(
    zero.map((payout) => payout.amount),
    [0n, 0n],
  );
});

for (const field of ['agreement_hash', 'result_hash']) {
  test(`rejects a missing ${field} commitment in otherwise valid Case state`, async () => {
    const current = snapshot('funded');
    const supplied = accounts(current.accounts);
    supplied.case = await mutateAccount(current.accounts.case, 'Case', (state) => {
      state[field] = Array(32).fill(0);
    });
    fail(() => evaluate(current, supplied), 'InvalidState');
  });
}
