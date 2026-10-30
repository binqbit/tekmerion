# Tekmerion confidential audit workflow

This package implements a generic researcher for Tekmerion. An authenticated
trigger supplies only a Case address. The workflow reads finalized Solana
accounts, verifies the committed Agreement and Result, researches the claim,
and delivers a signed verdict through the Chainlink Forwarder.

The [adapter reference](../../docs/confidential-auditor.md) defines authorization,
execution claims, retries and application to Tekmerion.

## Run locally

For an OS-independent environment with the official CRE CLI, use the
[Docker setup](docker/README.md). It builds this workflow's WASM, includes a
local HTTP listener and keeps CRE login state in a Docker volume.

From Tekmerion, run `bash scripts/test.sh`. After the initial build,
`bash scripts/demo-confidential.sh --reuse-build` replays the actual workflow
report through real SBF adapter, Forwarder and core contracts. The model and DON
signatures are synthetic local fixtures. No external model, public RPC or Docker
is required; these tests do not establish live TEE execution or model accuracy.

For workflow-only checks:

```sh
npm ci --prefix ../../sdk/typescript
npm run build --prefix ../../sdk/typescript
npm ci --prefix ../../sdk/chainlink-typescript
npm run build --prefix ../../sdk/chainlink-typescript
npm ci
npm test
npm run test:wasm
```

## Execution

1. Read Case, Policy, Profile, Request and Clock through the official finalized
   Solana capability. Check ownership, PDA identity and immutable commitments.
2. Compare the semantic agent configuration hash with Profile. Check the Case
   deadline, attempt limit and available execution budget.
3. Obtain the native CRE execution ID. Deliver an execution claim and confirm
   its finalized state before fetching documents or calling the model.
4. Fetch the two committed raw IPFS documents. Verify SHA-256 and UTF-8 in code.
   The documents may describe any claim; no Manteon template is required.
5. Call the pinned OpenAI Responses model with the verified text, workflow-owned
   instructions, reasoning effort and only the explicitly allowed tools. When
   web_search is allowed, the provider's agent can search, open pages and revisit
   conflicting evidence within its domain and call limits. With no tools, it
   evaluates only the supplied documents.
6. Validate completion, model identity, tool types, source domains, citations,
   token/tool budgets and the structured Yes/No/Unknown decision.
7. Deliver the terminal report. Technical failure is a separate report and never
   an Unknown vote. Only authenticated retryable errors can release a new attempt.
8. Publish the final Markdown audit report after terminal delivery.
   Archive failure cannot prevent a verdict. A permissionless SDK instruction
   applies a Ready verdict to Tekmerion; the Market worker or an independent
   keeper can call it without an administrator key.

## Code organization

```text
src/
  main.ts                  CRE runner and configuration parser
  workflow/                HTTP/cron handlers and composition of one audit
  audit/                   Execution claim, terminal report and audit reports
  research/                Verified documents, research limits and agent execution
    openai/                Provider request, fixed prompt and response parsing
  integrations/
    solana/                Audit account reads, pending discovery and report delivery
    chainlink/             Native CRE HTTP, Solana and signed-report capabilities
    http.ts                Model credentials and artifact publication
  shared/                  Byte, JSON, URL and IPFS utilities
tests/
  audit/                   Lifecycle, ordering, failures and exact report replay
  research/                Documents, provider responses and source permissions
  integrations/            Native CRE capability routing and discovery
  wasm/                    Compiled artifact execution through the CRE/WASI host ABI
  support/                 Recorded model responses and protocol fixtures
```

Start with [workflow/run.ts](src/workflow/run.ts) to see how the account reader,
HTTP transport and report channel are connected. Then read
[audit/execution.ts](src/audit/execution.ts) for the claim → research → delivery
sequence and [research/agent.ts](src/research/agent.ts) for document verification
and model evaluation.

The dependency direction is explicit: `workflow` composes `audit` and
`integrations`; `audit` uses `research`; `research/openai` understands the provider
format. Research returns a validated decision and diagnostic bytes; the audit
layer constructs reports and protocol reports. Integration modules implement
the audit's effect interfaces and native CRE calls without importing workflow
handlers or deployment configuration. `shared` contains no workflow, audit or
runtime dependencies. No service locator, registry or extra package is required.

The researcher does not receive credentials or transaction tools. Agreement
text cannot expand its configured domain or tool permissions. A permitted
domain includes subdomains, matching the provider's web_search contract.
Source relevance and factual judgment remain model responsibilities; a valid
schema and authorized delivery do not prove factual correctness.

The request supplies the audit cutoff in both Unix seconds and UTC, so the agent
does not need a search service to convert it. Web research starts with permitted
primary links in the documents and counts searches, page opens and in-page
searches against the same call budget. API-backed calculator or other non-URL
sources are not permitted by a domain-only source policy.

