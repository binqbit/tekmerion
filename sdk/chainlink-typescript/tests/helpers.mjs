import { readFileSync } from 'node:fs';
import { unhex, validateConfidentialBinding } from '../dist/wire.js';

export const bytes = (value) => unhex(value, value.length / 2);
export const fixture = (network = 'mainnet') =>
  JSON.parse(
    readFileSync(
      new URL(`../../fixtures/chainlink/confidential-${network}.json`, import.meta.url),
      'utf8',
    ),
  );

export function binding(vector, stage = 'prepared') {
  const accounts = vector.snapshots[stage];
  return validateConfidentialBinding({
    network: vector.network,
    profileKey: vector.profile,
    profileBytes: bytes(accounts[0].data),
    policyBytes: bytes(accounts[1].data),
    caseKey: vector.case,
    caseBytes: bytes(accounts[2].data),
    requestKey: vector.request,
    requestBytes: bytes(accounts[3].data),
  });
}
