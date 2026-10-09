# SDK compatibility vectors

`sdk-mainnet.json` and `sdk-devnet.json` come from
[`sdk_fixtures.rs`](../../tests/examples/sdk_fixtures.rs), using Rust instruction
builders and actual execution of the compiled contract in Solana's local test
runtime. They contain public disposable account state, not wallet secrets.

Each profile contains 26 instruction vectors and 22 account snapshots. Vectors
include all nine instructions, all valid audit answer/reason pairs, remaining
account order, aliased author claims and `u64::MAX` arguments. Snapshots cover
funding, partial votes, Yes/No/Unknown decisions, timeout, optimistic settlement
and claims. Expected contributions, quorum fees and claims are calculated by
validated Rust views of those same bank accounts.

The retained mainnet/devnet SDK profiles select public addresses for encoding
and validation; testing those byte profiles does not contact either network.
