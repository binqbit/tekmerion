# Tekmerion Anchor program

Tekmerion provides bonded assertions, capped challenge funding and settlement
by independent auditors. A Policy selects a committee, a strict-majority
threshold and a configured fee wallet per auditor. When a funded Case receives
enough matching answers, the final vote settles the Case and pays the winning
auditors automatically.

Program ID: `TEKxbA5q3EhSDSBeYiW9AT3GGPyzruBSQUcuQXxdy3s`.

## Build and check

Build the production SBF program from the repository root:

```sh
bash scripts/build.sh
```

The command selects the mainnet USDC profile and writes `tekmerion.so`, the
generated `tekmerion.json`, build metadata and checksums to
`target/production/release/`. It prepares the pinned tools and builds only the
program and its interface. The [build guide](../../docs/build.md) describes the
requirements and output bundle. Building performs no deployment.

Before deployment, run the complete local checks separately:

```sh
bash scripts/test.sh
```

The runner prepares missing tools and dependencies, builds Tekmerion and runs
program, local contract and SDK checks. Execution uses a local Solana bank;
no RPC endpoint or wallet is required. The
[test guide](../../tests/README.md) describes the local Solana context
and setup modules. The [IDL](../../idl/tekmerion.json) records instruction arguments,
account ordering, state fields and errors; devnet snapshots are in
`idl/devnet/`.

Exactly one Cargo feature must be selected: `mainnet` (default) or `devnet`.
For devnet use `--no-default-features --features devnet` consistently for
programs and clients. [`src/network.rs`](src/network.rs) owns the USDC mint
selection. Mainnet uses `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v` and
devnet uses `4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU`. Both use six
decimals and the classic Token program. Selecting a profile performs no live
deployment or transfer.

## Architecture

The [visual mechanism guide](../../docs/how-it-works.md) explains funding,
auditor decisions, payments and the checks the contract enforces. This section
maps those responsibilities to the source code.

| Boundary                                              | Responsibility                                                                         |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------- |
| [lib.rs](src/lib.rs)                                  | Program identity and thin Anchor dispatch                                              |
| [instructions/policy](src/instructions/policy/mod.rs) | Policy authorization, creation and updates                                             |
| [instructions/case](src/instructions/case/mod.rs)     | Bonded creation, contributions, signed audit votes and finalization                    |
| [instructions/claim.rs](src/instructions/claim.rs)    | Shared author and contributor payout instruction                                       |
| [state](src/state/mod.rs)                             | Account layouts, PDA helpers, committee rules and lifecycle transitions                |
| [state/audit.rs](src/state/audit.rs)                  | Decisions, immutable auditor votes and quorum lookup                                   |
| [custody.rs](src/custody.rs)                          | Token validation, backing checks, exact transfers and claims                           |
| [calculations](src/calculations/mod.rs)               | Pure checked amounts, contribution quotes and economic allocations shared with the SDK |

All refund and reward calculations live together in
[`calculations/settlement.rs`](src/calculations/settlement.rs). Funding caps,
deadlines and checked amount types are adjacent modules under
`tekmerion::calculations`. The SDK uses these same functions through its program
dependency. The program retains inline unit tests for committee invariants,
completion classification and account layouts. Transaction-level settlement
scenarios live in the [local test workspace](../../tests/README.md).

Each instruction module exposes a crate-private `handle` function; the Anchor
entrypoint dispatches through `instructions::create_case::handle(...)`, for
example. Account contexts are reexported explicitly so generated client types
remain available at their established module paths.

Instruction contexts group the payer and role signers first, then protocol and
subject accounts in validation order, then the mint and token accounts in
transfer order, and finally program accounts. Serialized state groups identity,
configuration or funding, lifecycle, settlement rights and payment progress;
PDA bumps come last. Existing `PolicyRules` and `AuditContext` types keep related
data together without adding nested account contexts.

Handlers authenticate account identities and signers. State methods enforce
lifecycle rules, calculation functions determine entitlements, and custody verifies
actual token debit and credit. Policy rules and economic calculations are
shared with SDK account views. Applications interpret subject data and finalized
results.

Audit decisions and voting progress belong to `state/audit.rs`; Case methods
own voting windows and settlement transitions. `PolicyRules` validates its
parameters and deadlines. The creation instruction authenticates its preceding
Case locally. Token custody stays in one module, while calculations,
instruction builders and account views keep their own boundaries. The
[SDKs](../../sdk/README.md) provide optional RPC reads, monitoring, simulation and
transaction submission. Integrating applications supply their endpoint and
signers and decide when to submit or retry an operation.

Validation helpers use `require_*` names. Handlers keep authorization, amount
calculation, token movement and state updates visible in execution order.
Intermediate amounts have names that explain their role; checked arithmetic
and post-transfer balance checks remain explicit.

## Instructions

| Instruction                                          | Authorization and effect                                                                                       |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `create_policy(identifier, client_authority, rules)` | Admin and payer signatures; creates validated committee and economics                                          |
| `update_policy(rules)`                               | Stored admin; only while `active_uses == 0`                                                                    |
| `acquire_policy()`                                   | Stored client authority; retains an application dependency and permanently locks the rules                     |
| `create_case(nonce, author_principal, agreement_hash, result_hash)`    | Client authority and author; transfers the bond and creates Case, vault and author receipt                     |
| `contribute(requested)`                              | Contributor signature and owned token source; caps accepted principal at the remaining target                  |
| `submit_audit(answer, reason)`                       | Configured auditor signature; records its single vote and automatically settles/pays upon quorum               |
| `finalize_unchallenged()`                            | Anyone after the funding deadline while below target; optimistic Yes with full refunds                         |
| `finalize_timeout()`                                 | Anyone at/after the audit deadline without quorum; refunds principal without an answer or fee                  |
| `claim()`                                            | Receipt owner and owned USDC destination; validates the participant's role and discharges its entitlement once |

