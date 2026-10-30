/** Authenticate the immutable Policy/Case relationship and the adapter's execution state. */
import {
  CORE_ID,
  address,
  check,
  concat,
  decodeCaseIdentity,
  hash,
  hex,
  integerBytes,
  pda,
  unhex,
  utf8,
  type CaseIdentity,
} from '@tekmerion/sdk/wire';
import {
  ADAPTER_IDS,
  networkDomain,
  profileAddress,
  requestAddress,
  type ChainlinkNetwork,
} from './addresses.js';
import { BoundedReader, u32 } from './binary.js';
import { decodeConfidentialProfile, decodeConfidentialRequest } from './accounts.js';
import { retryableFailure } from './report.js';

/** The workflow uses these committed values to retrieve documents and enforce the audit cutoff. */
export interface AuditContext {
  networkDomain: string;
  coreProgram: string;
  adapterProgram: string;
  profile: string;
  policy: string;
  case: string;
  subject: string;
  caseNonce: bigint;
  agreementHash: string;
  resultHash: string;
  auditDeadline: bigint;
  cutoff: bigint;
}

export interface BindingAccounts {
  network: ChainlinkNetwork;
  profileKey: string;
  profileBytes: Uint8Array;
  policyBytes: Uint8Array;
  caseKey: string;
  caseBytes: Uint8Array;
  requestKey: string;
  requestBytes: Uint8Array;
}

export function requestDigest(
  profileKey: string,
  profileBytes: Uint8Array,
  caseKey: string,
  caseIdentity: CaseIdentity,
  network: ChainlinkNetwork,
): string {
  const profile = decodeConfidentialProfile(profileBytes);
  check(profile.policy === caseIdentity.policy, 'Invalid domain');
  return hex(
    hash(
      concat(
        utf8('tekmerion:chainlink:request:v3'),
        networkDomain(network),
        address(CORE_ID),
        address(ADAPTER_IDS[network]),
        address(profileKey),
        u32(profileBytes.length - 8),
        profileBytes.slice(8),
        address(caseIdentity.policy),
        address(caseKey),
        address(caseIdentity.subject),
        integerBytes(caseIdentity.nonce),
        unhex(caseIdentity.agreementHash),
        unhex(caseIdentity.resultHash),
        integerBytes(caseIdentity.deadline, true),
      ),
    ),
  );
}

/** Owners and finality must be checked by the account transport before this pure decoder. */
export function validateConfidentialBinding(binding: BindingAccounts) {
  const profile = decodeConfidentialProfile(binding.profileBytes);
  const request = decodeConfidentialRequest(binding.requestBytes);
  const caseIdentity = decodeCaseIdentity(binding.caseBytes);
  const [profileKey, profileBump] = profileAddress(caseIdentity.policy, binding.network);
  const [requestKey, requestBump] = requestAddress(profileKey, binding.caseKey, binding.network);
  check(
    binding.profileKey === profileKey &&
      profile.bump === profileBump &&
      binding.requestKey === requestKey &&
      request.bump === requestBump &&
      profile.policy === caseIdentity.policy &&
      request.profile === profileKey &&
      request.case === binding.caseKey,
    'Invalid PDA binding',
  );
  check(
    pda(
      [
        utf8('case'),
        address(caseIdentity.policy),
        address(caseIdentity.subject),
        address(caseIdentity.author),
        integerBytes(caseIdentity.nonce),
      ],
      CORE_ID,
    )[0] === binding.caseKey,
    'Invalid Case',
  );
  const auditTimeout = validatePolicy(
    binding.policyBytes,
    caseIdentity.policy,
    profileKey,
    profile.rulesHash,
    caseIdentity.mode,
  );
  check(
    request.digest ===
      requestDigest(
        profileKey,
        binding.profileBytes,
        binding.caseKey,
        caseIdentity,
        binding.network,
      ),
    'Request digest mismatch',
  );
  check(request.attemptsStarted <= profile.config.maxAttempts, 'Attempt limit');
  if (request.state === 'retryable' || request.state === 'stopped') {
    const canRetry =
      retryableFailure(request.failureCode) && request.attemptsStarted < profile.config.maxAttempts;
    check((request.state === 'retryable') === canRetry, 'Invalid retry state');
  }
  if (request.state !== 'prepared') {
    check(request.claimedAt < caseIdentity.deadline, 'Late execution claim');
    if (request.state !== 'active')
      check(request.terminalAt < caseIdentity.deadline, 'Late terminal report');
  }
  if (request.state === 'applied') {
    const vote = caseIdentity.auditVotes[0];
    check(
      caseIdentity.phase === 2 &&
        caseIdentity.result === request.answer &&
        caseIdentity.finalizedAt === request.appliedAt &&
        request.appliedAt < caseIdentity.deadline &&
        caseIdentity.auditResponse?.answer === request.answer &&
        caseIdentity.auditResponse.reason === request.reason &&
        caseIdentity.challengeTarget === caseIdentity.challengePrincipal &&
        caseIdentity.auditVotes.length === 1 &&
        vote?.auditorIndex === 0 &&
        vote.answer === request.answer &&
        vote.reason === request.reason,
      'Applied request does not match the finalized Case',
    );
  }
  const context: AuditContext = {
    networkDomain: hex(networkDomain(binding.network)),
    coreProgram: CORE_ID,
    adapterProgram: ADAPTER_IDS[binding.network],
    profile: profileKey,
    policy: caseIdentity.policy,
    case: binding.caseKey,
    subject: caseIdentity.subject,
    caseNonce: caseIdentity.nonce,
    agreementHash: caseIdentity.agreementHash,
    resultHash: caseIdentity.resultHash,
    auditDeadline: caseIdentity.deadline,
    cutoff: caseIdentity.deadline - auditTimeout,
  };
  integerBytes(context.cutoff, true);
  return { profile, request, case: caseIdentity, context };
}

function validatePolicy(
  bytes: Uint8Array,
  policyKey: string,
  profileKey: string,
  rulesHash: string,
  caseMode: 'challenge' | 'direct',
): bigint {
  const reader = new BoundedReader(bytes);
  reader.account('Policy', 711);
  const admin = reader.key();
  const identifier = reader.u64();
  const client = reader.take(32);
  const start = reader.offset;
  check(reader.u32() === 1 && reader.key() === profileKey, 'Invalid auditor');
  reader.key(); // The fee recipient is part of the immutable rules commitment.
  check(reader.u8() === 1, 'Invalid quorum');
  reader.take(32); // Bond, challenge target, audit fee, funding duration.
  const auditTimeout = reader.u64();
  const auditTrigger = reader.u8();
  check(auditTrigger <= 1, 'Invalid audit trigger');
  check(auditTrigger === 0 || caseMode === 'direct', 'Policy requires a direct audit');
  const rules = bytes.slice(start, reader.offset);
  check(
    pda([utf8('policy'), address(admin), integerBytes(identifier)], CORE_ID)[0] === policyKey &&
      rulesHash === hex(hash(concat(utf8('tekmerion:chainlink:policy:v1'), client, rules))),
    'Policy changed',
  );
  return auditTimeout;
}
