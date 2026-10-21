import { readFileSync } from 'node:fs';
import { PublicKey } from '@solana/web3.js';
import Anchor from '@anchor-lang/core';

export const pubkey = (value) => new PublicKey(value);
export const loadFixture = (network = 'mainnet') =>
  JSON.parse(readFileSync(new URL(`../../fixtures/sdk-${network}.json`, import.meta.url), 'utf8'));
export const loadIdl = (network = 'mainnet') =>
  JSON.parse(readFileSync(new URL(`../src/idl/${network}.json`, import.meta.url), 'utf8'));
export const observation = (value) => ({
  slot: BigInt(value.slot),
  unixTimestamp: BigInt(value.unixTimestamp),
});
export const identity = (value) => ({
  policy: pubkey(value.policy),
  subject: pubkey(value.subject),
  author: pubkey(value.author),
  nonce: BigInt(value.nonce),
});
export const account = (value) => ({
  key: pubkey(value.key),
  owner: pubkey(value.owner),
  executable: value.executable,
  data: Buffer.from(value.dataHex, 'hex'),
  slot: BigInt(value.slot),
});
export const accounts = (value) =>
  Object.fromEntries(Object.entries(value).map(([name, item]) => [name, account(item)]));
export const rules = (value) => ({
  ...value,
  auditors: value.auditors.map((auditor) => ({
    authority: pubkey(auditor.authority),
    feeRecipient: pubkey(auditor.feeRecipient),
  })),
  ...Object.fromEntries(
    ['authorBond', 'challengeTarget', 'auditFee', 'fundingDuration', 'auditTimeout'].map((key) => [
      key,
      BigInt(value[key]),
    ]),
  ),
});
export const normalize = (value) =>
  JSON.parse(
    JSON.stringify(value, (_key, item) => (typeof item === 'bigint' ? item.toString() : item)),
  );
export const snapshot = (name, fixture = loadFixture()) =>
  fixture.snapshots.find((item) => item.name === name);
export function readSnapshot(client, fixtureSnapshot) {
  return client.readCase(
    observation(fixtureSnapshot.observation),
    identity(fixtureSnapshot.caseAddress),
    accounts(fixtureSnapshot.accounts),
  );
}
// Synthetic state corruption uses the maintained IDL rather than hard-coded state offsets.
export async function mutateAccount(value, type, mutate, network = 'mainnet') {
  const result = account(value);
  const coder = new Anchor.BorshCoder(loadIdl(network));
  const decoded = coder.accounts.decode(type, result.data);
  mutate(decoded);
  const encoded = await coder.accounts.encode(type, decoded);
  result.data.fill(0);
  encoded.copy(result.data);
  return result;
}
