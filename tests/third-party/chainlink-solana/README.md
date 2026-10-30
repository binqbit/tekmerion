# Local forwarder authentication boundary

The source files listed in `UPSTREAM.json` are unchanged files from
[smartcontractkit/chainlink-solana](https://github.com/smartcontractkit/chainlink-solana/tree/bc2c7530dbcb8d9e1f28a7d8363c6d56620b9a22).
The adjacent workspace manifest and lockfile isolate the upstream Anchor 0.31
build from Tekmerion's Anchor 1.2 workspace. `scripts/verify-chainlink-source.py`
checks both SHA-256 and Git blob identities before compiling.

This program is loaded only into a disposable local bank with synthetic oracle
keys. Its program ID, state, oracle configuration, signatures and workflow
identity are **not live Chainlink deployments or attestations**. It proves that
the receiver accepts the pinned forwarder's actual signature and account-order
validation path. Network delivery and production oracle configuration require
separate live qualification.
