# Tekmerion Rust SDK

Instruction builders, validated account views, optional RPC integration and
Anchor CPI access for the selected network. The SDK reuses generated types and pure calculations from
[`tekmerion::calculations`](../../programs/tekmerion/src/calculations/mod.rs).

The defaults are `mainnet` and `host`. Mainnet selects the program's mainnet USDC
address. Host clients targeting devnet use
`--no-default-features --features devnet,host`.
Exactly one network must be selected across the program, SDK and their callers;
enabling both networks or neither fails compilation. The SDK forwards its network
selection to the program and uses `tekmerion::USDC_MINT` for instruction accounts
and custody validation. Network selection does not establish a live deployment
or authenticate an RPC connection. Local fixtures use the same selected public
addresses with disposable state.

| Feature | Surface |
|---|---|
| `mainnet` / `devnet` | Exactly one network, forwarded to the program |
| `host` | Account views, SDK errors and funding/payout quotes |
| `rpc` | Asynchronous state reads, monitoring, transaction signing, simulation and confirmation; enables `host` |
| `cpi` | Original Anchor-generated `cpi` functions and account contexts |
| `idl-build` | IDL generation forwarded to the original program |

Instruction builders and the `program` reexport are available without `host`.
The SDK declares Rust 1.89 compatibility for the SBF path; maintained host checks
use the root's Rust 1.97.1 toolchain. Onchain consumers disable default features
and select their network with `cpi`, keeping host account views out of their build.

## Modules

| Module | Responsibility |
|---|---|
| `instructions` | Policy administration/acquisition, Case creation, capped contributions, auditor votes, finalization and claims; canonical addresses and generated Anchor encoding |
| `accounts` (`host`) | Validated Case/Policy/vault observations with opaque subject identity, funding and current payout quotes, and authenticated claim markers |
| `client` (`rpc`) | `TekmerionClient` over a caller-configured Solana RPC client |
| `program` | Original Anchor program crate, including account types, generated instructions and program identity |
| `cpi` (`cpi`) | Original Anchor-generated CPI interface; no duplicate protocol wrapper |

