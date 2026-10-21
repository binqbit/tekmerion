import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { createClient } from '../dist/index.js';
import {
  account,
  accounts,
  identity,
  loadFixture,
  normalize,
  observation,
  pubkey,
  rules,
} from './helpers.mjs';

function build(client, vector) {
  const value = vector.input;
  const caseAddress = value.caseAddress && identity(value.caseAddress);
  switch (vector.name) {
    case 'createPolicy':
      return client.createPolicy({
        payer: pubkey(value.payer),
        admin: pubkey(value.admin),
        clientAuthority: pubkey(value.clientAuthority),
        identifier: BigInt(value.identifier),
        rules: rules(value.rules),
      });
    case 'updatePolicy':
      return client.updatePolicy(pubkey(value.admin), BigInt(value.identifier), rules(value.rules));
    case 'acquirePolicy':
      return client.acquirePolicy(
        pubkey(value.policy),
        pubkey(value.clientAuthority),
        pubkey(value.subject),
      );
    case 'createCase':
      return client.createCase(caseAddress, {
        payer: pubkey(value.payer),
        clientAuthority: pubkey(value.clientAuthority),
        author: pubkey(value.author),
        source: pubkey(value.source),
        funding:
          value.funding.kind === 'bonded'
            ? { kind: 'bonded', principal: BigInt(value.funding.principal) }
            : { kind: 'direct', expectedFee: BigInt(value.funding.expectedFee) },
        agreementHash: Buffer.from(value.agreementHashHex, 'hex'),
        resultHash: Buffer.from(value.resultHashHex, 'hex'),
      });
    case 'contribute':
      return client.contribute(
        caseAddress,
        { payer: pubkey(value.payer), owner: pubkey(value.owner), source: pubkey(value.source) },
        BigInt(value.requested),
      );
    case 'submitAudit':
      return client.submitAudit(
        caseAddress,
        pubkey(value.auditor),
        value.answer,
        value.reason,
        value.feeDestinations.map(pubkey),
      );
    case 'finalizeUnchallenged':
      return client.finalizeUnchallenged(caseAddress);
    case 'finalizeTimeout':
      return client.finalizeTimeout(caseAddress);
    case 'claim':
      return client.claim(caseAddress, pubkey(value.author), {
        owner: pubkey(value.owner),
        destination: pubkey(value.destination),
      });
    default:
      assert.fail(`Unhandled Rust vector ${vector.name}`);
  }
}
for (const network of ['mainnet', 'devnet']) {
  const fixture = loadFixture(network);
  const client = createClient(network);
  test(`${network}: bundled IDL exactly matches the maintained deployment IDL`, () => {
    const maintained =
      network === 'mainnet' ? '../../../idl/tekmerion.json' : '../../../idl/devnet/tekmerion.json';
    assert.deepEqual(
      readFileSync(new URL(`../src/idl/${network}.json`, import.meta.url)),
      readFileSync(new URL(maintained, import.meta.url)),
    );
    assert.equal(client.programId.toBase58(), fixture.programId);
    assert.equal(client.mint.toBase58(), fixture.mint);
  });
  test(`${network}: every unsigned instruction matches independent Rust bytes, PDAs and privileges`, () => {
    assert.equal(new Set(fixture.instructions.map((value) => value.name)).size, 9);
    for (const vector of fixture.instructions) {
      const instruction = build(client, vector);
      assert.deepEqual(
        {
          programId: instruction.programId.toBase58(),
          accounts: normalize(instruction.keys),
          dataHex: instruction.data.toString('hex'),
        },
        { programId: vector.programId, accounts: vector.accounts, dataHex: vector.dataHex },
        vector.label,
      );
    }
  });
  test(`${network}: a policy before first use remains readable`, () => {
    const input = fixture.policyBeforeCase;
    const view = client.readPolicy(
      observation(input.observation),
      pubkey(input.account.key),
      account(input.account),
    );
    assert.equal(view.policy.activeUses, 0n);
    assert.deepEqual(normalize(view.policy.rules), fixture.policyInput.rules);
    assert.deepEqual(
      [
        client.accountSize('Policy'),
        client.accountSize('Case'),
        client.accountSize('StakeReceipt'),
      ],
      [710, 356, 83],
    );
  });
  for (const current of fixture.snapshots) {
    test(`${network}: Rust account and quote parity: ${current.name}`, () => {
      const supplied = accounts(current.accounts);
      const view = client.readCase(
        observation(current.observation),
        identity(current.caseAddress),
        supplied,
      );
      const expected = current.expected;
      assert.equal(view.case.mode, expected.mode);
      assert.equal(view.case.phase, expected.phase);
      assert.equal(view.case.result, expected.result);
      assert.equal(view.completionMethod, expected.completion);
      assert.equal(view.vaultBalance.toString(), expected.vaultBalance);
      assert.deepEqual(normalize(view.authorClaim()), expected.authorClaim);
      assert.equal(view.auditFeesPaid()?.toString() ?? null, expected.auditFeesPaid);
      const state = view.case;
      const unpaid =
        (view.authorReceipt.claimed ? 0n : state.authorEntitlement) +
        state.contributorUnpaid +
        (state.feeClaimed ? 0n : state.feeEntitlement);
      assert.equal(unpaid.toString(), expected.unpaid);
      for (const name of ['alice', 'bob']) {
        if (expected[`${name}Claim`])
          assert.deepEqual(
            normalize(
              view.contributorClaim(pubkey(fixture.actors[name]), supplied[`${name}Receipt`]),
            ),
            expected[`${name}Claim`],
          );
      }
      for (const quote of expected.contributionQuotes) {
        const evaluate = () => view.quoteContribution(BigInt(quote.requested));
        if (quote.error) assert.throws(evaluate, { code: quote.error });
        else assert.equal(evaluate().toString(), quote.amount);
      }
      for (const quote of expected.auditQuotes) {
        const evaluate = () => view.quoteAudit(pubkey(quote.auditor), quote.answer, quote.reason);
        if (quote.error) assert.throws(evaluate, { code: quote.error });
        else assert.deepEqual(normalize(evaluate()), quote.payouts);
      }
    });
  }
}
