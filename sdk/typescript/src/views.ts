import { PublicKey } from '@solana/web3.js';
import {
  caseAddress,
  checkAccount,
  checkObservation,
  copy,
  integer,
  policyAddress,
  ProtocolCodec,
  receiptAddress,
  TOKEN_PROGRAM_ID,
  U64_MAX,
  vaultAddress,
} from './codec.js';
import {
  SdkError,
  type AccountData,
  type AuditAnswer,
  type AuditDecision,
  type AuditPayout,
  type AuditReason,
  type Case,
  type CaseAccounts,
  type CaseAddress,
  type ClaimState,
  type CompletionMethod,
  type Observation,
  type Policy,
  type PolicyRules,
  type StakeReceipt,
} from './types.js';

const same = (left: PublicKey | null, right: PublicKey | null): boolean =>
  left === null ? right === null : right !== null && left.equals(right);
const decisionEquals = (
  left: AuditDecision | null | undefined,
  right: AuditDecision | null,
): boolean =>
  left == null
    ? right === null
    : right !== null && left.answer === right.answer && left.reason === right.reason;
function requireState(condition: boolean): void {
  if (!condition) throw new SdkError('InvalidState');
}
function requireRelationship(condition: boolean): void {
  if (!condition) throw new SdkError('InvalidRelationship');
}
export function acceptsReason(answer: AuditAnswer, reason: AuditReason): boolean {
  return answer === 'unknown'
    ? [
        'missingEvidence',
        'staleEvidence',
        'conflictingEvidence',
        'unsupportedEvidence',
        'inconclusive',
      ].includes(reason)
    : (answer === 'yes' || answer === 'no') && reason === 'decisive';
}
export function validPolicyRules(rules: PolicyRules): boolean {
  const count = rules.auditors.length;
  if (
    count < 1 ||
    count > 9 ||
    !Number.isInteger(rules.threshold) ||
    rules.threshold <= Math.floor(count / 2) ||
    rules.threshold > count
  )
    return false;
  const authorities = new Set<string>();
  const recipients = new Set<string>();
  for (const auditor of rules.auditors) {
    const authority = auditor.authority.toBase58();
    const recipient = auditor.feeRecipient.toBase58();
    if (
      auditor.authority.equals(PublicKey.default) ||
      auditor.feeRecipient.equals(PublicKey.default) ||
      authorities.has(authority) ||
      recipients.has(recipient)
    )
      return false;
    authorities.add(authority);
    recipients.add(recipient);
  }
  for (const value of [
    rules.authorBond,
    rules.challengeTarget,
    rules.auditFee,
    rules.fundingDuration,
    rules.auditTimeout,
  ]) {
    if (typeof value !== 'bigint' || value < 0n || value > U64_MAX) return false;
  }
  return (
    rules.authorBond > 0n &&
    rules.challengeTarget > 0n &&
    rules.fundingDuration > 0n &&
    rules.auditTimeout > 0n &&
    rules.auditFee <= rules.authorBond &&
    rules.auditFee <= rules.challengeTarget
  );
}
export function completionMethod(state: Case): CompletionMethod | null {
  if (state.phase !== 'finalized' || state.challengeTarget === 0n || state.finalizedAt === null)
    return null;
  const { deadline, response, votes } = state.audit;
  if (deadline === null && response === null && state.result === 'yes') {
    return state.challengePrincipal < state.challengeTarget &&
      votes.length === 0 &&
      state.finalizedAt >= state.fundingDeadline
      ? 'optimistic'
      : null;
  }
  if (deadline !== null && response !== null && state.result !== null) {
    return state.challengePrincipal === state.challengeTarget &&
      state.finalizedAt < deadline &&
      response.answer === state.result
      ? 'audited'
      : null;
  }
  if (deadline !== null && response === null && state.result === null) {
    return state.challengePrincipal === state.challengeTarget && state.finalizedAt >= deadline
      ? 'technicalFailure'
      : null;
  }
  return null;
}
function allocation(state: Case, fee: bigint): [bigint, bigint, bigint] {
  const author = state.authorPrincipal;
  const target = state.challengeTarget;
  if (target === 0n || fee > author || fee > target) throw new SdkError('Arithmetic');
  switch (state.result) {
    case 'yes': {
      const amount = author + target - fee;
      if (amount > U64_MAX) throw new SdkError('Arithmetic');
      return [amount, 0n, fee];
    }
    case 'no':
      return [0n, target + author - fee, fee];
    case 'unknown':
      return [author - fee, target, fee];
    default:
      throw new SdkError('InvalidState');
  }
}
function lifecycle(
  codec: ProtocolCodec,
  state: Case,
  policy: Policy,
  authorClaimed: boolean,
): bigint {
  const rules = policy.rules;
  requireState(validPolicyRules(rules) && state.audit.votes.length <= rules.auditors.length);
  const seen = new Set<number>();
  const counts = { yes: 0, no: 0, unknown: 0 };
  for (const vote of state.audit.votes) {
    requireState(
      vote.auditorIndex < rules.auditors.length &&
        !seen.has(vote.auditorIndex) &&
        acceptsReason(vote.response.answer, vote.response.reason),
    );
    seen.add(vote.auditorIndex);
    counts[vote.response.answer]++;
  }
  const completion = completionMethod(state);
  if (state.audit.response !== null) {
    requireState(
      state.phase === 'finalized' &&
        completion === 'audited' &&
        decisionEquals(state.audit.votes.at(-1)?.response, state.audit.response) &&
        counts[state.audit.response.answer] === rules.threshold,
    );
  } else {
    requireState(Object.values(counts).every((count) => count < rules.threshold));
  }
  const previous =
    state.nonce === 0n
      ? null
      : caseAddress(
          { policy: state.policy, subject: state.subject, nonce: state.nonce - 1n },
          codec.programId,
        )[0];
  requireState(
    state.agreementHash.some((byte) => byte !== 0) &&
      state.resultHash.some((byte) => byte !== 0) &&
      same(state.previousCase, previous) &&
      policy.activeUses !== 0n &&
      state.authorPrincipal >= rules.authorBond &&
      state.challengeTarget === rules.challengeTarget &&
      state.challengePrincipal <= state.challengeTarget &&
      state.authorPrincipal + state.challengeTarget <= U64_MAX &&
      state.unclaimedContributors <= state.challengePrincipal,
  );
  const funded = state.challengePrincipal === state.challengeTarget;
  if (state.phase !== 'finalized') {
    requireState(
      state.phase === 'funding'
        ? !funded &&
            state.audit.deadline === null &&
            state.audit.response === null &&
            state.audit.votes.length === 0
        : funded && state.audit.deadline !== null && state.audit.response === null,
    );
    requireState(
      state.result === null &&
        state.finalizedAt === null &&
        state.authorEntitlement === 0n &&
        state.contributorEntitlement === 0n &&
        state.feeEntitlement === 0n &&
        state.contributorUnpaid === 0n &&
        !authorClaimed &&
        !state.feeClaimed &&
        (state.challengePrincipal === 0n) === (state.unclaimedContributors === 0n),
    );
    return state.authorPrincipal + state.challengePrincipal;
  }
  requireState(state.finalizedAt !== null);
  let expected: [bigint, bigint, bigint];
  if (completion === 'optimistic' && state.result === 'yes') {
    requireState(
      !funded &&
        state.audit.deadline === null &&
        state.audit.response === null &&
        state.audit.votes.length === 0 &&
        !state.feeClaimed,
    );
    expected = [state.authorPrincipal, state.challengePrincipal, 0n];
  } else if (completion === 'technicalFailure' && state.result === null) {
    requireState(
      funded && state.audit.deadline !== null && state.audit.response === null && !state.feeClaimed,
    );
    expected = [state.authorPrincipal, state.challengePrincipal, 0n];
  } else if (completion === 'audited' && state.result !== null) {
    requireState(
      funded &&
        state.audit.deadline !== null &&
        state.feeClaimed &&
        state.audit.response?.answer === state.result,
    );
    expected = allocation(state, rules.auditFee);
  } else throw new SdkError('InvalidState');
  requireState(
    state.authorEntitlement === expected[0] &&
      state.contributorEntitlement === expected[1] &&
      state.feeEntitlement === expected[2] &&
      state.contributorUnpaid <= state.contributorEntitlement &&
      (state.unclaimedContributors !== 0n || state.contributorUnpaid === 0n) &&
      (state.contributorEntitlement === 0n ||
        state.contributorUnpaid >= state.unclaimedContributors),
  );
  return (
    (authorClaimed ? 0n : state.authorEntitlement) +
    state.contributorUnpaid +
    (state.feeClaimed ? 0n : state.feeEntitlement)
  );
}

