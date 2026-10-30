import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { PublicKey, Transaction } from '@solana/web3.js';
import { BorshInstructionCoder } from '@anchor-lang/core';
import { ConfidentialChainlinkClient } from '../dist/index.js';
import { fixture, binding } from './helpers.mjs';

function setup() {
  const vector = fixture();
  const view = binding(vector);
  return {
    vector,
    view,
    client: new ConfidentialChainlinkClient('mainnet'),
    payer: new PublicKey(vector.profile),
    policy: new PublicKey(vector.policy),
    identity: {
      policy: new PublicKey(vector.policy),
      subject: new PublicKey(view.case.subject),
      author: new PublicKey(view.case.author),
      nonce: view.case.nonce,
    },
    idl: JSON.parse(readFileSync(new URL('../../../idl/chainlink_auditor.json', import.meta.url))),
  };
}

function sameMetas(idl, metas, name) {
  const specification = idl.instructions.find((instruction) => instruction.name === name);
  assert(specification, name);
  assert.equal(metas.length, specification.accounts.length, name);
  assert.deepEqual(
    metas.map((account) => [account.isSigner, account.isWritable]),
    specification.accounts.map((account) => [Boolean(account.signer), Boolean(account.writable)]),
    name,
  );
}

test('the adapter exposes only enrollment, preparation, official reports and permissionless application', () => {
  const { client, identity, view, vector, policy, idl } = setup();
  assert.deepEqual(
    idl.instructions.map((instruction) => instruction.name).sort(),
    [
      'apply_confidential_verdict',
      'on_report',
      'prepare_confidential_request',
      'register_confidential_profile',
    ].sort(),
  );
  sameMetas(idl, client.receiverAccounts(identity, view.profile.config), 'on_report');
  assert.equal(client.profile(policy).toBase58(), vector.profile);
  assert.equal(client.request(identity).toBase58(), vector.request);
});

test('unsigned builders match independent Anchor account privileges and argument codecs', () => {
  const { client, identity, view, payer, policy, idl } = setup();
  const coder = new BorshInstructionCoder(idl);
  for (const [name, instruction] of [
    [
      'register_confidential_profile',
      client.registerProfile(payer, payer, policy, view.profile.config),
    ],
    ['prepare_confidential_request', client.prepareRequest(payer, identity)],
    ['apply_confidential_verdict', client.applyVerdict(identity, payer)],
  ]) {
    assert.equal(instruction.programId.toBase58(), idl.address, name);
    const decoded = coder.decode(instruction.data);
    assert.equal(decoded?.name, name);
    sameMetas(idl, instruction.keys, name);
    assert.deepEqual(coder.encode(name, decoded.data), instruction.data, name);
  }
  assert(client.applyVerdict(identity, payer).keys.every((account) => !account.isSigner));
});

test('atomic enrollment creates the Policy before pinning its exact auditor configuration', () => {
  const { client, view, payer, idl } = setup();
  const identifier = 42n;
  const policy = client.core.policyAddress(payer, identifier)[0];
  const profile = client.profile(policy);
  const input = {
    payer,
    admin: payer,
    identifier,
    clientAuthority: payer,
    rules: {
      auditors: [{ authority: profile, feeRecipient: payer }],
      threshold: 1,
      authorBond: 125n,
      challengeTarget: 100n,
      auditFee: 7n,
      fundingDuration: 60n,
      auditTimeout: 600n,
      auditTrigger: 'onCreation',
    },
  };
  const instructions = client.createPolicy(input, view.profile.config);
  assert.equal(instructions.length, 2);
  assert(instructions[0].programId.equals(client.core.programId));
  assert(instructions[0].keys.some((meta) => meta.pubkey.equals(policy) && meta.isWritable));
  const coreIdl = JSON.parse(readFileSync(new URL('../../../idl/tekmerion.json', import.meta.url)));
  const created = new BorshInstructionCoder(coreIdl).decode(instructions[0].data);
  assert.equal(created.name, 'create_policy');
  assert.equal(created.data.identifier.toString(), identifier.toString());
  assert(created.data.rules.auditors[0].authority.equals(profile));
  assert.deepEqual(
    instructions[1],
    client.registerProfile(payer, payer, policy, view.profile.config),
  );
  const decoded = new BorshInstructionCoder(idl).decode(instructions[1].data);
  assert.equal(decoded.name, 'register_confidential_profile');
  assert.equal(
    Buffer.from(decoded.data.config.agent_config_hash).toString('hex'),
    view.profile.config.agentConfigHash,
  );
  const transaction = new Transaction({ feePayer: payer, recentBlockhash: payer.toBase58() }).add(
    ...instructions,
  );
  assert(
    transaction.serialize({ requireAllSignatures: false, verifySignatures: false }).length <= 1232,
  );
});
