/** Unsigned adapter instructions. Signing and submission belong to the caller. */
import {
  PublicKey,
  ComputeBudgetProgram,
  SystemProgram,
  TransactionInstruction,
  type AccountMeta,
} from '@solana/web3.js';
import {
  TekmerionClient,
  PublicKey as CorePublicKey,
  TOKEN_PROGRAM_ID,
  type CaseAddress,
  type CreatePolicy,
  type Network,
} from '@tekmerion/sdk';
import { address, concat, discriminator, pda, utf8 } from '@tekmerion/sdk/wire';
import {
  ADAPTER_IDS,
  profileAddress,
  requestAddress,
  encodeConfig,
  type ConfidentialConfig,
} from './wire.js';

export function confidentialComputeBudget(): TransactionInstruction {
  return ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 });
}

const meta = (pubkey: PublicKey, isWritable = false, isSigner = false): AccountMeta => ({
  pubkey,
  isWritable,
  isSigner,
});

export class ConfidentialChainlinkClient {
  readonly core: TekmerionClient;

  constructor(readonly network: Network = 'mainnet') {
    this.core = new TekmerionClient(network);
  }

  get programId(): PublicKey {
    return new PublicKey(ADAPTER_IDS[this.network]);
  }

  profile(policy: PublicKey): PublicKey {
    return new PublicKey(profileAddress(policy.toBase58(), this.network)[0]);
  }

  request(identity: CaseAddress): PublicKey {
    return new PublicKey(
      requestAddress(
        this.profile(identity.policy).toBase58(),
        this.core.caseAddress(identity)[0].toBase58(),
        this.network,
      )[0],
    );
  }

  /** Submit both instructions in one transaction to commit the Policy and Profile together. */
  createPolicy(input: CreatePolicy, config: ConfidentialConfig): TransactionInstruction[] {
    const coreKey = (key: PublicKey) => new CorePublicKey(key.toBytes());
    const coreInput: CreatePolicy = {
      ...input,
      payer: coreKey(input.payer),
      admin: coreKey(input.admin),
      clientAuthority: coreKey(input.clientAuthority),
      rules: {
        ...input.rules,
        auditors: input.rules.auditors.map((auditor) => ({
          authority: coreKey(auditor.authority),
          feeRecipient: coreKey(auditor.feeRecipient),
        })),
      },
    };
    const policy = this.core.policyAddress(coreInput.admin, coreInput.identifier)[0];
    return [
      this.core.createPolicy(coreInput),
      this.registerProfile(input.payer, input.admin, policy, config),
    ];
  }

  registerProfile(
    payer: PublicKey,
    admin: PublicKey,
    policy: PublicKey,
    config: ConfidentialConfig,
  ): TransactionInstruction {
    return this.instruction(
      'register_confidential_profile',
      [
        meta(payer, true, true),
        meta(admin, false, true),
        meta(policy),
        meta(this.profile(policy), true),
        meta(new PublicKey(config.forwarderProgram)),
        meta(new PublicKey(config.forwarderState)),
        meta(SystemProgram.programId),
      ],
      encodeConfig(config),
    );
  }

  prepareRequest(payer: PublicKey, identity: CaseAddress): TransactionInstruction {
    return this.instruction('prepare_confidential_request', [
      meta(payer, true, true),
      meta(this.profile(identity.policy)),
      meta(identity.policy),
      meta(this.core.caseAddress(identity)[0]),
      meta(this.request(identity), true),
      meta(SystemProgram.programId),
    ]);
  }

  applyVerdict(identity: CaseAddress, feeDestination: PublicKey): TransactionInstruction {
    return this.instruction('apply_confidential_verdict', [
      meta(this.profile(identity.policy)),
      meta(identity.policy),
      meta(this.core.caseAddress(identity)[0], true),
      meta(this.request(identity), true),
      meta(this.core.mint),
      meta(this.core.vaultAddress(identity)[0], true),
      meta(TOKEN_PROGRAM_ID),
      meta(this.core.programId),
      meta(feeDestination, true),
    ]);
  }

  /** Ordered receiver accounts committed by the native Forwarder account hash. */
  receiverAccounts(identity: CaseAddress, config: ConfidentialConfig): AccountMeta[] {
    const authority = pda(
      [utf8('forwarder'), address(config.forwarderState), this.programId.toBytes()],
      config.forwarderProgram,
    )[0];
    return [
      meta(new PublicKey(config.forwarderState)),
      meta(new PublicKey(authority), false, true),
      meta(this.profile(identity.policy)),
      meta(identity.policy),
      meta(this.core.caseAddress(identity)[0]),
      meta(this.request(identity), true),
    ];
  }

  private instruction(
    name: string,
    keys: AccountMeta[],
    args: Uint8Array = new Uint8Array(),
  ): TransactionInstruction {
    return new TransactionInstruction({
      programId: this.programId,
      keys,
      data: Buffer.from(concat(discriminator('global', name), args)),
    });
  }
}
