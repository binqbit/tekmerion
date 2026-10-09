# Tekmerion

Tekmerion helps applications resolve disputed statements through independent auditors. A Solana smart contract holds participants' deposits and enforces the review rules agreed in advance.

An author backs a statement with USDC. Other participants can pool funds to challenge it. A fully funded challenge goes to the appointed auditors, whose matching answers determine the outcome. That outcome determines the auditors' reward and the amounts participants can withdraw.

The repository contains the smart contract and Rust SDK for integrating it into other applications.

## Build the contract

Run from the repository root:

```sh
bash scripts/build.sh
```

This builds the production SBF program with the mainnet USDC profile. The program, generated IDL, build metadata and checksums are placed in `target/production/release/`. See the [build guide](docs/build.md) for requirements and artifact details. Building does not deploy the program.

## Repository structure

| Directory             | Purpose                         |
| --------------------- | ------------------------------- |
| `programs/tekmerion/` | Smart contract                  |
| `sdk/rust/`           | Rust SDK                        |
| `tests/`              | Local contract tests            |
| `docs/`               | Product and developer guides    |
| `idl/`                | Contract interface descriptions |
| `scripts/`            | Production build tooling        |

## Documentation

| Guide                                              | What it covers                                                          |
| -------------------------------------------------- | ----------------------------------------------------------------------- |
| [Concept](docs/concept.md)                         | Why Tekmerion exists and where it fits.                                 |
| [How Tekmerion works](docs/how-it-works.md)        | Funding, voting, payments and contract checks, explained with diagrams. |
| [Contract reference](programs/tekmerion/README.md) | Technical details for developers working with the contract.             |
| [Production build](docs/build.md)                  | Building the SBF program and inspecting its release artifacts.          |
| [Local testing](tests/README.md)                   | Setup requirements, running tests and test coverage.                    |
