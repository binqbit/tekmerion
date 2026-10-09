# TypeScript SDK

Node.js SDK for building Tekmerion instructions, validating account observations, and quoting contributions and payouts. It has no database, server, key storage or submission journal.

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

Before the first deposit, follow the Policy locking sequence: confirm `acquirePolicy`, refetch at or above its confirmed slot, require `activeUses > 0n`, then ask the author to review the locked rules before signing `createCase`. An already locked Policy needs no extra acquisition. A valid Policy read with `activeUses === 0n` is still admin-editable; acquisition and creation in one transaction do not pin rules read earlier.

## Reads and quotes

`readPolicy` and `readCase` on the unsigned client validate caller-supplied `AccountData` and `Observation`. A mismatched observation slot, missing required account, owner, discriminator, size, canonical PDA, role, lifecycle or vault backing fails validation. The caller supplies account data from one consistent observation.

The canonical Case authenticates its stored subject address. `CaseAccounts.subject` is optional; an explicitly supplied subject must match the expected address and observation slot and must be nonexecutable. All protocol accounts remain required. Applications own subject content and lifecycle checks.

A `CaseView` exposes copied `case`, `policy`, `authorReceipt`, `address` and `observation` snapshots, `vaultBalance`, `completionMethod`, and:

- `quoteContribution(requested)` returns the capped amount or throws `ActionUnavailable`.
- `quoteAudit(auditor, answer, reason)` returns an empty array below quorum, or ordered `{ auditorIndex, feeRecipient, amount }` payouts at quorum.
- `authorClaim()` and `contributorClaim(owner, account)` return `{ kind: 'unavailable' }`, `{ kind: 'claimed' }` or `{ kind: 'unclaimed', amount }`. A zero-value unclaimed result still requires the claim instruction to mark the receipt.
- `auditFeesPaid()` returns the total paid fee for audited completion, otherwise `null`.

Fee recipients are wallet authorities, not token account addresses. Resolve each recipient to an initialized classic SPL USDC account owned by that wallet and preserve the quote order when building `submitAudit`. The SDK does not assume associated token accounts. Quorum quotes include destinations even when their individual payout is zero.

Quotes describe one observed slot. State can change before execution, particularly when another auditor makes an earlier vote decisive. Refresh and rebuild when appropriate; the contract rechecks quorum, time, accounts and balances atomically.

## Validation

From the repository root:

```sh
npm ci --prefix sdk/typescript
npm test --prefix sdk/typescript
```

No RPC endpoint or wallet is required.

Tests compare all nine instruction builders, including u64 boundaries and receipt aliases, with independent Rust-generated bytes and account metas in [SDK fixtures](../fixtures/README.md). Both network profiles cover 22 actual bank snapshots, all five completion outcomes, contribution and audit quotes, and claims before and after settlement. Additional tests reject synthetic malformed state. Both SDK identity profiles are checked as offline byte vectors.