Policy admission accepts 1–9 unique auditor authorities and unique fee wallets,
a strict-majority threshold, positive minimum bond/target/durations, and a fee
not exceeding either bond minimum or target. Cases retain the Policy, making
those rules immutable. The full actual author principal is at risk, including
any excess over the minimum. A funded challenge fixes its audit deadline once.

An unused Policy remains admin-editable until acquisition or creation executes.
A creation signature commits to its address, not a previously read rule version.
For approval of fixed terms, confirm acquisition, reread the locked Policy, then
review and sign creation. Acquisition and creation in the same transaction do
not pin rules read before that transaction. See the
[first-Case integration sequence](../../sdk/README.md#first-case-with-a-policy).

The configured `client_authority` can be a wallet or a PDA signing through CPI.
Tekmerion binds the opaque, nonexecutable subject address, author and two nonzero 32-byte commitments: `agreement_hash` for the agreed rules and `result_hash` for the proposed result. The client defines the documents and their canonical encoding; both hashes remain immutable for the Case.
It does not infer an external program identity or interpret subject data.
The subject need not have a funded account. Closing an external subject account
later does not invalidate the Case or prevent settlement and withdrawals.
Application code owns subject authorization and transitions. This authority and
the auditor authorities are independent roles configured explicitly in Policy.

Each author has an independent Case sequence for a Policy and subject. Its initial
Case has nonce zero and no remaining creation accounts. A later attempt supplies
exactly its canonical preceding Case from the same author, which must have ended
with audited No/Unknown or technical failure. Yes does not permit a successor.
Each attempt has a fresh bond and vault; preceding claims stay independent.
Applications may enforce their own retry rules. Tests call Tekmerion directly
with wallet signers and read results from the Case account.

## State and custody

| Account      | PDA seeds                                           | Reserved bytes including discriminator |
| ------------ | --------------------------------------------------- | -------------------------------------: |
| Policy       | `policy`, admin, identifier as little-endian u64    | 710                                    |
| Case         | `case`, Policy, subject, author, nonce as little-endian u64 | 355                                    |
| StakeReceipt | `receipt`, Case, owner                              | 83                                     |
| Vault        | `vault`, Case                                       | Classic SPL Token account              |

The Case is the vault's token authority. Policy and Case allocations reserve
space for the maximum committee and votes. The protocol uses `u64` token
amounts and widened `u128` liabilities; Clock timestamps are `i64`.
`CasePhase` tags are Funding=0, Funded=1 and Finalized=2. The quorum-reaching
vote moves a funded Case directly to Finalized in the payment transaction.
`result` records Yes/No/Unknown, or no answer after a timeout. The
`completion_method()` helper derives optimistic, audited or technical-failure
completion from the lifecycle, result, audit response and funding state; no
separate completion-method field is stored. `finalized_at` records the terminal
chain time and remains unchanged by later claims.

Each vote retains the auditor index and an `AuditDecision` containing the answer
and reason. The terminal audit response records the decision completing quorum.
Auditors review the funded Case and its immutable Policy before signing a
transaction for that Case. The signed message binds the program, accounts,
answer, reason and payout destinations. Yes and No use `Decisive`; Unknown uses
one of the evidence-related reasons. Only the answer counts toward quorum.

Every payment checks the pinned mint and initialized, unfrozen token accounts,
then uses `transfer_checked` and verifies the exact change after reloading
source and destination. The vault cannot have a delegate or close authority.
Threshold settlement validates all winning recipients through atomic transfers;
any failure restores votes, state and balances to their prior values.
Author/contributor claims check outstanding backing and record discharge in the
same transaction. Zero entitlements can also be discharged once.

Authors and challengers use the same `StakeReceipt` account type, with
`side = 0` for the author and `side = 1` for a challenger. Creation records the
author's actual bond; challenge receipts aggregate accepted contributions and
top-ups. Each receipt stores its own `claimed` marker. Both roles call `claim()`
with the same accounts in this order: owner signer, writable Case, readonly
author receipt, writable owner receipt, mint, writable vault, writable
destination and Token program. The signer must own the receipt and destination.
The instruction identifies the author by `owner == case.author` and validates
the corresponding receipt side and principal; callers do not supply a role.

The canonical author receipt supplies the payment marker for the vault's
remaining obligations through `Case::outstanding_liability(author_claimed)`.
The earlier `unpaid()` name remains available for Rust callers.
For an author claim, both receipt entries reference the
same PDA; only the writable owner receipt persists the claim marker. Author
claims leave contributor counters unchanged. Contributor claims update the
remaining pool and receipt count, assigning any rounding remainder to the last
claimant. The frozen Case supplies settlement amounts and the audit fee, so no
Policy account is needed for either role. The Case stores no separate
author-claimed marker. Zero entitlements also discharge their receipt once.

Only accepted principal creates rights; direct vault donations do not start
audits, increase rewards or change claims. No sweep, claim expiry or account
closure is provided.
Policy usage is retained without a release operation, including after claims
are paid. Account layouts are defined by the matching generated IDL. Rebuild
clients and recreate disposable local accounts when layouts change; existing
layouts are not migrated.
The direct vote instruction takes two enum arguments and `claim()` serves both
participant roles. Build clients with the matching IDL.