`CaseAddress` contains the Policy, subject and nonce; its helpers derive the Case,
vault and owner receipt using the program's PDA methods. It grants no authority.
The Policy builder accepts the admin-selected `client_authority` public key;
assignment requires the admin's signature, not the authority's signature. Policy
stores no `client_program`, and Case authorization checks the stored authority's
address and signature. The subject is an opaque, nonexecutable account identity;
its owner, schema, PDA derivation and application lifecycle belong to the client.
Neither the protocol nor SDK identifies a client program from its authority.
See the [authority contract](../../programs/tekmerion/README.md#instructions).

`create_case(CaseAddress, CreateCase)` builds direct submission with explicit
payer, client authority, author, source, principal, agreement hash and result hash. For a nonzero
nonce it appends the canonical preceding Case as a readonly remaining account.
Creation also initializes the author's `StakeReceipt` at `case.receipt(&author)`;
the payer funds its rent along with the Case and vault.
`acquire_policy(policy, client_authority, subject)` builds the authority-signed
dependency increment. Builders neither sign nor submit, and do not implement
client application transitions.

## Independent auditor committee

`PolicyRules` configures `auditors: Vec<AuditorConfig>` and `threshold` alongside
the bond, challenge target, fee and deadlines. Each `AuditorConfig` has a signing
`authority` and a separate `fee_recipient` wallet. Committees have 1–9 members
and require a strict majority, including 1-of-1, 2-of-3 and 3-of-5. Authorities
and fee recipients must be nonzero and distinct within their respective lists.
The application `client_authority` authorizes Case creation; committee members
sign audit votes separately.

`submit_audit(case, auditor, answer, reason, fee_destinations)` builds an
authority-signed vote using `program::AuditAnswer` and `program::AuditReason`.
The signed instruction identifies the Case and auditor and carries the answer
and reason. Each member can vote once. Quorum counts matching answers; members
may give different reasons consistent with that answer.

Before quorum, pass an empty `fee_destinations` slice. The vote that reaches the
threshold must include the winning voters' USDC token accounts in their configured
auditor-index order. Each token account must belong to that member's configured
fee-recipient wallet. The contract atomically finalizes the answer and transfers
the full audit fee equally to those voters. Remaining base units go one each to
the earliest winning indices, so shares differ by at most one base unit.
There is no separate fee claim. A rejected destination or
failed transfer rolls back the entire threshold vote and settlement.

Use `view.quote_audit(auditor, answer, reason)` to obtain the winning auditor
indices, fee-recipient wallets and exact amounts in committee order. It returns
an empty list for a valid vote before quorum. Resolve those wallets to their
USDC token accounts and pass the accounts to `submit_audit` in the returned order.
Concurrent votes can change which account list is required: a stale transaction
fails without recording a vote or paying fees, and a revised message needs fresh
review and signing. SDK builders do not fetch accounts or sign transactions.

## Direct wallet integration

Use `create_policy` to configure a creation authority and the auditor committee.
Before the first deposit, follow the
[Policy locking sequence](../README.md#first-case-with-a-policy): confirm
`acquire_policy`, reread and require `active_uses > 0`, then have the author review
the locked rules. Build `create_case` with the configured `client_authority`
wallet and author, then sign the transaction with those wallets and its payer.
Putting acquisition and creation in one transaction does not preserve rules read
before signing. An already locked Policy does not need another acquisition.
Auditor wallets submit
`submit_audit` transactions directly. Read the result from the Case account;
no extra application contract or result-consumption transaction is required.

The [contract scenarios](../../tests/cases/) exercise wallet signing,
contribution, audit voting, result reads and claims.
The SDK's optional `cpi` feature also exposes Anchor's generated interface for
applications that choose to call Tekmerion from their own program.

## Backend RPC integration

Enable `rpc` for a backend; for devnet disable defaults and select `devnet,rpc`.
The caller supplies a Tokio runtime, RPC URL, commitment and signers:

```rust,ignore
use tekmerion_sdk::client::{CommitmentConfig, RpcClient, TekmerionClient};

let client = TekmerionClient::new(RpcClient::new_with_commitment(
    rpc_url,
    CommitmentConfig::confirmed(),
));
let view = client.fetch_case(case_address).await?;
let (view, claim) = client.fetch_claim(case_address, owner).await?;
let transaction = client.sign_transaction(&instructions, &payer, &signers).await?;
let simulation = client.simulate(&transaction).await?;
// Inspect simulation.value.err and logs before choosing to submit.
let signature = client.send_and_confirm(&transaction).await?;
```

`fetch_policy` validates a Policy independently of any Case, including before
its first use; a valid read with `active_uses == 0` is still admin-editable.
After confirming acquisition, refetch and require `active_uses > 0` before
reviewing rules and signing creation. A lagging RPC read must not silently fall
back to the earlier rules. Case and claim reads discover the immutable author first, then
fetch all required accounts and the Clock in one batch at an equal or later
context slot. The returned views use only that final batch.

`watch_case` and `watch_policy` poll validated snapshots at a caller-selected interval. Pass a
`tokio::sync::watch::Receiver<bool>` and set it to `true` to stop; dropping its
sender also cancels. Cancellation interrupts an in-flight read. RPC or validation
errors return to the caller. Polling does not promise every intermediate vote or
claim. Choose the commitment appropriate to how you will use the result.

Signing fetches a recent blockhash once. Simulation and submission preserve the
resulting message and signatures; submission does not create replacement
transactions. Retain the signed transaction to reconcile its signature if an
RPC call fails or confirmation is uncertain, before deciding to sign again.

## Anchor CPI

```toml
tekmerion-sdk = { path = "../tekmerion/sdk/rust", default-features = false, features = ["mainnet", "cpi"] }
```

Use `tekmerion_sdk::cpi` and its generated account contexts directly. A calling
program can authorize creation or an auditor vote with its PDA through
`CpiContext::new_with_signer`. Attach winning fee token accounts with
`with_remaining_accounts`, preserving the required committee order. All nine
instructions are available through the original generated interface.

The [minimal consumer](../../tests/programs/sdk_consumer/README.md)
demonstrates PDA-authorized creation and audit votes, including predecessor and
fee remaining accounts. Its [runtime tests](../../tests/cases/cpi.rs)
execute actual cross-program calls. This build excludes account views and RPC
dependencies.

## Account views and claims

`CaseView::read` takes the expected Case identity, one `Observation` and borrowed
`AccountData` for Case, Policy, vault and the author receipt, with an optional
subject argument before the vault and the author receipt supplied last. Pass
`None` when no subject observation is needed; existing calls passing subject
`AccountData` remain supported. It checks observation slots,
owners, executable flags, encoding and sizes for protocol/custody accounts,
canonical protocol PDAs/bumps, Policy relationships, lifecycle consistency and
sufficient backing. The subject identity is authenticated from the Case and its
derived address. The subject account need not exist: RPC reads, claims and
monitoring do not fetch it. If supplied directly, it must have the expected address,
matching observation slot and nonexecutable flag. Its owner and bytes remain opaque.
Static Policy parameters and audit answer/reason compatibility use the same
predicates as the program. These shared rules do not replace the SDK's account,
observation and lifecycle checks, or reapply admission-time Clock checks to an
existing Policy.

`CaseView` exposes no decoded subject or current-pointer projection. Applications
must perform their own subject schema, controller and current-Case checks before
using a result. Later Case views check the canonical predecessor address;
this read does not independently fetch or validate that predecessor's account.

When using views directly, the caller must fetch these accounts and chain Clock
from one compatible observation; the RPC client performs that batching for you.
Supplying matching slot labels does not authenticate their source.
This SDK does not establish RPC consensus, network identity or chain finality.
The contract tests use one isolated ProgramTest bank. Do not combine an
older Policy or receipt with a newer Case and relabel it as one observation.

Views expose immutable typed getters. `case.audit.votes` records partial committee
progress; the threshold vote records the terminal response and pays auditor fees
in the same transaction. Each retained vote contains its auditor index, answer
and reason. Views reject duplicate or out-of-range voter indices, inconsistent
quorums and audited terminal states with unpaid fees.
`audit_fees_paid()` returns the total fee already transferred for an audited
result and `None` for other outcomes or before finalization. `Unknown` is an
auditor answer; technical timeout has no answer and pays no auditor fee.

`case.completion_method()` derives `Optimistic`, `Audited` or `TechnicalFailure`
from the finalized lifecycle, funding, deadlines, response and result. There is
no separate serialized completion-method field. The getter returns `None` before
finalization or for an inconsistent combination; `CaseView` still validates
committee votes, entitlements and backing before exposing a valid terminal view.
The author's `StakeReceipt.claimed` and Case's `fee_claimed` remain independent
payment markers. `author_receipt()` exposes the validated author receipt;
the Case no longer stores an author-claimed flag.

Authors and challengers share the `StakeReceipt` type and `receipt` PDA seed.
`AUTHOR_SIDE = 0` identifies the author receipt, whose owner and principal must
match the Case exactly. `CHALLENGE_SIDE = 1` identifies a contributor receipt.
An author receipt cannot already be claimed while the Case is pending. Receipt
side, owner, principal, PDA and observation checks prevent substituting one role
for the other. The contributor count includes only challenge-side receipts.

`ClaimState` distinguishes `Unavailable`, `Unclaimed { amount }` and `Claimed`,
including genuine zero entitlements. Contributor views authenticate the exact
owner's receipt and reuse the program's current payout calculation. On audited
No, the last unclaimed receipt receives the contributor pool's rounding
remainder; another claim can therefore change this quote before execution.
`Claimed` records discharge without reconstructing a historical payout amount.
Reading the result changes neither payout state nor payment markers.
Values remain integer base units (`u64` and aggregate `u128`); the selected USDC mint
has six decimals.

`claim(case, author, ClaimDestination { owner, destination })` builds withdrawals
for either role with the same accounts. Supply `author` from the validated Case;
the `owner` signs and the contract derives the payout role from that owner and
the receipt's side. The author's receipt supplies the vault's remaining author
liability, and the owner's receipt records the claim. For an author withdrawal,
both account positions refer to the same receipt; the transaction makes it
writable through the owner's receipt position. A contributor's withdrawal uses
two distinct receipts and does not need the author's signature.
The instruction does not need Policy: the finalized Case contains the fee used
to calculate contributor rewards.
`author_claim()` and `contributor_claim()` remain separate validated payout
quotes. When calling the lower-level
`Case::outstanding_liability(author_claimed)` method, obtain the boolean from
the authenticated author receipt in the same observation. `Case::unpaid()`
remains an alias for existing Rust callers.

`quote_contribution` returns the permitted credit in this observation. The
requested amount must be positive for both deposits and top-ups. The instruction
retains that amount and the program caps the actual debit
against capacity at execution. An old quote is never evidence of accepted credit.

## Run and inspect

From the repository root:

```sh
bash scripts/test.sh
```

The runner prepares missing tools and locked dependencies, checks formatting
and Clippy, tests SDK encoding and mocked RPC behavior, and executes the
contract locally using SDK builders. No RPC endpoint or wallet is needed.
SDK build output is isolated from independently locked workspaces.
The [RPC tests](tests/rpc.rs) exercise consistent reads, monitoring, signing,
simulation and confirmation against an in-memory transport.

`Policy::SPACE` and `Case::SPACE` are the canonical allocated account sizes.
`finalized_at` records terminal chain time and remains unchanged by later claims.
Programs, IDLs and SDK must use the same committee layout. The SDK does not
migrate incompatible accounts or transactions; recreate disposable fixture state
when changing formats.

Instruction accounts are grouped as signers, protocol accounts, token accounts
and external programs. Builders use the program's generated account types, so
their encoded order follows those declarations. Account state is grouped by
identity, configuration or funding, audit/lifecycle, entitlements, payment
progress and PDA metadata. Reordering fields changes the serialized layout even
when the discriminator and allocated size stay the same. Rebuild matching IDLs
and recreate incompatible accounts; retained signed transactions cannot be
reordered without fresh review and signatures.

The [settlement scenarios](../../tests/cases/audit_settlement.rs) and
[funding journey](../../tests/cases/journey.rs) use the SDK from deposits through
claims. Their [shared test support](../../tests/support/)
owns disposable keys, signing and ProgramTestContext execution; none of that code
enters the SDK.
The [runtime read tests](../../tests/cases/sdk_reads.rs)
cover stale funding and payout quotes, partial votes, last-claimant rewards,
Unknown versus timeout and rejected account substitutions/corrupt observations.
The [instruction tests](tests/instructions.rs) cover audit answers and reasons,
canonical accounts, quorum payout order, the author's aliased receipt and
contribution amount encoding. The contract tests verify signatures, reject
account substitutions and check atomic rollback of failed transactions.

Protocol history supports later attempts after audited No/Unknown or technical
failure. Each application owns its own transition rules and chooses which
authenticated Case result to use.
