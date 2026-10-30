import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import ts from 'typescript';

const root = fileURLToPath(new URL('..', import.meta.url));

test('packed core SDK supports an independent auditor without the adapter or source checkout', () => {
  const destination = mkdtempSync(join(tmpdir(), 'tekmerion-independent-'));
  try {
    const packed = JSON.parse(
      execFileSync(
        'npm',
        ['pack', '--ignore-scripts', '--json', '--pack-destination', destination],
        { cwd: root, encoding: 'utf8' },
      ),
    )[0];
    assert(packed.files.some((file) => file.path === 'dist/wire.js'));
    assert(!packed.files.some((file) => /chainlink|confidential|template/.test(file.path)));

    const installed = join(destination, 'node_modules/@tekmerion/sdk');
    mkdirSync(installed, { recursive: true });
    execFileSync('tar', [
      '-xf',
      join(destination, packed.filename),
      '-C',
      installed,
      '--strip-components=1',
    ]);

    // Reuse installed third-party packages offline. The SDK itself is the packed
    // artifact, with no link back to the source tree or the optional adapter.
    const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
    for (const name of new Set([
      ...Object.keys(manifest.dependencies),
      ...Object.keys(manifest.peerDependencies),
    ])) {
      const link = join(destination, 'node_modules', name);
      mkdirSync(dirname(link), { recursive: true });
      symlinkSync(realpathSync(join(root, 'node_modules', name)), link, 'dir');
    }
    writeFileSync(join(destination, 'package.json'), '{"private":true,"type":"module"}\n');
    const example = readFileSync(join(root, 'examples/independent-auditor.ts'), 'utf8');
    writeFileSync(
      join(destination, 'auditor.mjs'),
      ts.transpileModule(example, {
        compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2022 },
      }).outputText,
    );
    writeFileSync(
      join(destination, 'run.mjs'),
      `
      import assert from 'node:assert/strict';
      import { createClient } from '@tekmerion/sdk';
      import { CORE_ID, decodeCaseIdentity } from '@tekmerion/sdk/wire';
      import { PublicKey } from '@solana/web3.js';
      import { createDirectAudit } from './auditor.mjs';
      const key = (value) => new PublicKey(new Uint8Array(32).fill(value));
      const { identity, transaction } = createDirectAudit({
        network: 'devnet', policyIdentifier: 1n, payer: key(1), administrator: key(2),
        clientAuthority: key(3), author: key(4), authorTokenAccount: key(5), auditor: key(6),
        feeRecipient: key(7), subject: key(8), agreementHash: new Uint8Array(32).fill(9),
        resultHash: new Uint8Array(32).fill(10), auditFee: 7n, auditTimeout: 3600n,
      });
      assert.equal(transaction.instructions.length, 2);
      assert(transaction.instructions.every((ix) => ix.programId.toBase58() === CORE_ID));
      assert.equal(typeof decodeCaseIdentity, 'function');
      const client = createClient('devnet');
      const vote = client.submitAudit(identity, key(6), 'yes', 'decisive', [key(11)]);
      assert.equal(vote.programId.toBase58(), CORE_ID);
      assert(vote.keys.some((account) => account.isSigner && account.pubkey.equals(key(6))));
      assert.equal(client.finalizeTimeout(identity).programId.toBase58(), CORE_ID);
      assert.equal(client.claim(identity, key(4), { owner: key(4), destination: key(5) }).programId.toBase58(), CORE_ID);
      assert.throws(() => import.meta.resolve('@tekmerion/chainlink-sdk'), { code: 'ERR_MODULE_NOT_FOUND' });
      assert.throws(() => import.meta.resolve('@tekmerion/sdk/confidential'), { code: 'ERR_PACKAGE_PATH_NOT_EXPORTED' });
    `,
    );
    execFileSync(process.execPath, ['run.mjs'], { cwd: destination, encoding: 'utf8' });
  } finally {
    rmSync(destination, { recursive: true, force: true });
  }
});
