# Rust CPI consumer

This test-only SBF program demonstrates calling Tekmerion through the Rust SDK
with a PDA authority. It owns no application state. Its two instructions forward
Case creation and audit voting to the generated Anchor CPI interface.

The [runtime tests](../../cases/cpi.rs) execute both programs in ProgramTest,
including Tekmerion's nested SPL Token transfers. The fixture's fixed program
address is a local test identity, not a deployment.

## Dependency configuration

An onchain consumer disables SDK defaults, selects one network, and enables CPI:

```toml
[dependencies]
anchor-lang = { version = "=1.2.1", default-features = false, features = ["solana-v3"] }
tekmerion-sdk = { path = "/path/to/tekmerion/sdk/rust", default-features = false, features = ["mainnet", "cpi"] }
```

Use `devnet` instead of `mainnet` for that network. Do not enable `host` in the
SBF dependency graph. The fixture's [manifest](Cargo.toml) forwards its network
feature and is built independently from the host test harness.

## Sign with the caller's PDA

The [complete source](src/lib.rs) requires the controller's transaction signature
and verifies `authority = PDA([b"authority", controller], consumer_program_id)`.
The outer authority account is an `UncheckedAccount` constrained by those seeds;
it becomes a signer only in the inner Tekmerion invocation.

The Policy admin must configure this PDA as `client_authority` for Case creation,
or include it as an auditor authority for voting. These are separate roles;
possession of creation authority does not grant an audit vote. The author still
signs a Case creation and authorizes the bond transfer from their token account.

Each handler constructs the SDK's original generated account context and calls:

```rust,ignore
let controller = ctx.accounts.controller.key();
let bump = [ctx.bumps.authority];
let seeds = [b"authority".as_slice(), controller.as_ref(), bump.as_ref()];
let signer_seeds = [&seeds[..]];
let accounts = tekmerion_sdk::cpi::accounts::SubmitAudit {
    auditor: ctx.accounts.authority.to_account_info(),
    case: ctx.accounts.case.to_account_info(),
    policy: ctx.accounts.policy.to_account_info(),
    mint: ctx.accounts.mint.to_account_info(),
    vault: ctx.accounts.vault.to_account_info(),
    token_program: ctx.accounts.token_program.to_account_info(),
};
let cpi = CpiContext::new_with_signer(
    ctx.accounts.tekmerion_program.key(),
    accounts,
    &signer_seeds,
)
.with_remaining_accounts(ctx.remaining_accounts.to_vec());

tekmerion_sdk::cpi::submit_audit(cpi, answer, reason)
```

Anchor 1.2.1's `CpiContext` constructor takes the callee's **public key**. The
fixture validates the callee with
`Program<'info, tekmerion_sdk::program::program::Tekmerion>`. The generated CPI
function serializes the instruction and performs `invoke_signed`; the consumer
does not duplicate that encoding.

Protocol accounts are forwarded as `UncheckedAccount` because Tekmerion performs
their canonical-address, ownership, state and token validation. Case, vault and
receipt accounts do not exist before creation. A consumer that deserializes a
Tekmerion account and reads it after CPI must reload it before using the new state.
An application also owns validation of its subject and permission rules.

## Forward remaining accounts

Both handlers preserve the caller's remaining-account order and privileges:

- For Case nonce zero, pass none. For a successor, pass the canonical previous
  Case as readonly. Tekmerion checks that its terminal outcome permits a retry.
- Before an audit quorum, pass none. For the decisive vote, pass writable USDC
  accounts for the winning voters' fee recipients, in Policy index order.

The tests reject a missing predecessor and missing, reversed or readonly payout
accounts, then verify complete rollback and successful corrected calls. They
also verify rejection of a direct unsigned PDA and a mismatched controller.
The 2-of-3 audit test proves that arrival order does not change fee ordering:
a fee of seven base units pays four to index zero and three to index one.

## Run

From the repository root:

```sh
bash scripts/test.sh
```

The runner builds `sdk_consumer.so` using the SDK without `host`, then registers
it alongside the real Tekmerion SBF program for the CPI tests. It does not add
the fixture to the deployed contract or its IDL.
