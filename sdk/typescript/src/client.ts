import { PublicKey, type TransactionInstruction } from '@solana/web3.js';
import {
  caseAddress,
  copy,
  policyAddress,
  ProtocolCodec,
  receiptAddress,
  vaultAddress,
} from './codec.js';
import { CaseView, PolicyView } from './views.js';
import type {
  AccountData,
  AuditAnswer,
  AuditReason,
  Case,
  CaseAccounts,
  CaseAddress,
  Network,
  Observation,
  PolicyRules,
  StakeReceipt,
} from './types.js';

export interface CreatePolicy {
  readonly payer: PublicKey;
  readonly admin: PublicKey;
  readonly clientAuthority: PublicKey;
  readonly identifier: bigint;
  readonly rules: PolicyRules;
}
export interface CreateCase {
  readonly payer: PublicKey;
  readonly clientAuthority: PublicKey;
  readonly author: PublicKey;
  readonly source: PublicKey;
  readonly authorPrincipal: bigint;
  readonly assertion: Uint8Array;
}
export interface Contribution {
  readonly payer: PublicKey;
  readonly owner: PublicKey;
  readonly source: PublicKey;
}
export interface ClaimDestination {
  readonly owner: PublicKey;
  readonly destination: PublicKey;
}

/** Unsigned builders and supplied-account readers; does not access a network. */
export class TekmerionClient {
  readonly #codec: ProtocolCodec;
  constructor(readonly network: Network = 'mainnet') {
    this.#codec = new ProtocolCodec(network);
  }
  get programId(): PublicKey {
    return this.#codec.programId;
  }
  get mint(): PublicKey {
    return this.#codec.mint;
  }
  accountSize(name: 'Policy' | 'Case' | 'StakeReceipt'): number {
    return this.#codec.accountSize(name);
  }
  policyAddress(admin: PublicKey, identifier: bigint): [PublicKey, number] {
    return policyAddress(admin, identifier, this.programId);
  }
  caseAddress(identity: CaseAddress): [PublicKey, number] {
    return caseAddress(identity, this.programId);
  }
  vaultAddress(identity: CaseAddress): [PublicKey, number] {
    return vaultAddress(this.caseAddress(identity)[0], this.programId);
  }
  receiptAddress(identity: CaseAddress, owner: PublicKey): [PublicKey, number] {
    return receiptAddress(this.caseAddress(identity)[0], owner, this.programId);
  }
  createPolicy(input: CreatePolicy): TransactionInstruction {
    return this.#codec.instruction(
      'create_policy',
      {
        payer: input.payer,
        admin: input.admin,
        policy: this.policyAddress(input.admin, input.identifier)[0],
      },
      { identifier: input.identifier, clientAuthority: input.clientAuthority, rules: input.rules },
    );
  }
  updatePolicy(admin: PublicKey, identifier: bigint, rules: PolicyRules): TransactionInstruction {
    return this.#codec.instruction(
      'update_policy',
      { admin, policy: this.policyAddress(admin, identifier)[0] },
      { rules },
    );
  }
  acquirePolicy(
    policy: PublicKey,
    clientAuthority: PublicKey,
    subject: PublicKey,
  ): TransactionInstruction {
    return this.#codec.instruction('acquire_policy', {
      policy,
      client_authority: clientAuthority,
      subject,
    });
  }
  createCase(identity: CaseAddress, input: CreateCase): TransactionInstruction {
    const remaining =
      identity.nonce === 0n
        ? []
        : [
            {
              pubkey: this.caseAddress({ ...identity, nonce: identity.nonce - 1n })[0],
              isSigner: false,
              isWritable: false,
            },
          ];
    return this.#codec.instruction(
      'create_case',
      {
        payer: input.payer,
        client_authority: input.clientAuthority,
        author: input.author,
        policy: identity.policy,
        subject: identity.subject,
        case: this.caseAddress(identity)[0],
        author_receipt: this.receiptAddress(identity, input.author)[0],
        source: input.source,
        vault: this.vaultAddress(identity)[0],
      },
      { nonce: identity.nonce, authorPrincipal: input.authorPrincipal, assertion: input.assertion },
      remaining,
    );
  }
  contribute(
    identity: CaseAddress,
    input: Contribution,
    requested: bigint,
  ): TransactionInstruction {
    return this.#codec.instruction(
      'contribute',
      {
        payer: input.payer,
        owner: input.owner,
        policy: identity.policy,
        case: this.caseAddress(identity)[0],
        receipt: this.receiptAddress(identity, input.owner)[0],
        source: input.source,
        vault: this.vaultAddress(identity)[0],
      },
      { requested },
    );
  }
  submitAudit(
    identity: CaseAddress,
    auditor: PublicKey,
    answer: AuditAnswer,
    reason: AuditReason,
    feeDestinations: readonly PublicKey[] = [],
  ): TransactionInstruction {
    return this.#codec.instruction(
      'submit_audit',
      {
        auditor,
        case: this.caseAddress(identity)[0],
        policy: identity.policy,
        vault: this.vaultAddress(identity)[0],
      },
      { answer, reason },
      feeDestinations.map((pubkey) => ({ pubkey, isSigner: false, isWritable: true })),
    );
  }
  finalizeUnchallenged(identity: CaseAddress): TransactionInstruction {
    return this.#codec.instruction('finalize_unchallenged', {
      case: this.caseAddress(identity)[0],
      vault: this.vaultAddress(identity)[0],
    });
  }
  finalizeTimeout(identity: CaseAddress): TransactionInstruction {
    return this.#codec.instruction('finalize_timeout', {
      case: this.caseAddress(identity)[0],
      vault: this.vaultAddress(identity)[0],
    });
  }
  claim(identity: CaseAddress, author: PublicKey, input: ClaimDestination): TransactionInstruction {
    return this.#codec.instruction('claim', {
      owner: input.owner,
      case: this.caseAddress(identity)[0],
      author_receipt: this.receiptAddress(identity, author)[0],
      receipt: this.receiptAddress(identity, input.owner)[0],
      vault: this.vaultAddress(identity)[0],
      destination: input.destination,
    });
  }
  readPolicy(observation: Observation, expected: PublicKey, account: AccountData): PolicyView {
    return PolicyView.read(this.#codec, observation, expected, account);
  }
  readCase(observation: Observation, expected: CaseAddress, accounts: CaseAccounts): CaseView {
    return CaseView.read(this.#codec, observation, expected, accounts);
  }
  /** Structural decoding only. Use readCase for relationships, lifecycle and custody. */
  decodeCase(observation: Observation, account: AccountData): Case {
    return copy(this.#codec.decode<Case>('Case', account, observation));
  }
  /** Structural decoding only. CaseView.contributorClaim authenticates a claim's role. */
  decodeStakeReceipt(observation: Observation, account: AccountData): StakeReceipt {
    return copy(this.#codec.decode<StakeReceipt>('StakeReceipt', account, observation));
  }
}
export function createClient(network: Network = 'mainnet'): TekmerionClient {
  return new TekmerionClient(network);
}
