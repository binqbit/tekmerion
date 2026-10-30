import assert from 'node:assert/strict';
import test from 'node:test';
import { createClient } from '../dist/index.js';
import { decodeCaseIdentity, pda, utf8, address, integerBytes, CORE_ID } from '../dist/wire.js';
import { loadFixture } from './helpers.mjs';

test('portable core decoder and PDA derivation match independently generated runtime fixtures', () => {
  for (const network of ['mainnet', 'devnet']) {
    const fixture = loadFixture(network);
    const client = createClient(network);
    for (const name of ['funded', 'direct-auditing']) {
      const snapshot = fixture.snapshots.find((value) => value.name === name);
      assert(snapshot, name);
      const identity = decodeCaseIdentity(Buffer.from(snapshot.accounts.case.dataHex, 'hex'));
      assert.equal(identity.author, snapshot.caseAddress.author);
      assert.equal(identity.mode, snapshot.expected.mode);
      assert.equal(identity.nonce, BigInt(snapshot.caseAddress.nonce));
      const [key] = pda(
        [
          utf8('case'),
          address(identity.policy),
          address(identity.subject),
          address(identity.author),
          integerBytes(identity.nonce),
        ],
        CORE_ID,
      );
      assert.equal(key, snapshot.accounts.case.key);
      assert.equal(CORE_ID, client.programId.toBase58());
    }
  }
});
