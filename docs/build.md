# Build the production contract

From the repository root, run:

```sh
bash scripts/build.sh
```

The command builds the release SBF program for Solana with the fixed `mainnet`
USDC profile. The output bundle is `target/production/release/`.
Use `bash scripts/build.sh --help` to see the command's usage; it takes no build
options or network selector.

## Requirements and tools

Install Bash, Python 3.11 or newer, `rustup`, a C compiler, `pkg-config` and the
OpenSSL development headers for your operating system. The standalone contract
build does not require Node.js, npm or the local contract-test workspace.

The script prepares missing pinned tools and dependencies. The first setup
requires internet access for downloads; subsequent runs reuse cached tools and
Cargo dependencies. Compilation uses the locked dependency graph offline.

| Tool                  | Version and role                                                      |
| --------------------- | --------------------------------------------------------------------- |
| Host Rust             | `1.97.1`, from `rust-toolchain.toml`; host tooling and IDL generation |
| Anchor CLI            | `1.2.1`; generates the program interface                              |
| `cargo-build-sbf`     | `4.0.0`; builds the Solana program                                    |
| Solana platform tools | `v1.53`; SBF compiler toolchain                                       |
| SBF architecture      | `v0`                                                                  |

The program is built with `--no-default-features --features mainnet` through
`cargo-build-sbf`, with release overflow checks enabled. The script rejects SBF
compiler errors and unknown-syscall diagnostics, checks that the binary is an
ELF64 SBF v0 shared object, and verifies the generated IDL against the maintained
[interface](../idl/tekmerion.json). It also checks the program identity and the
mainnet mint recorded by that interface.

Build intermediates live under `target/production/`. Cargo and the SBF tooling
also reuse their download caches. Installed host tools are shared under
`target/tools/`; an existing tool installation under
`target/local-contracts/tools/` can be reused. These directories are ignored by
Git.

## Release bundle

| File              | Contents                                                        |
| ----------------- | --------------------------------------------------------------- |
| `tekmerion.so`    | Release SBF program binary                                      |
| `tekmerion.json`  | Generated IDL checked against the maintained mainnet interface  |
| `build-info.json` | Tool versions, source fingerprint, program ID and selected mint |
| `SHA256SUMS`      | SHA-256 checksums for the other release files                   |

`build-info.json` records the actual host and SBF compiler versions, build
flags, program identity, mint, and Git revision and dirty status when available.
Its `source_files` map contains hashes of the selected program, configuration,
interface and build-script inputs; `source_sha256` fingerprints that map. The
`artifacts` map records the binary and IDL hashes. `SHA256SUMS` covers those two
files and `build-info.json`.

The bundle contains no private key. Keep its files together when handing a build
to deployment tooling. On systems with `sha256sum`, verify the bundle from its
directory:

```sh
cd target/production/release
sha256sum --check SHA256SUMS
```

Checksums detect changes to the bundle files. The metadata identifies selected
build inputs; it does not establish reproducibility or prove that a deployed
program matches this binary.

## Identity, checks and deployment

The program ID is `TEKxbA5q3EhSDSBeYiW9AT3GGPyzruBSQUcuQXxdy3s`. This build
uses the mainnet USDC mint `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`.
The profile selects public identities; building does not contact an RPC
endpoint, deploy a program or perform a token transfer.

Before deployment, run the separate local validation command:

```sh
bash scripts/test.sh
```

Run it from the repository root. It builds and checks the program, exercises
contract transactions in a local Solana bank, and tests both SDKs with local
fixtures and mock RPCs. Its additional setup requirements and coverage are in
the [test guide](../tests/README.md). Building the release bundle does not run
this suite.
