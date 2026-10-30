/** Native v3 account layouts. Execution state is held in one Request per Case. */
import { address, check, concat, hex, unhex, validateDecision } from '@tekmerion/sdk/wire';
import { BoundedReader, u16 } from './binary.js';

export const PROFILE_ACCOUNT_BYTES = 204;
export const REQUEST_ACCOUNT_BYTES = 232;

export interface ConfidentialConfig {
  forwarderProgram: string;
  forwarderState: string;
  workflowId: string;
  agentConfigHash: string;
  maxAttempts: number;
}

export interface ConfidentialProfile {
  version: 3;
  bump: number;
  policy: string;
  rulesHash: string;
  config: ConfidentialConfig;
}

export function encodeConfig(config: ConfidentialConfig): Uint8Array {
  check(
    Object.keys(config).sort().join(',') ===
      'agentConfigHash,forwarderProgram,forwarderState,maxAttempts,workflowId',
    'Unexpected configuration fields',
  );
  const defaultKey = '11111111111111111111111111111111';
  check(
    config.forwarderProgram !== defaultKey && config.forwarderState !== defaultKey,
    'Empty Forwarder identity',
  );
  check(config.maxAttempts > 0, 'At least one execution attempt is required');
  for (const value of [config.workflowId, config.agentConfigHash])
    check(value !== '0'.repeat(64), 'Empty configuration commitment');
  return concat(
    address(config.forwarderProgram),
    address(config.forwarderState),
    unhex(config.workflowId),
    unhex(config.agentConfigHash),
    u16(config.maxAttempts),
  );
}

export function decodeConfidentialProfile(bytes: Uint8Array): ConfidentialProfile {
  const reader = new BoundedReader(bytes);
  reader.account('ConfidentialProfile', PROFILE_ACCOUNT_BYTES);
  const version = reader.u8();
  check(version === 3, 'Unsupported profile');
  const profile: ConfidentialProfile = {
    version,
    bump: reader.u8(),
    policy: reader.key(),
    rulesHash: hex(reader.take(32)),
    config: {
      forwarderProgram: reader.key(),
      forwarderState: reader.key(),
      workflowId: hex(reader.take(32)),
      agentConfigHash: hex(reader.take(32)),
      maxAttempts: reader.u16(),
    },
  };
  reader.end();
  encodeConfig(profile.config);
  return profile;
}

export type RequestState = 'prepared' | 'active' | 'retryable' | 'ready' | 'applied' | 'stopped';

export interface ConfidentialRequest {
  version: 3;
  bump: number;
  profile: string;
  case: string;
  digest: string;
  state: RequestState;
  attemptsStarted: number;
  executionId: string;
  claimedAt: bigint;
  terminalAt: bigint;
  terminalPayloadHash: string;
  failureCode: number;
  reportHash: string;
  answer: number;
  reason: number;
  appliedAt: bigint;
}

export function decodeConfidentialRequest(bytes: Uint8Array): ConfidentialRequest {
  const reader = new BoundedReader(bytes);
  reader.account('ConfidentialRequest', REQUEST_ACCOUNT_BYTES);
  const version = reader.u8();
  check(version === 3, 'Unsupported request');
  const bump = reader.u8();
  const profile = reader.key();
  const caseKey = reader.key();
  const digest = hex(reader.take(32));
  const states = ['prepared', 'active', 'retryable', 'ready', 'applied', 'stopped'] as const;
  const state = states[reader.u8()];
  check(state !== undefined, 'Invalid request state');
  const request: ConfidentialRequest = {
    version,
    bump,
    profile,
    case: caseKey,
    digest,
    state,
    attemptsStarted: reader.u16(),
    executionId: hex(reader.take(32)),
    claimedAt: reader.i64(),
    terminalAt: reader.i64(),
    terminalPayloadHash: hex(reader.take(32)),
    failureCode: reader.u8(),
    reportHash: hex(reader.take(32)),
    answer: reader.u8(),
    reason: reader.u8(),
    appliedAt: reader.i64(),
  };
  reader.end();
  validateRequestState(request);
  return request;
}

function validateRequestState(request: ConfidentialRequest): void {
  const zero = '0'.repeat(64);
  const isVerdict = request.state === 'ready' || request.state === 'applied';
  const isFailure = request.state === 'retryable' || request.state === 'stopped';
  if (request.state === 'prepared') {
    check(
      request.attemptsStarted === 0 && request.executionId === zero && request.claimedAt === 0n,
      'Invalid initial request',
    );
  } else {
    check(request.attemptsStarted > 0 && request.executionId !== zero, 'Missing execution binding');
  }
  if (isVerdict || isFailure) {
    check(
      request.terminalPayloadHash !== zero && request.terminalAt >= request.claimedAt,
      'Missing terminal report',
    );
  } else {
    check(
      request.terminalPayloadHash === zero && request.terminalAt === 0n,
      'Unexpected terminal report',
    );
  }
  if (isVerdict) {
    validateDecision(request.answer, request.reason);
    check(request.reportHash !== zero, 'Missing verdict commitments');
    check(request.failureCode === 0, 'Verdict cannot be a technical failure');
  } else {
    check(request.answer === 0 && request.reason === 0, 'Unexpected verdict');
    if (isFailure) {
      check(request.reportHash !== zero && request.failureCode > 0, 'Missing failure report');
    } else {
      check(request.reportHash === zero && request.failureCode === 0, 'Unexpected failure');
    }
  }
  if (request.state === 'applied') {
    check(request.appliedAt >= request.terminalAt, 'Invalid application time');
  } else {
    check(request.appliedAt === 0n, 'Unexpected application time');
  }
}
