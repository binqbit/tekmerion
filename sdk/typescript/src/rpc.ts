import { createPublicKey, verify } from 'node:crypto';
import Anchor from '@anchor-lang/core';
import {
  Connection,
  PublicKey,
  SYSVAR_CLOCK_PUBKEY,
  Transaction,
  TransactionInstruction,
  VersionedTransaction,
  type AccountInfo,
  type Commitment,
  type SendOptions,
  type Signer,
  type SimulateTransactionConfig,
  type RpcResponseAndContext,
  type SimulatedTransactionResponse,
} from '@solana/web3.js';
import { TekmerionClient } from './client.js';
import { caseAddress, copy, integer } from './codec.js';
import {
  SdkError,
  type AccountData,
  type CaseAddress,
  type CaseAccounts,
  type ClaimState,
  type Network,
  type Observation,
} from './types.js';
import type { CaseView, PolicyView } from './views.js';

export interface Wallet {
  signTransaction(transaction: Transaction): Promise<Transaction>;
}
export interface BlockhashLifetime {
  readonly blockhash: string;
  readonly lastValidBlockHeight: number;
}
export interface SendResult extends BlockhashLifetime {
  readonly signature: string;
  readonly slot: bigint;
}
export interface SignAndSendOptions {
  readonly signers?: readonly Signer[];
  readonly wallet?: Wallet;
  readonly sendOptions?: SendOptions;
}
export type Unsubscribe = () => Promise<void>;
export interface WatchCallbacks<T> {
  readonly onUpdate: (view: T) => void;
  readonly onError: (error: unknown) => void;
}
export type RpcConnection = Pick<
  Connection,
  | 'getMultipleAccountsInfoAndContext'
  | 'onAccountChange'
  | 'removeAccountChangeListener'
  | 'getLatestBlockhash'
  | 'simulateTransaction'
  | 'sendRawTransaction'
  | 'confirmTransaction'
>;
const SYSVAR_OWNER = new PublicKey('Sysvar1111111111111111111111111111111111111');

function slot(value: number): bigint {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new SdkError('InconsistentObservation', 'Unsafe RPC context slot');
  return BigInt(value);
}
function slotNumber(value: bigint): number {
  integer(value);
  if (value > BigInt(Number.MAX_SAFE_INTEGER))
    throw new SdkError('InvalidArgument', 'RPC requires a safe slot number');
  return Number(value);
}
function account(
  key: PublicKey,
  info: AccountInfo<Buffer> | null | undefined,
  contextSlot: bigint,
): AccountData {
  if (!info) throw new SdkError('InvalidAccount', `Account missing: ${key.toBase58()}`);
  return {
    key: copy(key),
    owner: copy(info.owner),
    executable: info.executable,
    data: Uint8Array.from(info.data),
    slot: contextSlot,
  };
}

