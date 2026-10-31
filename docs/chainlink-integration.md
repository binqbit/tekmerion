# Chainlink developer integration

The version 3 adapter connects Tekmerion's generic auditor interface to a
qualified Chainlink confidential workflow. Tekmerion manages the Case, funds,
deadline and decision. The workflow reads the committed documents, researches
the claim and submits an authenticated answer. The adapter authenticates that
answer and applies it through Tekmerion's ordinary auditor interface.

The [verification guide](confidential-auditor.md) explains the safeguards,
retry rules and public qualification requirements. For the participant's flow,
see [Chainlink as a Tekmerion auditor](chainlink.md).

## Enrollment and execution

1. Build a 1-of-1 Policy naming the canonical `ConfidentialProfile` PDA as its
   auditor. Set the fee-recipient wallet, Case economics, `audit_trigger` and
   workflow configuration. `OnCreation` requires DirectAudit; `OnDispute` allows
   bonded challenges and voluntary direct audits. The Profile commits this term
   together with the rest of the immutable Policy rules.
2. Use the adapter SDK to create the Policy and register its immutable Profile
   in one transaction. Failure of either instruction rolls back both accounts.
   The Profile binds the Policy rules, authorized Forwarder and workflow identity, canonical agent
   configuration hash and maximum attempts. There is no specification upload or
   per-attempt account.
3. An author creates a Case under that Policy. A Challenge Case starts audit
   when its challenge target is funded; a DirectAudit Case starts it immediately
   with the author's audit-fee payment. A Challenge that never reaches its
   target can be accepted optimistically under core rules, without an AI audit.
4. Anyone prepares the Case's canonical Request. An HTTP trigger identifies the
   Case; scheduled discovery offers an independent launch path. CRE reads
   finalized core and adapter accounts, verifies their identities and checks its
   agent configuration against the Profile commitment.
5. CRE delivers an authenticated execution claim and observes it finalized.
   The workflow then retrieves Agreement and Result bytes from approved IPFS
   gateways and verifies their hashes against the Case. It passes the verified
   text to the researcher under workflow-owned instructions.
6. The model researches within the approved source domains and execution budget.
   The workflow validates its final answer, reason, cited sources and provider
   metadata. An authenticated Verdict records Ready. A verified technical failure
   may permit another execution within the fixed attempt limit and original
   deadline; valid Yes, No and Unknown answers never authorize a new execution.
7. Anyone applies Ready through the existing core vote CPI. The fee transfer and
   Applied state commit atomically. Neither receiving Ready nor retrying extends
   the Case deadline. Core timeout and participant claims remain unchanged.

Each author's Case has its own Request, execution count and withdrawals. A
completed answer for another Case does not cancel its audit or claims. The
application consuming Tekmerion decides how accepted Cases affect its own state.

## SDK boundaries

The independent `@tekmerion/sdk` and Rust `sdk/rust` expose Policy and Case
creation, direct payment, challenge contributions, ordinary auditor voting,
timeouts and withdrawals. A wallet auditor or an external program's PDA uses
the same core interface without installing or understanding Chainlink. The
[standalone TypeScript example](../sdk/typescript/examples/independent-auditor.ts)
creates a paid audit and builds an ordinary auditor vote.

Chainlink-specific clients live in optional packages:

- Rust `sdk/chainlink-rust`: adapter accounts, binding validation and unsigned
  register, prepare and apply instructions.
- TypeScript `@tekmerion/chainlink-sdk`: the equivalent client and instruction
  builders.
- TypeScript `@tekmerion/chainlink-sdk/wire`: portable adapter account and report
  codecs for CRE WASM and other consumers.
- TypeScript `@tekmerion/sdk/wire`: provider-independent byte, address and core
  identity utilities.

The adapter SDKs expose the fixed transaction compute budget; instruction
builders do not silently add transaction-level instructions. No SDK renders or
asks Solana to reproduce an AI request body. The immutable workflow owns the
instructions and provider adapter; its canonical agent configuration commits
the model revision, reasoning effort, explicitly allowed tools, permitted gateways
and research domains, and execution, response and tool budgets.