export class PolicyView {
  readonly #policy: Policy;
  readonly #observation: Observation;
  readonly #address: PublicKey;
  private constructor(policy: Policy, observation: Observation, address: PublicKey) {
    this.#policy = copy(policy);
    this.#observation = copy(observation);
    this.#address = copy(address);
  }
  static read(
    codec: ProtocolCodec,
    observation: Observation,
    expected: PublicKey,
    account: AccountData,
  ): PolicyView {
    checkObservation(observation);
    const policy = codec.decode<Policy>('Policy', account, observation);
    const [address, bump] = policyAddress(policy.admin, policy.identifier, codec.programId);
    requireRelationship(
      account.key.equals(expected) && address.equals(expected) && policy.bump === bump,
    );
    requireState(validPolicyRules(policy.rules));
    return new PolicyView(policy, observation, expected);
  }
  get policy(): Policy {
    return copy(this.#policy);
  }
  get observation(): Observation {
    return copy(this.#observation);
  }
  get address(): PublicKey {
    return copy(this.#address);
  }
}

export class CaseView {
  readonly #codec: ProtocolCodec;
  readonly #observation: Observation;
  readonly #address: CaseAddress;
  readonly #case: Case;
  readonly #policy: Policy;
  readonly #authorReceipt: StakeReceipt;
  readonly #vaultBalance: bigint;
  private constructor(
    codec: ProtocolCodec,
    observation: Observation,
    address: CaseAddress,
    state: Case,
    policy: Policy,
    authorReceipt: StakeReceipt,
    vaultBalance: bigint,
  ) {
    this.#codec = codec;
    this.#observation = copy(observation);
    this.#address = copy(address);
    this.#case = copy(state);
    this.#policy = copy(policy);
    this.#authorReceipt = copy(authorReceipt);
    this.#vaultBalance = vaultBalance;
  }
  static read(
    codec: ProtocolCodec,
    observation: Observation,
    expected: CaseAddress,
    accounts: CaseAccounts,
  ): CaseView {
    checkObservation(observation);
    const state = codec.decode<Case>('Case', accounts.case, observation);
    const policy = PolicyView.read(codec, observation, expected.policy, accounts.policy).policy;
    const receipt = codec.decode<StakeReceipt>('StakeReceipt', accounts.authorReceipt, observation);
    const [address, bump] = caseAddress(expected, codec.programId);
    const [vault, vaultBump] = vaultAddress(address, codec.programId);
    const [authorReceipt, receiptBump] = receiptAddress(address, state.author, codec.programId);
    requireRelationship(
      accounts.case.key.equals(address) &&
        state.policy.equals(expected.policy) &&
        state.subject.equals(expected.subject) &&
        state.nonce === expected.nonce &&
        state.bump === bump &&
        state.vaultBump === vaultBump &&
        accounts.vault.key.equals(vault) &&
        accounts.authorReceipt.key.equals(authorReceipt) &&
        receipt.case.equals(address) &&
        receipt.owner.equals(state.author) &&
        receipt.bump === receiptBump,
    );
    requireState(
      receipt.side === 0 && receipt.principal === state.authorPrincipal && receipt.principal > 0n,
    );
    // The canonical Case binds this opaque identity even after the subject closes.
    if (accounts.subject !== undefined) {
      requireRelationship(accounts.subject.key.equals(expected.subject));
      checkAccount(accounts.subject, observation);
    }
    checkAccount(accounts.vault, observation, TOKEN_PROGRAM_ID);
    const bytes = Buffer.from(accounts.vault.data);
    if (bytes.length !== 165) throw new SdkError('InvalidAccount');
    // Classic SPL Token account layout: this is token custody, not Anchor state.
    if (
      !new PublicKey(bytes.subarray(0, 32)).equals(codec.mint) ||
      !new PublicKey(bytes.subarray(32, 64)).equals(address) ||
      bytes[108] !== 1 ||
      bytes.readUInt32LE(72) !== 0 ||
      bytes.readUInt32LE(109) !== 0 ||
      bytes.readUInt32LE(129) !== 0
    )
      throw new SdkError('InvalidAccount');
    const balance = bytes.readBigUInt64LE(64);
    const liability = lifecycle(codec, state, policy, receipt.claimed);
    if (balance < liability) throw new SdkError('InsufficientBacking');
    return new CaseView(codec, observation, expected, state, policy, receipt, balance);
  }
  get case(): Case {
    return copy(this.#case);
  }
  get policy(): Policy {
    return copy(this.#policy);
  }
  get authorReceipt(): StakeReceipt {
    return copy(this.#authorReceipt);
  }
  get observation(): Observation {
    return copy(this.#observation);
  }
  get address(): CaseAddress {
    return copy(this.#address);
  }
  get vaultBalance(): bigint {
    return this.#vaultBalance;
  }
  get completionMethod(): CompletionMethod | null {
    return completionMethod(this.#case);
  }
  authorClaim(): ClaimState {
    return this.#case.phase !== 'finalized'
      ? { kind: 'unavailable' }
      : this.#authorReceipt.claimed
        ? { kind: 'claimed' }
        : { kind: 'unclaimed', amount: this.#case.authorEntitlement };
  }
  auditFeesPaid(): bigint | null {
    return this.completionMethod === 'audited' ? this.#case.feeEntitlement : null;
  }
  quoteContribution(requested: bigint): bigint {
    integer(requested);
    const state = this.#case;
    if (
      requested === 0n ||
      state.phase !== 'funding' ||
      this.#observation.unixTimestamp >= state.fundingDeadline ||
      state.challengePrincipal >= state.challengeTarget
    )
      throw new SdkError('ActionUnavailable');
    const remaining = state.challengeTarget - state.challengePrincipal;
    return requested < remaining ? requested : remaining;
  }
  quoteAudit(auditor: PublicKey, answer: AuditAnswer, reason: AuditReason): readonly AuditPayout[] {
    const state = this.#case;
    const rules = this.#policy.rules;
    const index = rules.auditors.findIndex((member) => member.authority.equals(auditor));
    if (
      state.phase !== 'funded' ||
      state.audit.deadline === null ||
      this.#observation.unixTimestamp >= state.audit.deadline ||
      index < 0 ||
      state.audit.votes.some((vote) => vote.auditorIndex === index) ||
      !acceptsReason(answer, reason)
    )
      throw new SdkError('ActionUnavailable');
    const winners = [
      ...state.audit.votes
        .filter((vote) => vote.response.answer === answer)
        .map((vote) => vote.auditorIndex),
      index,
    ].sort((left, right) => left - right);
    if (winners.length < rules.threshold) return [];
    const count = BigInt(winners.length);
    return winners.map((auditorIndex, position) => ({
      auditorIndex,
      feeRecipient: copy(rules.auditors[auditorIndex]!.feeRecipient),
      amount: rules.auditFee / count + (BigInt(position) < rules.auditFee % count ? 1n : 0n),
    }));
  }
  contributorClaim(owner: PublicKey, account: AccountData): ClaimState {
    const state = this.#case;
    const receipt = this.#codec.decode<StakeReceipt>('StakeReceipt', account, this.#observation);
    const address = caseAddress(this.#address, this.#codec.programId)[0];
    const [key, bump] = receiptAddress(address, owner, this.#codec.programId);
    requireRelationship(
      account.key.equals(key) &&
        receipt.case.equals(address) &&
        receipt.owner.equals(owner) &&
        receipt.bump === bump,
    );
    requireState(
      !owner.equals(state.author) &&
        receipt.side === 1 &&
        receipt.principal > 0n &&
        receipt.principal <= state.challengePrincipal,
    );
    if (state.phase !== 'finalized') {
      requireState(!receipt.claimed);
      return { kind: 'unavailable' };
    }
    if (receipt.claimed) {
      requireState(state.unclaimedContributors <= state.challengePrincipal - receipt.principal);
      return { kind: 'claimed' };
    }
    requireState(
      state.unclaimedContributors > 0n &&
        state.unclaimedContributors - 1n <= state.challengePrincipal - receipt.principal,
    );
    let amount = receipt.principal;
    if (this.completionMethod === 'audited') {
      if (state.result === 'yes') amount = 0n;
      else if (state.result === 'no') {
        const rewardPool = state.authorPrincipal - state.feeEntitlement;
        if (rewardPool < 0n || state.contributorUnpaid > U64_MAX) throw new SdkError('Arithmetic');
        const ordinary =
          receipt.principal + (rewardPool * receipt.principal) / state.challengeTarget;
        const last = state.unclaimedContributors === 1n;
        const maximum = (last ? receipt.principal : state.challengeTarget) + rewardPool;
        if (
          state.contributorUnpaid < ordinary ||
          state.contributorUnpaid > maximum ||
          (!last && state.contributorUnpaid === ordinary)
        )
          throw new SdkError('Arithmetic');
        amount = last ? state.contributorUnpaid : ordinary;
        if (amount > U64_MAX) throw new SdkError('Arithmetic');
      }
    }
    requireState(
      amount <= state.contributorUnpaid &&
        (state.unclaimedContributors !== 1n || amount === state.contributorUnpaid) &&
        (state.contributorEntitlement === 0n ||
          (amount >= receipt.principal &&
            state.contributorUnpaid - amount >= state.unclaimedContributors - 1n)),
    );
    return { kind: 'unclaimed', amount };
  }
}
