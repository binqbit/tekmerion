import assert from 'node:assert/strict';
import test from 'node:test';
import Anchor from '@anchor-lang/core';
import {
  Keypair,
  PublicKey,
  SYSVAR_CLOCK_PUBKEY,
  SystemProgram,
  Transaction,
  TransactionMessage,
  VersionedTransaction,
} from '@solana/web3.js';
import { TekmerionRpc } from '../dist/index.js';
import { account, identity, loadFixture, normalize, pubkey, snapshot } from './helpers.mjs';

const fixture = loadFixture();
const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
class MockConnection {
  current = snapshot('funded');
  listeners = new Map();
  removed = [];
  requests = [];
  sent = [];
  confirmed = [];
  simulations = [];
  lifetime = { blockhash: Keypair.generate().publicKey.toBase58(), lastValidBlockHeight: 999 };
  confirmationError = null;
  getResponse(keys) {
    const known = new Map(
      Object.values(this.current.accounts).map((value) => [value.key, account(value)]),
    );
    return {
      context: { slot: Number(this.current.observation.slot) },
      value: keys.map((key) => {
        const found = known.get(key.toBase58());
        return found
          ? {
              data: Buffer.from(found.data),
              owner: found.owner,
              executable: found.executable,
              lamports: 1,
            }
          : null;
      }),
    };
  }
  async getMultipleAccountsInfoAndContext(keys, config) {
    this.requests.push({ keys, config });
    const response = this.getResponse(keys);
    return this.intercept ? this.intercept(response, keys) : response;
  }
  onAccountChange(key, callback, commitment) {
    const id = this.listeners.size + 1;
    if (this.failRegistration === id) throw new Error('registration failed');
    this.listeners.set(id, { key, callback, commitment });
    return id;
  }
  async removeAccountChangeListener(id) {
    this.removed.push(id);
    this.listeners.delete(id);
    if (this.failCleanup === id) throw new Error('cleanup failed');
  }
  emit(key, atSlot) {
    for (const listener of this.listeners.values())
      if (listener.key.equals(key)) listener.callback({}, { slot: atSlot });
  }
  async getLatestBlockhash(commitment) {
    assert.equal(commitment, 'confirmed');
    return this.lifetime;
  }
  async simulateTransaction(transaction, config) {
    this.simulations.push({ transaction, config });
    return {
      context: { slot: 321 },
      value: {
        err: { InstructionError: [0, { Custom: 6000 }] },
        logs: ['program rejected'],
        unitsConsumed: 100,
      },
    };
  }
  async sendRawTransaction(bytes, options) {
    this.sent.push({ bytes: Buffer.from(bytes), options });
    if (this.sendError) throw this.sendError;
    const transaction = VersionedTransaction.deserialize(bytes);
    return this.wrongSignature ?? Anchor.utils.bytes.bs58.encode(transaction.signatures[0]);
  }
  async confirmTransaction(strategy, commitment) {
    this.confirmed.push({ strategy, commitment });
    return { context: { slot: 345 }, value: { err: this.confirmationError } };
  }
}
function setup(name = 'funded') {
  const connection = new MockConnection();
  connection.current = snapshot(name);
  return {
    connection,
    rpc: new TekmerionRpc(connection),
    address: identity(connection.current.caseAddress),
  };
}
function unsignedTransfer(payer, owner = payer) {
  const transaction = new Transaction();
  transaction.feePayer = payer.publicKey;
  return transaction.add(
    SystemProgram.transfer({
      fromPubkey: owner.publicKey,
      toPubkey: PublicKey.default,
      lamports: 7n,
    }),
  );
}
function signedTransfer(connection, signer = Keypair.generate()) {
  const transaction = unsignedTransfer(signer);
  transaction.recentBlockhash = connection.lifetime.blockhash;
  transaction.sign(signer);
  return { transaction, signer };
}