## Configuration and trust

Copy `.env.example` to ignored `.env` in this workflow directory. Set
`TEKMERION_MODEL_KEY` to the OpenAI API key for live execution. Recorded local
tests do not need a real key. `TEKMERION_ARCHIVE_KEY` is optional and can remain
empty; the default configuration does not publish audit artifacts.

The workflow uses fixed secret identifiers: `MODEL_KEY` for OpenAI and
`ARCHIVE_KEY` for artifact uploads. `secrets.yaml` maps them to environment
variables; their names are not configurable in the workflow JSON.

To enable uploads, set `archiveUrl` to the backend's `/v1/audit-artifacts`
endpoint and enable the `ARCHIVE_KEY` mapping in `secrets.yaml`.
Supply the same `TEKMERION_ARCHIVE_KEY` to the backend. The key
must contain 32–256 printable, non-whitespace ASCII characters. For deployment,
upload configured secrets to the Vault DON; a deployed workflow cannot read the
local `.env` file. Do not enable the optional secret mapping when no archive key
is supplied.

Missing archive settings, an absent Vault secret or an empty archive key skip
uploads. Neither disables model evaluation or terminal report delivery. An
unavailable archive is also nonfatal because publication happens only after
the terminal report. A missing model key remains a technical failure, never a
fabricated verdict. Credentials and archive routing are outside the committed
agent configuration, so rotating a key does not change `agentConfigHash`.

Copy `config.example.json` to ignored `config.local.json`. Replace placeholders
and validate with `node tools/config.mjs config.local.json`. The command prints the
canonical agent configuration hash to enroll in Profile. The hash covers the
model revision, reasoning effort, allowed tools, permitted domains, document
gateways and budgets. It is not a reconstruction of an AI HTTP request.

