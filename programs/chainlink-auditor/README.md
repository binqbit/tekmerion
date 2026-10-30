# Chainlink auditor architecture

The adapter is an authorized Tekmerion PDA auditor. A qualified Chainlink
workflow reads the Case, verifies its committed documents and runs the audit.
The adapter authenticates the delivered report, enforces its Case/execution
binding and submits a standard Tekmerion vote. It does not render AI requests
or interpret Agreement text.

The [verification guide](../../docs/confidential-auditor.md) explains the
review safeguards, failure handling and trust boundaries. Public confidential enrollment remains
blocked until an actual deployment is qualified. Local fixtures use synthetic
DON keys and the pinned upstream Forwarder.

## Code boundaries

- `instructions/configuration/` registers an immutable Profile before Policy use.
- `instructions/audit/` prepares a Request, receives authenticated reports and
  applies a retained verdict. `on_report` has six explicit accounts; Apply has
  nine, including the single auditor fee destination. Neither accepts extra accounts.
- `state/profile.rs` owns immutable identity/configuration commitments.
  `state/request.rs` owns one Case's current execution and retained outcome;
  `state/request/receive.rs` implements its transitions.
- `confidential/binding.rs` binds a Request to the actual Case, Policy, Profile
  and network. `confidential/payload.rs` strictly decodes version 3 reports.
- `confidential/release.rs` qualifies enrollment. The production build rejects
  unqualified deployments; a separate fixture feature accepts synthetic identities.
- `report.rs` authenticates Forwarder state, its scoped signer and the workflow
  metadata. `payload.rs` validates the generic Tekmerion decision vocabulary.

## Why two accounts

The **Profile** commits one Policy's authorized workflow, Forwarder, configuration
hash and retry limit. It has no update instruction. The actual agent configuration
belongs to the workflow; its canonical JSON hash must match this commitment.

The **Request** binds one Case and retains its current execution identifier,
attempt counter, terminal message hash and verdict. A later attempt can start
only after an authenticated retryable technical failure. Old indexes cannot
become current again. Separate permanent Attempt accounts and specification
upload accounts are unnecessary.

Anyone can prepare the canonical Request or apply a Ready verdict. Only an
authenticated Forwarder report may claim execution or record its outcome.
Apply uses the Profile PDA to sign Tekmerion's existing vote instruction. Vote,
fee transfers and Applied state are atomic. A failed CPI leaves Ready intact;
a duplicate Apply never charges a second fee. Apply must still occur before
Tekmerion's original deadline.

## Verification

From the Tekmerion root, run `bash scripts/test.sh`. Tests use real local SBF
programs and the pinned Forwarder, verify both network identities and maintained
IDL/SDK fixtures, and replay the recorded workflow's report. The complete
suite also builds the workflow WASM. Local replay is not live TEE qualification.