test('RPC Case discovery is followed by one complete contextual batch including Clock and author receipt', async () => {
  const { connection, rpc, address } = setup('no-finalized');
  const fetched = await rpc.fetchContributorClaim(address, pubkey(fixture.actors.alice), 100n);
  assert.deepEqual(normalize(fetched.claim), connection.current.expected.aliceClaim);
  assert.equal(connection.requests.length, 2);
  assert.equal(connection.requests[0].keys.length, 2);
  assert.equal(connection.requests[1].keys.length, 6);
  assert(connection.requests.every(({ keys }) => keys.at(-1).equals(SYSVAR_CLOCK_PUBKEY)));
  assert.equal(
    connection.requests[1].config.minContextSlot,
    Number(connection.current.observation.slot),
  );
  assert.equal(fetched.view.observation.slot, BigInt(connection.current.observation.slot));
  assert.equal(
    fetched.view.observation.unixTimestamp,
    BigInt(connection.current.observation.unixTimestamp),
  );
});

test('RPC policy reads authenticate the same-batch Clock and unused policy', async () => {
  const connection = new MockConnection();
  const before = fixture.policyBeforeCase;
  connection.current = {
    observation: before.observation,
    accounts: { policy: before.account, clock: before.clockAccount },
  };
  const view = await new TekmerionRpc(connection).fetchPolicy(pubkey(before.account.key));
  assert.equal(view.policy.activeUses, 0n);
  assert.equal(connection.requests.length, 1);
});

for (const presence of ['absent', 'closed']) {
  test(`RPC reads and claims remain available when the subject is ${presence}`, async () => {
    const { connection, rpc, address } = setup('unknown-finalized');
    if (presence === 'closed') await rpc.fetchCase(address);
    delete connection.current.accounts.subject;
    connection.requests.length = 0;

    const view = await rpc.fetchCase(address);
    const { claim } = await rpc.fetchContributorClaim(address, pubkey(fixture.actors.alice));

    assert.deepEqual(normalize(view.authorClaim()), connection.current.expected.authorClaim);
    assert.deepEqual(normalize(claim), connection.current.expected.aliceClaim);
    assert(view.case.subject.equals(address.subject));
    assert(
      connection.requests.every(({ keys }) => !keys.some((key) => key.equals(address.subject))),
    );
  });
}

for (const role of ['case', 'policy', 'vault', 'authorReceipt', 'aliceReceipt', 'clock']) {
  test(`RPC rejects missing required ${role} even when the opaque subject is absent`, async () => {
    const { connection, rpc, address } = setup('no-finalized');
    const missing = connection.current.accounts[role].key;
    delete connection.current.accounts.subject;
    delete connection.current.accounts[role];

    await assert.rejects(rpc.fetchContributorClaim(address, pubkey(fixture.actors.alice)), {
      code: 'InvalidAccount',
      message: `Account missing: ${missing}`,
    });
  });
}

for (const [name, mutate, code] of [
  [
    'stale slot',
    (response) => {
      response.context.slot = 99;
    },
    'InconsistentObservation',
  ],
  [
    'unsafe numeric slot',
    (response) => {
      response.context.slot = Number.MAX_SAFE_INTEGER + 1;
    },
    'InconsistentObservation',
  ],
  [
    'missing Clock',
    (response) => {
      response.value[response.value.length - 1] = null;
    },
    'InvalidAccount',
  ],
  [
    'foreign Clock owner',
    (response) => {
      response.value.at(-1).owner = PublicKey.default;
    },
    'InvalidAccount',
  ],
  [
    'mismatched Clock slot',
    (response) => {
      response.value.at(-1).data.writeBigUInt64LE(99n, 0);
    },
    'InconsistentObservation',
  ],
  [
    'truncated response',
    (response) => {
      response.value.pop();
    },
    'InvalidAccount',
  ],
  [
    'extra response account',
    (response) => {
      response.value.push(null);
    },
    'InvalidAccount',
  ],
]) {
  test(`RPC rejects ${name}`, async () => {
    const { connection, rpc, address } = setup();
    connection.intercept = (response) => {
      mutate(response);
      return response;
    };
    await assert.rejects(rpc.fetchCase(address, 100n), { code });
  });
}