The example pins the real `gpt-5.4-2026-03-05` snapshot and uses SEC, Nasdaq and
NYSE as example sources for US listing agreements. Review the allowed domains
against your Agreement before creating its Policy; these are not universal
sources for every event. Only the Policy address and the CRE HTTP signer's
address remain placeholders because they belong to your deployment. Archive
uploads are disabled by default. See the [model reference](https://developers.openai.com/api/docs/models/gpt-5.4)
for the supported snapshot, reasoning and web-search capabilities.

The workflow code owns endpoint, system instructions, response schema and
supported tool implementations. The configuration explicitly selects which of
those tools the agent may use. The enrolled workflow identity must be pinned to
the reviewed code and deployment configuration under the release procedure. Changing a secret
reference cannot select a different model; a secret is only a credential.

CRE performs account reads, document verification, orchestration and report
delivery. OpenAI performs inference and hosted web research outside the enclave
and receives the document text. We trust the provider's reported model/tool
execution; this is not an independent cryptographic proof of its internals.
The final report is committed by `report_hash` in the terminal message and
retained in the on-chain Request.

## Audit report

The archive stores one readable Markdown document as UTF-8 text. The report
contains the verdict, a detailed assessment of the agreement's criteria,
supporting facts, contradictions or uncertainty, and source URLs used by the
agent. The workflow adds the Case, Policy, input hashes, audit cutoff, model and
execution identifiers so the document can be matched to the on-chain result.
The full dialogue, HTTP exchanges and raw provider output are not stored.

OpenAI returns a structured decision so `answer`, `reason`, `explanation` and
`sourceIds` can be resolved against the research catalogue before publication. `explanation` is the agent's
Markdown report; the source list may contain only consulted URLs permitted by
the committed tool policy. Technical failures produce a short failure notice
with the execution identity and error code, never a guessed Yes/No/Unknown.

The workflow hashes the final document and includes `report_hash` with the
on-chain verdict. After delivery, one authenticated PUT uploads the same bytes
to `/v1/audit-artifacts/{report_hash}`. GET serves them as
`text/markdown; charset=utf-8`. Uploads are optional, and archive failures do not
change the decision. There is no separate artifact-size policy, chunk upload,
assembly manifest or second evidence document. Normal CRE and HTTP transport
limits still apply; the committed model token/response budgets remain enforced.

When web search is enabled, the workflow first collects factual findings with the
configured tools. It then makes one separate, tool-free decision request using
the same model and reasoning setting. The final request receives the committed
documents, research findings and a catalogue of URLs observed in completed tool
calls. It returns citation IDs; the workflow resolves them to URLs. Unknown IDs,
unlisted links and tool calls in the decision response are rejected. This binds
citations to observed sources, but does not prove that an AI summary is accurate.
Document-only policies skip research and make just the decision request.

All HTTP time must fit the execution window, and execution plus delivery reserve
must fit the five-minute CRE run limit. `documentTimeoutSeconds` bounds each
document fetch and optional report upload. `researchTimeoutSeconds` and
`decisionTimeoutSeconds` bound the two model stages. All three are committed in
the immutable configuration hash. The example allocates 10 seconds per document,
150 for research and 70 for the decision, within a 240-second execution window
and 45-second delivery reserve. The worst-case budget includes both documents
through every gateway; document-only policies omit the research timeout.
Individual responses are capped at 100 KB and model request bodies at 120 KB.
Each stage runs once per audit attempt. An incomplete response or timeout is a
technical failure, never a guessed verdict; retries follow the on-chain Policy.

This handler uses `HTTPClient` through `TeeRuntime`, not the separate
`ConfidentialHTTPClient` capability. These application budgets do not grant
deployment quotas. Qualify the selected confidential environment's HTTP limits
before deployment; a local run does not prove that its quota is available.
See the [confidential client reference](https://docs.chain.link/cre/reference/sdk/confidential-workflows-client-ts)
and [service quotas](https://docs.chain.link/cre/service-quotas).
Replacing the former shared timeout changes the configuration hash and requires
a new Policy/Profile; existing immutable Profiles are not migrated.

Cron queries Prepared and Retryable accounts using server-side state filters.
Finished account history therefore does not count against pending discovery.
Each scan inspects a bounded rotating set of candidates, so stale pending cases
do not monopolize every invocation.
A Case trigger addresses a specific prepared Request without a broad scan.
Preparation and Ready application remain permissionless keeper operations.

Production enrollment remains closed with `UnqualifiedRelease` until a live
confidential tenant, native Solana delivery, pinned release identity and
model/tool limits have been qualified. A local replay does not remove this gate.

The WASM build uses a pinned standalone WHATWG URL implementation because Javy
does not provide the browser URL constructor. Base64 uses the SDK-compatible
bundled Buffer implementation rather than browser globals. The same URL policy
and canonicalization apply in unit tests and in the compiled workflow.

### Explicit tool permissions

Set these fields manually in `agent` before enrolling its hash in Profile.
`allowedTools` is mandatory; an omitted list never enables tools implicitly.

| Audit mode | `allowedTools` | `maxToolCalls` | `allowedSearchDomains` |
| --- | --- | --- | --- |
| Supplied documents only | `[]` | `0` | `[]` |
| Bounded web research | `["web_search"]` | `1`–`16` | `1`–`100` reviewed domains |

`web_search` is currently the only supported agent tool. It includes the
provider's search, open-page and find actions; these are not separate permissions.
Unknown or duplicate tool names and contradictory limits are rejected. Merely
adding a tool name cannot enable shell access, MCP, custom HTTP or other tools.

Without tools, the request contains `tools: []` and `tool_choice: "none"`.
The workflow still fetches and verifies the two committed IPFS documents;
that is mandatory input loading, not an agent capability. URLs inside those
documents remain text. The prompt directs the agent to return Unknown when
external verification is required but unavailable, and `sources` must be empty.

With web research, the request includes only the allowed tool, pinned domains
and call budget. Final `sources` may contain only URLs recorded as consulted
by that tool on a permitted domain. The response parser rejects unauthorized
tools and unsubstantiated citations in either mode as technical failures.
Citation checks establish reported tool use, not the truth of a page's content.

The Agreement defines what to verify; it cannot grant tools or change these
permissions. A trigger cannot supply tool overrides. Changing tools, domains
or budgets changes `agentConfigHash`; a mismatch with the immutable Profile
stops execution before any claim, document read or AI call. Existing enrolled
Profiles cannot be updated to change these permissions.

## Validation coverage

Tests cover unrelated claim domains, multi-action research traces, content
tampering, document instruction injection boundaries, malformed responses,
explicit tool opt-in, document-only audits, unsupported tools, false citations,
tool/domain/model/config drift, impossible budgets,
execution races, terminal delivery failure, archive order, native SDK routing
and history-independent cron filtering. Contract tests independently verify
state transitions, replay protection, fee accounting and permissionless apply.
`npm run test:wasm` compiles the deployment entrypoint and executes its actual
WASM bytes with WASI and recorded CRE host replies. It checks trigger registration,
TEE requirements, HTTP input rejection, configuration validation and empty-queue
cron discovery. The full contract script also runs these artifact checks;
successful compilation alone is insufficient. Host replies are local fixtures,
not evidence of live tenant execution or enclave attestation.

References:
[Confidential CRE execution](https://docs.chain.link/cre/concepts/confidential-workflows),
[CRE quotas](https://docs.chain.link/cre/service-quotas),
[Responses tool configuration](https://developers.openai.com/api/docs/guides/tools),
[Responses web search](https://developers.openai.com/api/docs/guides/tools-web-search),
[Structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs).
