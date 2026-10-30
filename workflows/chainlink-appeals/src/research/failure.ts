/** Stable wire codes shared with the adapter's retry policy. */
export const FailureCode = {
  ProviderRateLimited: 1,
  ProviderUnavailable: 2,
  ModelTransportFailed: 3,
  SourceUnavailable: 4,
  ModelCredentialsRejected: 64,
  InvalidDocument: 65,
  ModelRevisionMismatch: 66,
  InvalidModelOutput: 67,
  BudgetExceeded: 68,
  SourcePolicyViolation: 69,
  SourceRejected: 70,
} as const;

export class AuditFailure extends Error {
  constructor(readonly code: number) {
    super(`Confidential audit technical failure ${code}`);
  }
}