test('RPC rejects a second batch that regresses below the discovery slot', async () => {
  const { connection, rpc, address } = setup();
  connection.intercept = (response, keys) => {
    if (keys.length > 2) response.context.slot--;
    return response;
  };
  await assert.rejects(rpc.fetchCase(address), { code: 'InconsistentObservation' });
});

for (const initiallyPresent of [false, true]) {
  test(`watchCase refreshes protocol state without a ${initiallyPresent ? 'closed' : 'present'} subject`, async () => {
    const { connection, rpc, address } = setup();
    if (!initiallyPresent) delete connection.current.accounts.subject;
    const updates = [];
    const errors = [];
    const close = await rpc.watchCase(address, {
      onUpdate: (view) => updates.push(view),
      onError: (error) => errors.push(error),
    });

    try {
      assert.equal(updates.length, 1);
      assert.equal(connection.listeners.size, 5);
      assert([...connection.listeners.values()].every(({ key }) => !key.equals(address.subject)));
      delete connection.current.accounts.subject;
      const requestsBeforeClosure = connection.requests.length;
      connection.emit(address.subject, 100);
      await tick();

      assert.equal(connection.requests.length, requestsBeforeClosure);
      connection.current = snapshot('audit-expired');
      delete connection.current.accounts.subject;
      connection.emit(SYSVAR_CLOCK_PUBKEY, 101);
      await tick();
      await tick();

      assert.deepEqual(
        updates.map((view) => view.observation.slot),
        [100n, 101n],
      );
      assert.equal(updates.at(-1).observation.unixTimestamp, 1300n);
      assert.deepEqual(errors, []);
      assert(
        connection.requests.every(({ keys }) => !keys.some((key) => key.equals(address.subject))),
      );
    } finally {
      await close();
    }

    assert.equal(connection.listeners.size, 0);
    assert.equal(connection.removed.length, 5);
  });
}

test('watchCase covers Clock and claim markers, discards stale in-flight snapshots and cleans every listener', async () => {
  const { connection, rpc, address } = setup();
  const updates = [];
  const errors = [];
  const close = await rpc.watchCase(address, {
    onUpdate: (view) => updates.push(view),
    onError: (error) => errors.push(error),
  });
  assert.equal(connection.listeners.size, 5);
  const keys = [...connection.listeners.values()].map((entry) => entry.key.toBase58());
  assert(keys.includes(connection.current.accounts.authorReceipt.key));
  assert(keys.includes(SYSVAR_CLOCK_PUBKEY.toBase58()));
  assert.equal(updates.length, 1);
  const held = deferred();
  let intercepted = false;
  connection.intercept = async (response, requested) => {
    if (!intercepted && requested.length > 2) {
      intercepted = true;
      await held.promise;
    }
    return response;
  };
  connection.emit(SYSVAR_CLOCK_PUBKEY, 100);
  await tick();
  assert(intercepted);
  connection.current = snapshot('audit-expired');
  assert.equal(connection.current.observation.slot, '101');
  connection.emit(SYSVAR_CLOCK_PUBKEY, 101);
  held.resolve();
  await tick();
  await tick();
  assert.deepEqual(
    updates.map((view) => view.observation.slot),
    [100n, 101n],
  );
  assert.equal(updates.at(-1).observation.unixTimestamp, 1300n);
  const calls = connection.requests.length;
  connection.emit(SYSVAR_CLOCK_PUBKEY, 99);
  await tick();
  assert.equal(connection.requests.length, calls);
  assert.deepEqual(errors, []);
  await close();
  await close();
  assert.equal(connection.listeners.size, 0);
  assert.equal(connection.removed.length, 5);
});

