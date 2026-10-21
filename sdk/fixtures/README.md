# SDK compatibility vectors

`sdk-mainnet.json` and `sdk-devnet.json` come from
[`sdk_fixtures.rs`](../../tests/examples/sdk_fixtures.rs), using Rust instruction
builders and actual execution of the compiled contract in Solana's local test
runtime. They contain public disposable account state, not wallet secrets.

Each profile contains 27 instruction vectors and 36 account snapshots. Vectors
include all nine instructions, all valid audit answer/reason pairs, remaining
account order, aliased author claims and `u64::MAX` arguments. Snapshots cover
funding, partial votes, Yes/No/Unknown decisions, timeout, optimistic settlement
and claims, plus immediate direct audits, all their answers, timeout refunds and zero-fee author receipts. Expected contributions, quorum fees and claims are calculated by
validated Rust views of those same bank accounts.

The TypeScript tests compare both public identity profiles against these
vectors offline. Rust RPC tests use the raw account bytes with a mocked
transport. Neither accesses a public chain or requires a wallet.

From the repository root, the shared test command generates vectors for the
contract's default identity in a local Solana bank and compares them with
`sdk-mainnet.json`, then runs the SDK checks:

```sh
bash scripts/test.sh
```

After an intentional interface or calculation change, regenerate the default
vectors with the same runner and review the differences:

```sh
bash scripts/test.sh --update-fixtures
```

`--update-idl` updates the default contract IDL when its interface changes.
Keep the SDK's bundled IDLs synchronized with the maintained deployment IDLs.
The retained mainnet/devnet SDK profiles select public addresses for encoding
and validation; testing those byte profiles does not contact either network.
