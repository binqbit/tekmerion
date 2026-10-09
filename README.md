# Tekmerion

Tekmerion helps applications resolve disputed statements through independent auditors. A Solana smart contract holds participants' deposits and enforces the review rules agreed in advance.

An author backs a statement with USDC. Other participants can pool funds to challenge it. A fully funded challenge goes to the appointed auditors, whose matching answers determine the outcome. That outcome determines the auditors' reward and the amounts participants can withdraw.

The repository contains the smart contract and Rust and TypeScript SDKs for integrating it into other applications.

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

## Repository structure

| Directory             | Purpose                         |
| --------------------- | ------------------------------- |
| `programs/tekmerion/` | Smart contract                  |
| `sdk/rust/`           | Rust SDK                        |
| `sdk/typescript/`     | TypeScript SDK                  |
| `tests/`              | Local contract tests            |
| `docs/`               | Product and developer guides    |
| `idl/`                | Contract interface descriptions |
| `scripts/`            | Build and test tooling          |

## Documentation

| Guide                                              | What it covers                                                          |
| -------------------------------------------------- | ----------------------------------------------------------------------- |
| [Concept](docs/concept.md)                         | Why Tekmerion exists and where it fits.                                 |
| [How Tekmerion works](docs/how-it-works.md)        | Funding, voting, payments and contract checks, explained with diagrams. |
| [SDK integration](sdk/README.md)                   | Connecting an application using the Rust or TypeScript SDK.             |
| [Contract reference](programs/tekmerion/README.md) | Technical details for developers working with the contract.             |
| [Production build](docs/build.md)                  | Building the SBF program and inspecting its release artifacts.          |
| [Local testing](tests/README.md)                   | Setup requirements, running tests and test coverage.                    |
