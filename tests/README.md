# Local contract execution

The contract tests execute the compiled Tekmerion SBF program in a local Solana bank. Tests use Rust 1.97.1, Anchor 1.2.1 and `solana-program-test` 4.2.0 with disposable signers, synthetic balances and a controlled Clock.

Set `BPF_OUT_DIR` to the directory containing the compiled SBF programs when executing the test workspace. Token operations execute against the Token program in the local bank. Test setup and execution require no public RPC or live wallet. Independent workspaces retain separate Cargo output directories.

`TestContext` exposes its actual `ProgramTestContext` and retains the actors and addresses used by each Case scenario. A private Tokio runtime lets the synchronous scenario helpers call `banks_client` directly.

```text
tests/
  Cargo.toml           standalone contract test workspace
  lib.rs               reusable scenario support
  contract.rs          test suite entrypoint
  cases/               contract execution scenarios
  support/
    setup/
      context.rs       local bank and context setup
      programs.rs      program registration
      token.rs         reusable mint and token-account setup
      case.rs          default actors, Policy rules and Case setup
    context.rs         Case scenario actors and addresses
    transactions.rs    signing, execution and rollback assertions
  programs/
    sdk_consumer/      minimal Anchor CPI consumer with PDA signing
```

Setup helpers prepare local state. Assertions read actual bank accounts and transaction history. Standard Solana messages and transactions verify signatures and the 1,232-byte packet limit before executing through BanksClient. Rejection checks compare protected accounts, rent and token balances, allowing only the independently queried transaction fee to change.

Tests that inject corrupted program-owned accounts explicitly exercise defensive checks. They do not assume an ordinary caller can rewrite those accounts.

For a custom async test, take the builder from `setup::programs::program_test()`, register extra programs or accounts, and pass it with the initial Clock to `setup::context::start`. The result is a native `ProgramTestContext`; token setup helpers accept it directly. Case scenarios can pass the same customized builder to `TestContext::with_program_test`.

Both authors and contributors have canonical `StakeReceipt` accounts: side `0` records the author's actual bond and side `1` records challenge contributions. Both use `claim`; author claims repeat the same receipt in its readonly backing and writable claiming positions. Reading a finalized Case does not require another application program.

The default policy has one independent auditor for concise settlement vectors.

Coverage includes:

- Reusable token setup on a standalone `ProgramTestContext`, including repeated mint operations without changing the test Clock.
