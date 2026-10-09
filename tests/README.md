# Local contract execution

The contract tests execute the compiled Tekmerion SBF program in a local Solana bank. Tests use Rust 1.97.1, Anchor 1.2.1 and `solana-program-test` 4.2.0 with disposable signers, synthetic balances and a controlled Clock.

Set `BPF_OUT_DIR` to the directory containing the compiled SBF programs when executing the test workspace. Token operations execute against the Token program in the local bank. Test setup and execution require no public RPC or live wallet. Independent workspaces retain separate Cargo output directories.

`TestContext` exposes its actual `ProgramTestContext` and retains the actors and addresses used by each Case scenario. A private Tokio runtime lets the synchronous scenario helpers call `banks_client` directly.

```text
tests/
  Cargo.toml           standalone contract test workspace
  lib.rs               reusable scenario support
  contract.rs          test suite entrypoint
  cases/               contract execution and SDK account-read scenarios
  support/
    setup/
      context.rs       local bank and context setup
      programs.rs      program registration
      token.rs         reusable mint and token-account setup
      case.rs          default actors, Policy rules and Case setup
    context.rs         Case scenario actors and addresses
    transactions.rs    signing, execution and rollback assertions
  examples/
    sdk_fixtures.rs    SDK compatibility vector generator
  programs/
    sdk_consumer/      minimal Anchor CPI consumer with PDA signing
```

Setup helpers prepare local state. Assertions read actual bank accounts and transaction history. Standard Solana messages and transactions verify signatures and the 1,232-byte packet limit before executing through BanksClient. Rejection checks compare protected accounts, rent and token balances, allowing only the independently queried transaction fee to change.

Tests that inject corrupted program-owned accounts explicitly exercise defensive checks. They do not assume an ordinary caller can rewrite those accounts.

For a custom async test, take the builder from `setup::programs::program_test()`, register extra programs or accounts, and pass it with the initial Clock to `setup::context::start`. The result is a native `ProgramTestContext`; token setup helpers accept it directly. Case scenarios can pass the same customized builder to `TestContext::with_program_test`.

Both authors and contributors have canonical `StakeReceipt` accounts: side `0` records the author's actual bond and side `1` records challenge contributions. Both use `claim`; author claims repeat the same receipt in its readonly backing and writable claiming positions. Reading a finalized Case does not require another application program.

The default policy has one independent auditor for concise settlement vectors. Committee scenarios additionally execute 2-of-3, 3-of-5 and maximum-size 9-of-9 decisions. Auditors submit an answer and reason directly, with the signature binding the vote to its Case and accounts. Every auditor votes once. Retained votes identify the auditor and preserve the answer and reason. The vote reaching the threshold atomically settles the case and transfers the fee to the matching voters' configured token wallets in committee order. Inconclusive opinions are real audit answers; an expired case without a matching quorum refunds deposits without an audit fee.

Coverage includes:

- Reusable token setup on a standalone `ProgramTestContext`, including repeated mint operations without changing the test Clock.
- Contributions, top-ups, capped final credits, donations, clock boundaries and optimistic refunds.
- Configured wallet authorization, opaque subjects, fresh successor collateral, cross-case isolation and stable terminal results across claims.
- Absent and legitimately closed subjects preserve SDK reads, settlement and claims; subject identity remains bound to the Case.
- First-use Policy approval: pre-signed creation uses the rules at execution, while confirmed acquisition followed by review/signing prevents later rule changes.
- Invalid Policy creation and rent rollback, executable subjects, and malformed or inconsistent state at the canonical predecessor address; repaired inputs permit a fresh attempt without consuming previous claims.
- Auditor signatures, unauthorized identities, duplicate votes, invalid answer/reason pairs, signed Case/account substitution and policy immutability.
- Votes before full funding, substitution of another valid Policy, and pre-signed votes whose required quorum destinations change before execution; rejected votes preserve state and remain retryable with a newly reviewed transaction.
- Matching-answer quorums, conflicting votes, strict audit deadlines, winners-only payments and deterministic fee rounding.
- Missing, reordered, read-only, duplicated, substituted, malformed, uninitialized, frozen and wrong-mint payment accounts; rollback of the final vote and earlier fee transfers; successful retry and replay rejection.
- Yes/No/Unknown allocation vectors, zero-fee audits, contributor rounding in both claim orders, aggregate custody backing and successful claims after thawing a frozen destination.
- Funding-time custody deficits cannot be hidden by fresh stakes, top-ups or final contributions. Zero-entitlement claims still validate backing and destinations; corrupted vault authorities cannot consume claim receipts.
- Author receipt creation and rollback, actual bond amounts, role and principal forgery rejection, independent receipts across later cases, persisted author receipt aliases, shared claim encoding and zero-entitlement claim discharge.
- SDK account authentication, consistent observations, voting progress, quotes and paid markers.
- Real CPI from a caller PDA, controller/signature rejection, successor predecessor accounts and atomic quorum payouts through remaining accounts.

The [CPI consumer](programs/sdk_consumer/README.md) is registered alongside Tekmerion for the CPI scenarios.

[SDK vectors](../sdk/fixtures/README.md) are generated from local contract execution.

The direct vote ABI test independently checks the Anchor discriminator and answer/reason bytes. Signed transaction mutations verify that changing the Case, vault or answer invalidates the signature, and custody checks reject mismatched Case/vault pairs even with a new signature. Tests exercise local execution and transaction atomicity; a deployment and external audit operations require their own operational verification.