test('closing a subscription suppresses an in-flight callback; partial registration and cleanup failures release others', async () => {
  const { connection, rpc, address } = setup();
  const updates = [];
  const close = await rpc.watchPolicy(address.policy, {
    onUpdate: (view) => updates.push(view),
    onError: assert.fail,
  });
  const held = deferred();
  connection.intercept = async (response) => {
    await held.promise;
    return response;
  };
  connection.emit(SYSVAR_CLOCK_PUBKEY, 100);
  await tick();
  await close();
  held.resolve();
  await tick();
  assert.equal(updates.length, 1);
  assert.equal(connection.listeners.size, 0);
  const partial = setup();
  partial.connection.failRegistration = 2;
  await assert.rejects(
    partial.rpc.watchPolicy(address.policy, { onUpdate: assert.fail, onError: assert.fail }),
    /registration failed/,
  );
  assert.deepEqual(partial.connection.removed, [1]);
  const cleanup = setup();
  const stop = await cleanup.rpc.watchPolicy(address.policy, {
    onUpdate: () => {},
    onError: assert.fail,
  });
  cleanup.connection.failCleanup = 1;
  await assert.rejects(stop(), AggregateError);
  assert.deepEqual(cleanup.connection.removed, [1, 2]);
});

test('subscription read failures are delivered without publishing invalid views', async () => {
  const { connection, rpc, address } = setup();
  const updates = [],
    errors = [];
  const close = await rpc.watchPolicy(address.policy, {
    onUpdate: (view) => updates.push(view),
    onError: (error) => errors.push(error),
  });
  connection.intercept = () => {
    throw new Error('endpoint unavailable');
  };
  connection.emit(SYSVAR_CLOCK_PUBKEY, 100);
  await tick();
  assert.equal(updates.length, 1);
  assert.match(errors[0].message, /endpoint unavailable/);
  await close();
});

test('sign and send snapshots unsigned input, collects caller signatures and confirms the exact lifetime', async () => {
  const { connection, rpc } = setup();
  const payer = Keypair.generate(),
    owner = Keypair.generate();
  const unsigned = unsignedTransfer(payer, owner);
  const result = await rpc.sendAndConfirm(unsigned, {
    signers: [owner],
    wallet: {
      async signTransaction(transaction) {
        transaction.partialSign(payer);
        return transaction;
      },
    },
  });
  assert.equal(unsigned.recentBlockhash, undefined);
  assert.deepEqual(unsigned.signatures, []);
  assert.equal(result.slot, 345n);
  assert.equal(connection.sent.length, 1);
  const sent = Transaction.from(connection.sent[0].bytes);
  assert(sent.verifySignatures());
  assert.equal(sent.recentBlockhash, connection.lifetime.blockhash);
  assert.equal(connection.sent[0].options.maxRetries, 0);
  assert.deepEqual(connection.confirmed, [
    { strategy: { signature: result.signature, ...connection.lifetime }, commitment: 'confirmed' },
  ]);
});

test('wallet mutation, missing signer and pre-signed input never send', async () => {
  const { connection, rpc } = setup();
  const signer = Keypair.generate();
  await assert.rejects(
    rpc.sendAndConfirm(unsignedTransfer(signer), {
      wallet: {
        async signTransaction(transaction) {
          transaction.instructions[0].data[4] ^= 1;
          transaction.sign(signer);
          return transaction;
        },
      },
    }),
    /Wallet changed/,
  );
  const other = Keypair.generate();
  await assert.rejects(
    rpc.sendAndConfirm(unsignedTransfer(signer, other), { signers: [signer] }),
    /Signature verification failed/,
  );
  await assert.rejects(
    rpc.sendAndConfirm(signedTransfer(connection, signer).transaction, { signers: [signer] }),
    /previously signed/,
  );
  assert.equal(connection.sent.length, 0);
});

test('sending a signed legacy transaction preserves its exact bytes and rejects incorrect lifetime', async () => {
  const { connection, rpc } = setup();
  const { transaction } = signedTransfer(connection);
  const original = transaction.serialize();
  await assert.rejects(
    rpc.sendSignedAndConfirm(transaction, {
      ...connection.lifetime,
      blockhash: PublicKey.default.toBase58(),
    }),
    /lifetime does not match/,
  );
  await rpc.sendSignedAndConfirm(transaction, connection.lifetime, { maxRetries: 1 });
  assert.deepEqual(connection.sent[0].bytes, original);
  assert.equal(connection.sent[0].options.maxRetries, 1);
});

