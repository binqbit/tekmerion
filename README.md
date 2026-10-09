# Tekmerion

Tekmerion helps applications resolve disputed statements through independent auditors. A Solana smart contract holds participants' deposits and enforces the review rules agreed in advance.

An author backs a statement with USDC. Other participants can pool funds to challenge it. A fully funded challenge goes to the appointed auditors, whose matching answers determine the outcome. That outcome determines the auditors' reward and the amounts participants can withdraw.

The repository contains the smart contract.

## Repository structure

| Directory             | Purpose                         |
| --------------------- | ------------------------------- |
| `programs/tekmerion/` | Smart contract                  |
| `docs/`               | Product and developer guides    |
| `idl/`                | Contract interface descriptions |

## Documentation

| Guide                                              | What it covers                                                          |
| -------------------------------------------------- | ----------------------------------------------------------------------- |
| [Concept](docs/concept.md)                         | Why Tekmerion exists and where it fits.                                 |
| [How Tekmerion works](docs/how-it-works.md)        | Funding, voting, payments and contract checks, explained with diagrams. |
| [Contract reference](programs/tekmerion/README.md) | Technical details for developers working with the contract.             |
