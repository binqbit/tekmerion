# TypeScript SDK

Node.js SDK for building Tekmerion instructions, reading and watching on-chain state, quoting contributions and payouts, and executing caller-authorized transactions. It has no database, server, key storage or submission journal.

Use Node.js 20.18 or newer and `@solana/web3.js` version `1.99.0` (the legacy v1 API). The SDK declares this exact version as a peer dependency so the application and SDK share compatible `PublicKey` and transaction classes; resolve peer dependency conflicts instead of forcing a second version. The pinned `@anchor-lang/core` coder supplies instruction and account serialization from the maintained deployment IDLs. All protocol amounts, nonces, slots and timestamps use native `bigint`; unsafe JavaScript number conversion is rejected.

## Install

This package is maintained in this repository and is not published to a registry. Build it before installing its directory:

```sh
cd /path/to/tekmerion/sdk/typescript
npm ci
npm run build
cd /path/to/backend
npm install --install-links --save-exact @solana/web3.js@1.99.0 /path/to/tekmerion/sdk/typescript
```

A backend can also declare `"@tekmerion/sdk": "file:../tekmerion/sdk/typescript"` alongside `"@solana/web3.js": "1.99.0"`, with `install-links=true` in its project `.npmrc`. The [npm `install-links` option](https://docs.npmjs.com/cli/v11/commands/npm-install/#install-links) installs a packed directory rather than a symlink, so the SDK does not resolve dependencies from its development checkout. For a standalone artifact, run `npm pack` in the SDK directory and install the resulting tarball. The `prepack` script builds the packaged `dist` files, declarations and both network IDLs. Consumer imports use `@tekmerion/sdk`; no source files or repository-relative IDL reads are needed at runtime.

## Unsigned instructions

`createClient('mainnet')` or `createClient('devnet')` selects the deployment's USDC mint. The program ID is the same in both profiles. Select the network explicitly in an application; the RPC endpoint must match it.

```ts
import { createClient, type CaseAddress } from '@tekmerion/sdk';
import { PublicKey, type TransactionInstruction } from '@solana/web3.js';

export function contributionInstruction(
  identity: CaseAddress,
  payer: PublicKey,
  owner: PublicKey,
  source: PublicKey,
  amount: bigint,
): TransactionInstruction {
  return createClient('devnet').contribute(identity, { payer, owner, source }, amount);
}
```

`CaseAddress` is `{ policy: PublicKey, subject: PublicKey, nonce: bigint }`. The builders derive canonical Case, vault, policy, stake receipt and predecessor PDAs. They return unsigned `TransactionInstruction` values; the program enforces authorization and current state.

| Builder                                                           | Inputs beyond the Case identity                                                           |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `createPolicy(input)`                                             | `payer`, `admin`, `clientAuthority`, `identifier`, `rules`                                |
| `updatePolicy(admin, identifier, rules)`                          | Policy admin and identifier                                                               |
| `acquirePolicy(policy, clientAuthority, subject)`                 | Policy, authority and opaque subject                                                      |
| `createCase(identity, input)`                                     | `payer`, `clientAuthority`, `author`, `source`, `authorPrincipal`, 32-byte `assertion`    |
| `contribute(identity, input, requested)`                          | `payer`, `owner`, `source`, requested base units                                          |
| `submitAudit(identity, auditor, answer, reason, feeDestinations)` | Member signer, decision, ordered SPL token accounts                                       |
| `finalizeUnchallenged(identity)`                                  | No extra input                                                                            |
| `finalizeTimeout(identity)`                                       | No extra input                                                                            |
| `claim(identity, author, destination)`                            | Case author and `{ owner, destination }`; the author receipt is aliased for author claims |

Answers are `yes`, `no` and `unknown`. Yes/No require `decisive`; Unknown uses `missingEvidence`, `staleEvidence`, `conflictingEvidence`, `unsupportedEvidence` or `inconclusive`.

Before the first deposit, follow the [Policy locking sequence](../README.md#first-case-with-a-policy): confirm `acquirePolicy`, refetch at or above its confirmed slot, require `activeUses > 0n`, then ask the author to review the locked rules before signing `createCase`. The [backend example](examples/backend.ts) implements `lockAndReadPolicy` for this preparation. An already locked Policy needs no extra acquisition. A valid Policy read with `activeUses === 0n` is still admin-editable; acquisition and creation in one transaction do not pin rules read earlier.

## Reads and quotes

`TekmerionRpc` accepts a caller-owned web3.js `Connection`, network and commitment (default `confirmed`). It provides:

- `fetchPolicy(address, minimumSlot?)`: read a canonical policy, including one that has not yet been used.
- `fetchCase(identity, minimumSlot?)`: authenticate the Case, policy, vault and author receipt.
- `fetchContributorClaim(identity, owner, minimumSlot?)`: return `{ view, claim }` with the contributor receipt from the same final batch.
- `watchPolicy(address, { onUpdate, onError })` and `watchCase(identity, callbacks)`: return an asynchronous unsubscribe function.

Reads use `getMultipleAccountsInfoAndContext` with the Clock sysvar. Discovering a Case's author requires a preliminary read; the final batch rereads the Case and all required protocol accounts together, at or above that slot. A mismatched Clock slot, missing required account, owner, discriminator, size, canonical PDA, role, lifecycle or vault backing fails validation. `readPolicy` and `readCase` on the unsigned client perform the same validation on caller-supplied `AccountData` and `Observation`. The RPC provider remains the source of account data and commitment; these checks are not cryptographic proofs of the chain.

The canonical Case authenticates its stored subject address. RPC reads and watchers neither fetch nor subscribe to the opaque subject account, so an absent or closed subject does not prevent reading Case state or claims. Applications own subject content and lifecycle checks. For direct `readCase` calls, `CaseAccounts.subject` is optional; an explicitly supplied subject must match the expected address and observation slot and must be nonexecutable. All protocol accounts remain required.

A `CaseView` exposes copied `case`, `policy`, `authorReceipt`, `address` and `observation` snapshots, `vaultBalance`, `completionMethod`, and:

- `quoteContribution(requested)` returns the capped amount or throws `ActionUnavailable`.
- `quoteAudit(auditor, answer, reason)` returns an empty array below quorum, or ordered `{ auditorIndex, feeRecipient, amount }` payouts at quorum.
- `authorClaim()` and `contributorClaim(owner, account)` return `{ kind: 'unavailable' }`, `{ kind: 'claimed' }` or `{ kind: 'unclaimed', amount }`. A zero-value unclaimed result still requires the claim instruction to mark the receipt.
- `auditFeesPaid()` returns the total paid fee for audited completion, otherwise `null`.

Fee recipients are wallet authorities, not token account addresses. Resolve each recipient to an initialized classic SPL USDC account owned by that wallet and preserve the quote order when building `submitAudit`. The SDK does not assume associated token accounts. Quorum quotes include destinations even when their individual payout is zero.

Quotes describe one observed slot. State can change before execution, particularly when another auditor makes an earlier vote decisive. Refresh and rebuild when appropriate; the contract rechecks quorum, time, accounts and balances atomically.

Watchers subscribe to the relevant accounts and Clock so deadlines and claim markers can trigger a refreshed view. Notifications cause fresh validated reads; they never become partial state updates. Older slots and in-flight results overtaken by newer notifications are suppressed. This reports current snapshots, not every historical transition. Handle `onError`, use a suitable commitment, and `await unsubscribe()` when finished; closing suppresses pending callbacks and attempts removal of every listener. Callbacks should not throw.

## Simulation and execution

`simulate(transaction, config?)` preserves the message and returns the RPC result, including any `value.err` and logs. Supply a recent blockhash yourself; `sigVerify` and other web3.js simulation options remain caller-controlled. Simulation does not send a transaction or guarantee later execution.

`sendAndConfirm(unsignedLegacyTransaction, { signers?, wallet?, sendOptions? })` obtains a fresh blockhash, snapshots the instructions and fee payer, gathers the caller's signatures, verifies that a wallet has not changed the message, sends and confirms. Set `transaction.feePayer` first. A wallet implements `signTransaction(transaction): Promise<Transaction>`; local web3.js `Signer` objects can supply other required signatures. The input transaction is left unchanged.

`sendSignedAndConfirm(legacyOrVersionedTransaction, { blockhash, lastValidBlockHeight }, sendOptions?)` preserves signed bytes and verifies all required signatures before sending. The supplied lifetime must belong to the signed recent blockhash. It supports legacy and v0 transactions; use a recent-blockhash lifetime, not a durable nonce. Both send methods check the returned transaction signature and confirmed execution error. They default to `maxRetries: 0`; callers can explicitly select an RPC retry setting. There is no SDK retry, resigning, persistence or transaction reconstruction.

A successful result contains `{ signature, blockhash, lastValidBlockHeight, slot }`. RPC transport errors, expiration and execution failures propagate. A send or confirmation error can leave the transaction outcome uncertain; use the original signature to inspect the chain before authorizing another transaction.

See the [typechecked backend example](examples/backend.ts) for Policy locking, contribution, quorum voting and monitoring with caller-owned connections and signers. Importing the example performs no I/O. When serializing results for JSON, convert `bigint` values to decimal strings explicitly.

## Validation

From the repository root, run the program, local contract and both SDK suites:

```sh
bash scripts/test.sh
```

The runner prepares missing tools and locked dependencies and reuses local caches. It also checks TypeScript formatting, builds the package and typechecks the backend example. No RPC endpoint or wallet is required.

Tests compare all nine instruction builders, including u64 boundaries and receipt aliases, with independent Rust-generated bytes and account metas in [SDK fixtures](../fixtures/README.md). Both network profiles cover 22 actual bank snapshots, all five completion outcomes, contribution and audit quotes, and claims before and after settlement. Additional tests reject synthetic malformed state and exercise RPC batching, subscription cleanup, stale observations, transaction signatures, wallet changes and RPC failures without spending live funds. The runner regenerates the default contract identity's fixtures locally and compares them to detect drift; both SDK identity profiles are checked as offline byte vectors.
