import type { PublicKey } from '@solana/web3.js';

export type Network = 'mainnet' | 'devnet';
export type AuditAnswer = 'yes' | 'no' | 'unknown';
export type AuditReason =
  | 'decisive'
  | 'missingEvidence'
  | 'staleEvidence'
  | 'conflictingEvidence'
  | 'unsupportedEvidence'
  | 'inconclusive';
export type CasePhase = 'funding' | 'funded' | 'finalized';
export type CompletionMethod = 'optimistic' | 'audited' | 'technicalFailure';
export interface AuditDecision {
  readonly answer: AuditAnswer;
  readonly reason: AuditReason;
}
export interface AuditVote {
  readonly auditorIndex: number;
  readonly response: AuditDecision;
}
export interface AuditorConfig {
  readonly authority: PublicKey;
  readonly feeRecipient: PublicKey;
}
export interface PolicyRules {
  readonly auditors: readonly AuditorConfig[];
  readonly threshold: number;
  readonly authorBond: bigint;
  readonly challengeTarget: bigint;
  readonly auditFee: bigint;
  readonly fundingDuration: bigint;
  readonly auditTimeout: bigint;
}
export interface Policy {
  readonly admin: PublicKey;
  readonly identifier: bigint;
  readonly clientAuthority: PublicKey;
  readonly rules: PolicyRules;
  readonly activeUses: bigint;
  readonly bump: number;
}
export interface CaseAddress {
  readonly policy: PublicKey;
  readonly subject: PublicKey;
  readonly author: PublicKey;
  readonly nonce: bigint;
}
export interface Case extends CaseAddress {
  readonly previousCase: PublicKey | null;
  readonly agreementHash: readonly number[];
  readonly resultHash: readonly number[];
  readonly authorPrincipal: bigint;
  readonly challengeTarget: bigint;
  readonly challengePrincipal: bigint;
  readonly fundingDeadline: bigint;
  readonly audit: {
    readonly deadline: bigint | null;
    readonly votes: readonly AuditVote[];
    readonly response: AuditDecision | null;
  };
  readonly phase: CasePhase;
  readonly result: AuditAnswer | null;
  readonly finalizedAt: bigint | null;
  readonly authorEntitlement: bigint;
  readonly contributorEntitlement: bigint;
  readonly feeEntitlement: bigint;
  readonly contributorUnpaid: bigint;
  readonly unclaimedContributors: bigint;
  readonly feeClaimed: boolean;
  readonly bump: number;
  readonly vaultBump: number;
}
export interface StakeReceipt {
  readonly case: PublicKey;
  readonly owner: PublicKey;
  readonly side: number;
  readonly principal: bigint;
  readonly claimed: boolean;
  readonly bump: number;
}
export interface Observation {
  readonly slot: bigint;
  readonly unixTimestamp: bigint;
}
export interface AccountData {
  readonly key: PublicKey;
  readonly owner: PublicKey;
  readonly executable: boolean;
  readonly data: Uint8Array;
  readonly slot: bigint;
}
export interface CaseAccounts {
  readonly case: AccountData;
  readonly policy: AccountData;
  /** Optional opaque application account; when supplied, identity, slot and executable status are checked. */
  readonly subject?: AccountData;
  readonly vault: AccountData;
  readonly authorReceipt: AccountData;
}
export type ClaimState =
  | { readonly kind: 'unavailable' }
  | { readonly kind: 'claimed' }
  | { readonly kind: 'unclaimed'; readonly amount: bigint };
export interface AuditPayout {
  readonly auditorIndex: number;
  readonly feeRecipient: PublicKey;
  readonly amount: bigint;
}
export type ErrorCode =
  | 'InvalidArgument'
  | 'InconsistentObservation'
  | 'InvalidAccount'
  | 'InvalidRelationship'
  | 'InvalidState'
  | 'InsufficientBacking'
  | 'ActionUnavailable'
  | 'Arithmetic';
export class SdkError extends Error {
  constructor(
    readonly code: ErrorCode,
    message: string = code,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = 'SdkError';
  }
}
