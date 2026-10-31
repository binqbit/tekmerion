# Chainlink auditor SDK

Optional integration for the Chainlink auditor adapter. The independent
[`@tekmerion/sdk`](../typescript/README.md) owns Policy, Case, voting, fees and
claims. This package adds only adapter enrollment, execution state, authenticated
report encoding and permissionless application of an accepted verdict.

## Entry points

| Import                          | Purpose                                                                      |
| ------------------------------- | ---------------------------------------------------------------------------- |
| `@tekmerion/chainlink-sdk`      | `ConfidentialChainlinkClient`, unsigned Solana instructions and wire exports |
| `@tekmerion/chainlink-sdk/wire` | Portable account and report codecs for CRE, browsers and offline validation  |

The portable entry point has no Node globals, RPC client, credentials or CRE
runtime dependency. It uses `@tekmerion/sdk/wire` for native Case identity,
hashing, address encoding and PDA derivation.

## Lifecycle

1. Use `createPolicy(input, config)` and submit both returned instructions in
   one transaction. It creates the Policy and its immutable Profile together;
   either both succeed or neither exists. The Profile fixes the authorized
   Forwarder/workflow, agent configuration commitment and retry limit.
2. Prepare one Request for a funded Case. Anyone can pay the account rent.
3. The authorized workflow claims an execution through the native Forwarder,
   reads the finalized claim, verifies the committed documents and runs the audit.
4. The workflow delivers a verdict or a classified technical failure. The
   Request stores its current attempt and report; no separate Attempt account is
   required. Only a retryable technical failure permits another execution.
5. Anyone can call `applyVerdict` on a ready Request before the Case deadline.
   The adapter signs its standard Tekmerion vote with the Profile PDA.

The adapter verifies origin, Case/Policy binding, execution identity, deadline
and replay rules. The workflow owns agent execution and document verification;
the SDK does not render or hash a model HTTP request.

## Unsigned client

```ts
import { ConfidentialChainlinkClient } from '@tekmerion/chainlink-sdk';

const adapter = new ConfidentialChainlinkClient('devnet');
const enrollment = adapter.createPolicy(policyInput, workflowConfig);
const prepare = adapter.prepareRequest(payer, caseIdentity);
const apply = adapter.applyVerdict(caseIdentity, auditorFeeTokenAccount);
```

The application owns its RPC connection, fee payer, transaction submission and
confirmation. `applyVerdict` requires no privileged caller. Its destination must
be the token account of the Policy's configured auditor fee recipient.

`policyInput.rules` must name `adapter.profile(policyAddress)` as its sole auditor
with threshold one. The lower-level `registerProfile` instruction remains
available for transaction composition. It cannot change or close an existing
Profile. A different model, reasoning mode, tool policy or retry limit requires
a new Policy and Profile.

For account reads, check owners and finality before passing the raw Profile,
Policy, Case and Request bytes to `validateConfidentialBinding`. The resulting
context contains the committed document hashes and audit cutoff. It is an
interpretation of supplied chain data, not an independent proof of RPC finality.

## Installation and verification

Packages are maintained in this checkout and are not published. Build the core
SDK first, then install and build this package:

```sh
npm ci --prefix sdk/typescript
npm run build --prefix sdk/typescript
npm ci --prefix sdk/chainlink-typescript
npm test --prefix sdk/chainlink-typescript
```

For another project, pack both SDKs and install their generated archives together
with the declared `@solana/web3.js` peer version. No Manteon package is required.

Tests compare v3 account commitments, instruction privileges and report bytes
with independent Rust/SBF fixtures for both networks. The same contract builds
accept the Forwarder program and state declared by each immutable Profile.
Local fixtures do not qualify a live confidential deployment; state ownership,
signer and workflow checks remain enforced.
