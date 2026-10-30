# Tekmerion

Tekmerion helps applications resolve disputed statements through independent auditors. A Solana smart contract holds participants' deposits and enforces the review rules agreed in advance.

An author backs a statement with USDC. Other participants can pool funds to challenge it. A fully funded challenge goes to the appointed auditors, whose matching answers determine the outcome. That outcome determines the auditors' reward and the amounts participants can withdraw.

Alternatively, the author can choose a direct audit when submitting: pay only the fixed audit fee and start the review immediately, without a bond or challenge window. The fee is earned on any audited answer; if the audit times out without a quorum, the author can reclaim it. Each Case permanently records which mode was chosen.

Alongside human committees, Tekmerion supports an automated auditor using AI
and Chainlink. It follows the same review and payment rules. See
[Chainlink as a Tekmerion auditor](docs/chainlink.md) for the review process,
final report and conditions for public use.
The repository contains the contracts, workflow, and Rust and TypeScript SDKs.

## Build the contract

Run from the repository root:

```sh
bash scripts/build.sh
```

This builds the production SBF program with the mainnet USDC profile. The program, generated IDL, build metadata and checksums are placed in `target/production/release/`. See the [build guide](docs/build.md) for requirements and artifact details. Building does not deploy the program; run the local checks below before deployment.

## Run locally

Install the [local setup requirements](tests/README.md), then run from the repository root:

```sh
bash scripts/test.sh
```

The command prepares missing build tools and dependencies, then runs all contract and SDK tests locally. The first setup needs internet access for downloads; later runs reuse the local caches. Tests use temporary test funds and require no wallet.

For a complete linked local appeal with recorded model responses and real contracts, run
`bash scripts/demo-confidential.sh`. No tenant, API key or external server is needed.

For the official CRE CLI in a container, follow the
[Docker simulation guide](workflows/chainlink-appeals/docker/README.md).
It includes workflow tests, browser login and a local HTTP listener.

## Repository structure

| Directory                      | Purpose                                   |
| ------------------------------ | ----------------------------------------- |
| `programs/chainlink-auditor/`  | Authenticated CRE receiver and core CPI   |
| `workflows/chainlink-appeals/` | CRE workflow and recorded local execution |
| `sdk/chainlink-rust/`          | Adapter Rust SDK                          |
| `programs/tekmerion/`          | Smart contract                            |
| `sdk/rust/`                    | Rust SDK                                  |
| `sdk/typescript/`              | TypeScript SDK                            |
| `tests/`                       | Local contract tests                      |
| `docs/`                        | Product and developer guides              |
| `idl/`                         | Contract interface descriptions           |
| `scripts/`                     | Build and test tooling                    |

## Documentation

| Guide                                                      | What it covers                                                          |
| ---------------------------------------------------------- | ----------------------------------------------------------------------- |
| [Concept](docs/concept.md)                                 | Why Tekmerion exists and where it fits.                                 |
| [How Tekmerion works](docs/how-it-works.md)                | Funding, voting, payments and contract checks, explained with diagrams. |
| [Chainlink auditor](docs/chainlink.md)                     | Automated review, clear agreement rules and release conditions.         |
| [Chainlink developer guide](docs/chainlink-integration.md) | Adapter setup, workflow integration and reproducible testing.           |
| [SDK integration](sdk/README.md)                           | Connecting an application using the Rust or TypeScript SDK.             |
| [Contract reference](programs/tekmerion/README.md)         | Technical details for developers working with the contract.             |
| [Production build](docs/build.md)                          | Building the SBF program and inspecting its release artifacts.          |
| [Local testing](tests/README.md)                           | Setup requirements, running tests and test coverage.                    |
