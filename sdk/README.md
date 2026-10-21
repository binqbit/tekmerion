# Tekmerion SDKs

Integrate the contract from a backend or another Solana program.

Start with [how Tekmerion works](../docs/how-it-works.md) for a visual explanation
of funding, voting and payments. The [program reference](../programs/tekmerion/README.md)
describes accounts and instructions.

| Integration | Package | Capabilities |
|---|---|---|
| Rust backend | [rust](rust/README.md), `tekmerion-sdk` with `rpc` | Instructions, validated state reads, quotes, monitoring, signing, simulation and confirmation |
| TypeScript / Node.js backend | [typescript](typescript/README.md), `@tekmerion/sdk` | The same contract operations, typed decoders, RPC reads and subscriptions, simulation and transaction submission |
| Solana program | [rust](rust/README.md#anchor-cpi), defaults disabled with `cpi` | Original Anchor CPI functions and contexts, PDA signing and remaining accounts |

Both SDKs cover all nine contract instructions: create/update/acquire a policy,
create a case, contribute, submit an audit, finalize an unchallenged or expired
case, and claim author/contributor funds. They derive protocol addresses and
quote contributions, claims and the auditor destinations required by a deciding
vote. Amounts use integer base units: Rust integers or TypeScript `bigint`.

Supply an RPC connection and your own signers. State reads validate account
ownership, encoding, canonical addresses, lifecycle and backing. Monitoring
returns current snapshots; it is not an event archive. The SDKs do not run a
server, store keys or persist application data.

Choose `mainnet` or `devnet` consistently with the contract's mint profile. A
profile selects public identities; it does not deploy the contract or verify
that an RPC endpoint serves that network.

## Case modes

Creation selects Challenge (bond and challenge window) or DirectAudit (only the
author-paid Policy fee, immediate audit). The immutable mode is exposed by both
SDKs separately from lifecycle phase. Direct audits use the same Case, vault,
author receipt, auditor votes and claim instruction. The author can recover the
fee after TechnicalFailure; Yes, No and Unknown pay it to the deciding quorum.
Existing Cases cannot switch modes.

## Concurrent authors

Case addresses bind Policy, subject, author and a nonce that starts at zero for
each author. A successor requires that same author’s immediately preceding Case
to have completed with audited No/Unknown or technical failure. Different
authors can proceed concurrently, each with separate custody and claims. The
application owns final-result selection; Tekmerion never cancels a peer Case
because another one returned Yes.

## First Case with a Policy

A Policy's admin can change its rules while `active_uses == 0`. A successful
`acquire_policy` or `create_case` locks those rules permanently. Reading a Policy
or signing a transaction does not lock it: a creation signature binds the Policy
address, not the rules previously returned by RPC.

Before asking an author to approve the first deposit:

1. Read the Policy and check its configured client authority. If it is unused,
   have that authority submit `acquire_policy` and wait for confirmation at the
   application's chosen commitment.
2. Fetch the Policy again and require `active_uses > 0`. In TypeScript, pass the
   acquisition's confirmed slot as `minimumSlot`. If the read does not observe
   the lock, stop and refetch; do not proceed with the earlier rules.
3. Present these locked rules to the author: bond, challenge target, committee,
   threshold, fee and deadlines. Build and sign `create_case` only after approval.

An already locked Policy needs no extra acquisition. Keep the confirmed Policy
address for creation; a different Policy requires its own review. Acquisition
does not deposit tokens and does not check that its rules match an earlier read.
Putting acquisition and creation in one transaction cannot protect approval of
rules read before that transaction: an admin update can execute first.

The [TypeScript backend example](typescript/examples/backend.ts) implements
`lockAndReadPolicy`: it returns the locked rules for application review and does
not create a Case automatically. Rust callers use `fetch_policy`, the
`acquire_policy` instruction builder, `sign_transaction` and `send_and_confirm`
in the same order. A CPI caller can also check its expected rules in its own
atomic flow before authorizing creation. RPC observations still depend on the
chosen provider and commitment.

## Install from a checkout

For a Rust backend:

```toml
[dependencies]
tekmerion-sdk = { path = "../tekmerion/sdk/rust", features = ["rpc"] }
```

For an onchain program:

```toml
[dependencies]
tekmerion-sdk = { path = "../tekmerion/sdk/rust", default-features = false, features = ["mainnet", "cpi"] }
```

For TypeScript, build and pack a standalone package from this checkout:

```sh
npm ci --prefix sdk/typescript
npm pack ./sdk/typescript
# From the consuming application, install the generated archive:
npm install /path/to/tekmerion-sdk-0.1.0.tgz
```

These packages are maintained in this repository and are not published to a
registry. Rust depends on the adjacent program crate, so keep the checkout
intact or use a Git dependency pinned to the intended repository revision.

## Checks

```sh
bash scripts/test.sh
```

Run this command from the repository root. It prepares missing build tools and
locked dependencies, reuses local caches, and runs both SDK suites together with
the program and contract tests. No RPC endpoint or wallet is required.

The Rust checks execute both Tekmerion and a small CPI consumer in the local
Solana runtime. TypeScript tests compare instruction bytes, account metas,
decoded state and payout quotes against [fixtures](fixtures/README.md)
generated by that runtime. Both public identity profiles are checked as offline
byte vectors. Mock RPC tests exercise consistent reads, updates,
signatures and confirmation failures without sending live transactions.
