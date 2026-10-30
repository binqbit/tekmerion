# Tekmerion Chainlink Rust SDK

This optional integration crate builds instructions and authenticates accounts
for the Chainlink auditor adapter. Applications that use ordinary wallet or
program/PDA auditors need only `tekmerion-sdk`.

Select one network feature (`mainnet` or `devnet`). `confidential::create_policy`
returns the core creation and Profile registration instructions; submit both in
one transaction to create them atomically. The rules must name the Policy's
canonical Profile as their sole auditor. `confidential::register_profile` remains
available for lower-level composition before Policy use. Existing Profiles
cannot be updated or closed.
`prepare_request` creates the deterministic Request for a funded Case.
`receiver_accounts` describes the six native Forwarder receiver accounts.
`apply_verdict` applies an authenticated Ready result through the ordinary
Tekmerion vote. Its fee destination is validated by the core contract.

The `read_profile` and `read_request` functions validate ownership, addresses,
Policy rules, Case commitments, state and terminal results. They are not just
unchecked deserializers. `payload::AuditReport` strictly encodes and decodes the
version 3 claim, verdict and technical-failure messages.

There are no AI request renderers or specification upload instructions in this
SDK. The workflow owns agent configuration and verifies its canonical hash
against the Profile. See the [protocol reference](../../docs/confidential-auditor.md)
for account fields, retry rules and the release qualification boundary.