test('versioned transaction signatures are verified before submission', async () => {
  const { connection, rpc } = setup();
  const signer = Keypair.generate();
  const instruction = unsignedTransfer(signer).instructions[0];
  const message = new TransactionMessage({
    payerKey: signer.publicKey,
    recentBlockhash: connection.lifetime.blockhash,
    instructions: [instruction],
  }).compileToV0Message();
  const transaction = new VersionedTransaction(message);
  await assert.rejects(
    rpc.sendSignedAndConfirm(transaction, connection.lifetime),
    /Invalid transaction signature/,
  );
  transaction.sign([signer]);
  const signed = transaction.serialize();
  transaction.message.compiledInstructions[0].data[4] ^= 1;
  await assert.rejects(
    rpc.sendSignedAndConfirm(transaction, connection.lifetime),
    /Invalid transaction signature/,
  );
  await rpc.sendSignedAndConfirm(VersionedTransaction.deserialize(signed), connection.lifetime);
  assert.equal(connection.sent.length, 1);
  assert.deepEqual(connection.sent[0].bytes, Buffer.from(signed));
});

test('send, RPC signature and confirmation errors remain observable and are never retried by the SDK', async () => {
  for (const failure of ['sendError', 'wrongSignature', 'confirmationError']) {
    const { connection, rpc } = setup();
    connection[failure] =
      failure === 'sendError'
        ? new Error('preflight failed')
        : failure === 'wrongSignature'
          ? 'wrong'
          : { InstructionError: [0, { Custom: 6000 }] };
    await assert.rejects(
      rpc.sendSignedAndConfirm(signedTransfer(connection).transaction, connection.lifetime),
    );
    assert.equal(connection.sent.length, 1);
    assert.equal(connection.confirmed.length, failure === 'confirmationError' ? 1 : 0);
  }
});

test('simulation preserves signed messages and exposes program error and logs without broadcasting', async () => {
  const { connection, rpc } = setup();
  const { transaction } = signedTransfer(connection);
  const before = transaction.serialize();
  const result = await rpc.simulate(transaction, { sigVerify: true });
  assert.deepEqual(result.value.err, { InstructionError: [0, { Custom: 6000 }] });
  assert.deepEqual(result.value.logs, ['program rejected']);
  assert.deepEqual(Buffer.from(connection.simulations[0].transaction.serialize()), before);
  assert.equal(connection.simulations[0].config.sigVerify, true);
  assert.equal(connection.sent.length, 0);
  assert.deepEqual(transaction.serialize(), before);
});

test('confirmation snapshots the signed lifetime before an asynchronous send', async () => {
  const { connection, rpc } = setup();
  const { transaction } = signedTransfer(connection);
  const original = { ...connection.lifetime };
  const callerLifetime = { ...original };
  const held = deferred();
  const send = connection.sendRawTransaction.bind(connection);
  connection.sendRawTransaction = async (...args) => {
    await held.promise;
    return send(...args);
  };
  const pending = rpc.sendSignedAndConfirm(transaction, callerLifetime);
  callerLifetime.blockhash = PublicKey.default.toBase58();
  callerLifetime.lastValidBlockHeight = 0;
  held.resolve();
  const result = await pending;
  assert.equal(connection.confirmed[0].strategy.blockhash, original.blockhash);
  assert.equal(
    connection.confirmed[0].strategy.lastValidBlockHeight,
    original.lastValidBlockHeight,
  );
  assert.equal(result.blockhash, original.blockhash);
  assert.equal(result.lastValidBlockHeight, original.lastValidBlockHeight);
});

test('unsigned simulation supplies signature placeholders without changing or signing the input', async () => {
  const { connection, rpc } = setup();
  const signer = Keypair.generate();
  const transaction = unsignedTransfer(signer);
  transaction.recentBlockhash = connection.lifetime.blockhash;
  await rpc.simulate(transaction, { sigVerify: false });
  assert.equal(connection.simulations[0].transaction.signatures.length, 1);
  assert.deepEqual(connection.simulations[0].transaction.signatures[0], Buffer.alloc(64));
  assert.deepEqual(transaction.signatures, []);
  assert.equal(connection.sent.length, 0);
});