## Research and trust boundaries

The workflow accepts generic Agreement and Result text. It does not require a
Manteon event schema or hard-code company IPO rules. The Agreement supplies the
substantive criteria; the researcher may search, inspect relevant pages and
resolve contradictions within its allowed tools and budget. Documents and web
pages are evidence and cannot grant new tools or replace workflow instructions.
An empty tool list restricts the audit to the verified document text. Web search
must be enabled manually in the committed configuration. Tool calls outside that
permission, or source citations without recorded permitted retrieval, are rejected.

The current provider adapter uses OpenAI Responses with provider-hosted web
research. HTTP requests and secret access run inside the confidential workflow;
model inference and web research occur at the provider, outside the Chainlink
enclave. These execution guarantees require live release qualification.
Native Forwarder authentication establishes which configured
workflow supplied a report. It does not independently prove the provider's
internal execution, model weights, private reasoning or the truth of a source.

Solana verifies the Profile, Case commitments, authenticated execution,
request digest, report identity and deadline. It does **not** independently
recompute the initial prompt or receive a native attestation of its exact HTTP
bytes. Correct document retrieval, research and result interpretation belong
to the qualified workflow's trusted execution boundary.

Ready is permissionlessly applicable. Independent keepers should discover and
apply it before the deadline; a Ready record alone does not finalize the core
Case. A backend can fail to launch, relay or archive work, but it cannot submit
an authenticated verdict through its ordinary application permissions.

## Local checks and fixtures

The [Docker CRE environment](../workflows/chainlink-appeals/docker/README.md)
provides the official CLI, pinned build tools, workflow tests and HTTP simulation.
It requires no host Node, Bun or CRE installation. CLI simulation still requires
Chainlink authentication and, for Solana, a test wallet and configured RPC.

Run `bash scripts/test.sh` from Tekmerion. It restores pinned tools and
dependencies, builds real SBF programs and both network IDLs, checks SDKs and
committed vectors, tests CRE capability integration and compiles WASM. The
recorded researcher replay sends its report through the pinned upstream
Forwarder and applies the core vote. After the first build, use
`bash scripts/demo-confidential.sh --reuse-build` for that replay alone.

Local execution requires no tenant, model keys, public RPC, funded wallet or
Docker. Dependency restoration may use the network. Synthetic DON keys and
recorded model responses verify local interoperability and contract behavior;
they do not establish live confidential execution or model accuracy.

`tests/third-party/chainlink-solana/UPSTREAM.json` pins imported Forwarder source
hashes. `sdk/fixtures/chainlink/confidential-{mainnet,devnet}.json` stores runtime
vectors. `tests/fixtures/confidential/` stores committed documents, agent
configuration and recorded research. Normal checks reject drift. Intentional
ABI/vector changes can be regenerated with
`bash scripts/test.sh --update-idl --update-fixtures` and require review.

## Public deployment

A Policy administrator selects the Forwarder program and state when creating
the immutable Profile. There is no built-in address allowlist or release gate.
The contract verifies that the program is executable, owns the selected state,
and supplies the correct state-scoped PDA signer for reports.

Enrollment does not establish who operates the chosen Forwarder. Before live
use, verify program and workflow identities, Forwarder/DON authorization,
confidential transport, secret handling,
claim-before-AI ordering, retry and terminal delivery, independent discovery,
and sufficient transaction and deadline budgets. See
[qualification](confidential-auditor.md#local-verification-and-release-qualification).

The Profile fixes addresses; it does not freeze upgrades or oracle-configuration
changes permitted by the external Forwarder. Its governance remains part of the
selected transport's trust model.

Build artifacts with `bash scripts/build.sh`; building does not deploy them.
Configure the workflow using its [README](../workflows/chainlink-appeals/README.md).
Evaluate the model on reviewed examples of valid claims, contradictions,
missing evidence, identity mismatches and prompt injection. Authentication and
fixed hashes cannot replace that evaluation.
