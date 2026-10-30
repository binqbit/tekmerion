/** A standalone wallet auditor. No platform backend or provider integration. */
import {
  createClient,
  type AuditDecision,
  type CaseAddress,
  type CaseView,
  type Network,
} from '@tekmerion/sdk';
import { Transaction, type PublicKey } from '@solana/web3.js';

export interface DirectAuditSetup {
  network: Network;
  policyIdentifier: bigint;
  payer: PublicKey;
  administrator: PublicKey;
  clientAuthority: PublicKey;
  author: PublicKey;
  authorTokenAccount: PublicKey;
  auditor: PublicKey;
  feeRecipient: PublicKey;
  subject: PublicKey;
  agreementHash: Uint8Array;
  resultHash: Uint8Array;
  auditFee: bigint;
  auditTimeout: bigint;
}

/** Create a new Policy and its first direct Case together. The signers approve
 * the full rules in this transaction; no previously mutable Policy is trusted.
 * Token accounts and the subject are supplied by the consuming application. */
export function createDirectAudit(input: DirectAuditSetup): {
  identity: CaseAddress;
  transaction: Transaction;
} {
  const client = createClient(input.network);
  const identity: CaseAddress = {
    policy: client.policyAddress(input.administrator, input.policyIdentifier)[0],
    subject: input.subject,
    author: input.author,
    nonce: 0n,
  };
  const transaction = new Transaction().add(
    client.createPolicy({
      payer: input.payer,
      admin: input.administrator,
      clientAuthority: input.clientAuthority,
      identifier: input.policyIdentifier,
      rules: {
        auditors: [{ authority: input.auditor, feeRecipient: input.feeRecipient }],
        threshold: 1,
        authorBond: input.auditFee > 0n ? input.auditFee : 1n,
        challengeTarget: input.auditFee > 0n ? input.auditFee : 1n,
        auditFee: input.auditFee,
        fundingDuration: 60n,
        auditTimeout: input.auditTimeout,
        auditTrigger: 'onCreation',
      },
    }),
    client.createCase(identity, {
      payer: input.payer,
      clientAuthority: input.clientAuthority,
      author: input.author,
      source: input.authorTokenAccount,
      funding: { kind: 'direct', expectedFee: input.auditFee },
      agreementHash: input.agreementHash,
      resultHash: input.resultHash,
    }),
  );
  transaction.feePayer = input.payer;
  return { identity, transaction };
}

/** The auditor reaches its decision independently, then submits this standard
 * protocol vote. A deciding vote pays its authorized quorum in the same call. */
export function buildAuditVote(
  network: Network,
  view: CaseView,
  auditor: PublicKey,
  decision: AuditDecision,
  feeTokenAccounts: ReadonlyMap<string, PublicKey>,
): Transaction {
  const destinations = view.quoteAudit(auditor, decision.answer, decision.reason).map((payout) => {
    const account = feeTokenAccounts.get(payout.feeRecipient.toBase58());
    if (!account)
      throw new Error(`Missing fee token account for ${payout.feeRecipient.toBase58()}`);
    return account;
  });
  const transaction = new Transaction().add(
    createClient(network).submitAudit(
      view.address,
      auditor,
      decision.answer,
      decision.reason,
      destinations,
    ),
  );
  transaction.feePayer = auditor;
  return transaction;
}