/** Actual RPC I/O. The caller supplies the endpoint, commitment and signing authority. */
export class TekmerionRpc {
  readonly client: TekmerionClient;
  constructor(
    readonly connection: RpcConnection,
    network: Network = 'mainnet',
    readonly commitment: Commitment = 'confirmed',
  ) {
    this.client = new TekmerionClient(network);
  }
  private async batch(
    keys: readonly PublicKey[],
    minimumSlot?: bigint,
  ): Promise<{ observation: Observation; accounts: AccountData[] }> {
    const response = await this.connection.getMultipleAccountsInfoAndContext(
      [...keys, SYSVAR_CLOCK_PUBKEY],
      {
        commitment: this.commitment,
        ...(minimumSlot === undefined ? {} : { minContextSlot: slotNumber(minimumSlot) }),
      },
    );
    if (response.value.length !== keys.length + 1)
      throw new SdkError('InvalidAccount', 'Unexpected RPC account count');
    const contextSlot = slot(response.context.slot);
    if (minimumSlot !== undefined && contextSlot < minimumSlot)
      throw new SdkError('InconsistentObservation', 'RPC returned a stale context');
    const clock = account(SYSVAR_CLOCK_PUBKEY, response.value[keys.length], contextSlot);
    if (clock.executable || !clock.owner.equals(SYSVAR_OWNER) || clock.data.length !== 40)
      throw new SdkError('InvalidAccount', 'Invalid Clock sysvar');
    const clockData = Buffer.from(clock.data);
    if (clockData.readBigUInt64LE(0) !== contextSlot)
      throw new SdkError('InconsistentObservation', 'Clock slot differs from RPC context');
    const observation = { slot: contextSlot, unixTimestamp: clockData.readBigInt64LE(32) };
    return {
      observation,
      accounts: keys.map((key, index) => account(key, response.value[index], contextSlot)),
    };
  }
  async fetchPolicy(address: PublicKey, minimumSlot?: bigint): Promise<PolicyView> {
    const result = await this.batch([address], minimumSlot);
    return this.client.readPolicy(result.observation, address, result.accounts[0]!);
  }
  private async caseSnapshot(
    identity: CaseAddress,
    contributor?: PublicKey,
    minimumSlot?: bigint,
  ): Promise<{ view: CaseView; contributor: AccountData | undefined }> {
    const address = this.client.caseAddress(identity)[0];
    const preliminary = await this.batch([address], minimumSlot);
    const author = this.client.decodeCase(preliminary.observation, preliminary.accounts[0]!).author;
    const keys = [
      address,
      identity.policy,
      this.client.vaultAddress(identity)[0],
      this.client.receiptAddress(identity, author)[0],
    ];
    if (contributor) keys.push(this.client.receiptAddress(identity, contributor)[0]);
    // The second batch includes the Case again. Its immutable subject address
    // needs no live application account. The preliminary author lookup is not
    // part of the authenticated final observation.
    const result = await this.batch(keys, preliminary.observation.slot);
    const [caseAccount, policy, vault, authorReceipt] = result.accounts;
    const accounts: CaseAccounts = {
      case: caseAccount!,
      policy: policy!,
      vault: vault!,
      authorReceipt: authorReceipt!,
    };
    const view = this.client.readCase(result.observation, identity, accounts);
    if (!view.case.author.equals(author))
      throw new SdkError('InvalidRelationship', 'Case author changed during discovery');
    return { view, contributor: result.accounts[4] };
  }
  async fetchCase(identity: CaseAddress, minimumSlot?: bigint): Promise<CaseView> {
    return (await this.caseSnapshot(copy(identity), undefined, minimumSlot)).view;
  }
  async fetchContributorClaim(
    identity: CaseAddress,
    owner: PublicKey,
    minimumSlot?: bigint,
  ): Promise<{ view: CaseView; claim: ClaimState }> {
    const result = await this.caseSnapshot(copy(identity), copy(owner), minimumSlot);
    return { view: result.view, claim: result.view.contributorClaim(owner, result.contributor!) };
  }
  private async watch<T extends { observation: Observation }>(
    keys: readonly PublicKey[],
    fetch: (minimumSlot?: bigint) => Promise<T>,
    callbacks: WatchCallbacks<T>,
  ): Promise<Unsubscribe> {
    let closed = false;
    let running: Promise<void> | undefined;
    let requested: bigint | undefined;
    let dirty = false;
    let delivered = -1n;
    const subscriptions: number[] = [];
    const refresh = (): void => {
      if (closed || running) return;
      running = (async () => {
        while (dirty && !closed) {
          dirty = false;
          try {
            const view = await fetch(requested);
            if (
              !closed &&
              view.observation.slot >= delivered &&
              (requested === undefined || view.observation.slot >= requested)
            ) {
              delivered = view.observation.slot;
              callbacks.onUpdate(view);
            }
          } catch (error) {
            if (!closed) callbacks.onError(error);
          }
        }
      })().finally(() => {
        running = undefined;
        if (dirty && !closed) refresh();
      });
    };
    const unsubscribe: Unsubscribe = async () => {
      if (closed) return;
      closed = true;
      const cleanup = await Promise.allSettled(
        subscriptions.map((id) => this.connection.removeAccountChangeListener(id)),
      );
      const failed = cleanup.filter(
        (result): result is PromiseRejectedResult => result.status === 'rejected',
      );
      if (failed.length)
        throw new AggregateError(
          failed.map((result) => result.reason),
          'Subscription cleanup failed',
        );
    };
    try {
      const unique = new Map([...keys, SYSVAR_CLOCK_PUBKEY].map((key) => [key.toBase58(), key]));
      for (const key of unique.values()) {
        subscriptions.push(
          this.connection.onAccountChange(
            key,
            (_info, context) => {
              if (closed) return;
              try {
                const changedAt = slot(context.slot);
                if (changedAt < delivered) return;
                requested =
                  requested === undefined || changedAt > requested ? changedAt : requested;
                dirty = true;
                refresh();
              } catch (error) {
                callbacks.onError(error);
              }
            },
            this.commitment,
          ),
        );
      }
      dirty = true;
      refresh();
      await running;
      return unsubscribe;
    } catch (error) {
      await unsubscribe();
      throw error;
    }
  }
  async watchPolicy(
    address: PublicKey,
    callbacks: WatchCallbacks<PolicyView>,
  ): Promise<Unsubscribe> {
    const key = copy(address);
    return this.watch([key], (minimum) => this.fetchPolicy(key, minimum), callbacks);
  }
  async watchCase(
    identity: CaseAddress,
    callbacks: WatchCallbacks<CaseView>,
  ): Promise<Unsubscribe> {
    const expected = copy(identity);
    const initial = await this.fetchCase(expected);
    return this.watch(
      [
        caseAddress(expected, this.client.programId)[0],
        expected.policy,
        this.client.vaultAddress(expected)[0],
        this.client.receiptAddress(expected, initial.case.author)[0],
      ],
      (minimum) => this.fetchCase(expected, minimum),
      callbacks,
    );
  }
  async simulate(
    transaction: Transaction | VersionedTransaction,
    config: SimulateTransactionConfig = {},
  ): Promise<RpcResponseAndContext<SimulatedTransactionResponse>> {
    // Converting a legacy message preserves its existing signatures. The caller
    // chooses signature verification; no signer or blockhash is changed here.
    let candidate: VersionedTransaction;
    if (transaction instanceof VersionedTransaction) {
      candidate = transaction;
    } else {
      const message = transaction.compileMessage();
      const signatures = message.accountKeys
        .slice(0, message.header.numRequiredSignatures)
        .map(
          (key) =>
            transaction.signatures.find((entry) => entry.publicKey.equals(key))?.signature ??
            Buffer.alloc(64),
        );
      candidate = new VersionedTransaction(message, signatures);
    }
    return this.connection.simulateTransaction(candidate, {
      commitment: this.commitment,
      ...config,
    });
  }
  async sendSignedAndConfirm(
    transaction: Transaction | VersionedTransaction,
    lifetime: BlockhashLifetime,
    options: SendOptions = {},
  ): Promise<SendResult> {
    const confirmedLifetime = {
      blockhash: lifetime.blockhash,
      lastValidBlockHeight: lifetime.lastValidBlockHeight,
    };
    if (
      !Number.isSafeInteger(confirmedLifetime.lastValidBlockHeight) ||
      confirmedLifetime.lastValidBlockHeight < 0
    )
      throw new SdkError('InvalidArgument', 'Invalid block-height lifetime');
    const blockhash =
      transaction instanceof VersionedTransaction
        ? transaction.message.recentBlockhash
        : transaction.recentBlockhash;
    if (blockhash !== confirmedLifetime.blockhash)
      throw new SdkError(
        'InvalidArgument',
        'Confirmation lifetime does not match the signed transaction',
      );
    if (transaction instanceof VersionedTransaction) {
      const message = transaction.message.serialize();
      const signers = transaction.message.staticAccountKeys.slice(
        0,
        transaction.message.header.numRequiredSignatures,
      );
      if (transaction.signatures.length !== signers.length || signers.length === 0)
        throw new SdkError('InvalidArgument', 'Missing signatures');
      signers.forEach((key, index) => {
        const publicKey = createPublicKey({
          key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), key.toBuffer()]),
          format: 'der',
          type: 'spki',
        });
        if (!verify(null, message, publicKey, transaction.signatures[index]!))
          throw new SdkError('InvalidArgument', 'Invalid transaction signature');
      });
    }
    const bytes =
      transaction instanceof VersionedTransaction
        ? transaction.serialize()
        : transaction.serialize({ requireAllSignatures: true, verifySignatures: true });
    if (bytes.length > 1232)
      throw new SdkError('InvalidArgument', 'Transaction exceeds packet limit');
    const expectedSignature = Anchor.utils.bytes.bs58.encode(
      transaction instanceof VersionedTransaction
        ? transaction.signatures[0]!
        : transaction.signature!,
    );
    const signature = await this.connection.sendRawTransaction(bytes, {
      preflightCommitment: this.commitment,
      maxRetries: 0,
      ...options,
    });
    if (signature !== expectedSignature)
      throw new SdkError('InvalidAccount', 'RPC returned a different transaction signature');
    const confirmation = await this.connection.confirmTransaction(
      { signature, ...confirmedLifetime },
      this.commitment,
    );
    if (confirmation.value.err !== null)
      throw new Error(`Transaction ${signature} failed: ${JSON.stringify(confirmation.value.err)}`);
    return { signature, ...confirmedLifetime, slot: slot(confirmation.context.slot) };
  }
  async sendAndConfirm(transaction: Transaction, options: SignAndSendOptions): Promise<SendResult> {
    if (transaction.signatures.some((entry) => entry.signature !== null))
      throw new SdkError(
        'InvalidArgument',
        'Use sendSignedAndConfirm for previously signed transactions',
      );
    if (!options.wallet && !options.signers?.length)
      throw new SdkError('InvalidArgument', 'Caller signing authority is required');
    if (!transaction.feePayer)
      throw new SdkError('InvalidArgument', 'Set the intended fee payer before signing');
    if (transaction.nonceInfo)
      throw new SdkError(
        'InvalidArgument',
        'sendAndConfirm uses recent blockhashes, not durable nonces',
      );
    const feePayer = copy(transaction.feePayer);
    const instructions = transaction.instructions.map(
      (instruction) =>
        new TransactionInstruction({
          keys: copy(instruction.keys),
          programId: copy(instruction.programId),
          data: Buffer.from(instruction.data),
        }),
    );
    const lifetime = await this.connection.getLatestBlockhash(this.commitment);
    const unsigned = new Transaction({ ...lifetime, feePayer });
    unsigned.add(...instructions);
    const intended = unsigned.serializeMessage();
    if (options.signers?.length) unsigned.partialSign(...options.signers);
    const signed = options.wallet ? await options.wallet.signTransaction(unsigned) : unsigned;
    if (!signed.serializeMessage().equals(intended))
      throw new SdkError('InvalidArgument', 'Wallet changed the intended transaction message');
    return this.sendSignedAndConfirm(signed, lifetime, options.sendOptions);
  }
}
