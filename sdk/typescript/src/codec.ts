import Anchor from '@anchor-lang/core';
import type { BN, BorshCoder, Idl, IdlType } from '@anchor-lang/core';
import { PublicKey, TransactionInstruction, type AccountMeta } from '@solana/web3.js';
import mainnet from './idl/mainnet.json' with { type: 'json' };
import devnet from './idl/devnet.json' with { type: 'json' };
import {
  SdkError,
  type AccountData,
  type CaseAddress,
  type Network,
  type Observation,
} from './types.js';

export const MAX_AUDITORS = 9;
export const U64_MAX = (1n << 64n) - 1n;
export const U128_MAX = (1n << 128n) - 1n;
export const I64_MIN = -(1n << 63n);
export const I64_MAX = (1n << 63n) - 1n;
export const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
export const PROGRAM_ID = new PublicKey(mainnet.address);
const camel = (name: string): string =>
  name
    .replace(/_([a-z])/g, (_, value: string) => value.toUpperCase())
    .replace(/^[A-Z]/, (value) => value.toLowerCase());

export function integer(value: bigint, min = 0n, max = U64_MAX): bigint {
  if (typeof value !== 'bigint' || value < min || value > max)
    throw new SdkError('InvalidArgument', 'Expected an in-range bigint');
  return value;
}
export function u64Bytes(value: bigint): Buffer {
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64LE(integer(value));
  return bytes;
}
export function policyAddress(
  admin: PublicKey,
  identifier: bigint,
  programId = PROGRAM_ID,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from('policy'), admin.toBuffer(), u64Bytes(identifier)],
    programId,
  );
}
export function caseAddress(identity: CaseAddress, programId = PROGRAM_ID): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from('case'),
      identity.policy.toBuffer(),
      identity.subject.toBuffer(),
      u64Bytes(identity.nonce),
    ],
    programId,
  );
}
export function vaultAddress(address: PublicKey, programId = PROGRAM_ID): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([Buffer.from('vault'), address.toBuffer()], programId);
}
export function receiptAddress(
  address: PublicKey,
  owner: PublicKey,
  programId = PROGRAM_ID,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from('receipt'), address.toBuffer(), owner.toBuffer()],
    programId,
  );
}

/** Copy snapshots at the API boundary, including mutable PublicKey internals. */
export function copy<T>(value: T): T {
  if (value instanceof PublicKey) return new PublicKey(value.toBytes()) as T;
  if (value instanceof Uint8Array) return Uint8Array.from(value) as T;
  if (Array.isArray(value)) return value.map(copy) as T;
  if (value !== null && typeof value === 'object')
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, copy(item)])) as T;
  return value;
}
export function checkObservation(observation: Observation): void {
  integer(observation.slot);
  integer(observation.unixTimestamp, I64_MIN, I64_MAX);
}
export function checkAccount(
  account: AccountData,
  observation: Observation,
  owner?: PublicKey,
): void {
  if (account.slot !== observation.slot) throw new SdkError('InconsistentObservation');
  if (account.executable || (owner && !account.owner.equals(owner)))
    throw new SdkError('InvalidAccount');
}

/** Internal IDL-backed codec. All protocol layouts and account meta ordering come from the IDL. */
export class ProtocolCodec {
  readonly #idl: Idl;
  readonly #coder: BorshCoder;
  constructor(readonly network: Network) {
    if (network !== 'mainnet' && network !== 'devnet')
      throw new SdkError('InvalidArgument', 'Unknown network');
    this.#idl = structuredClone(network === 'mainnet' ? mainnet : devnet) as Idl;
    this.#coder = new Anchor.BorshCoder(this.#idl);
  }
  get programId(): PublicKey {
    return new PublicKey(this.#idl.address);
  }
  get mint(): PublicKey {
    const account = this.#idl.instructions
      .find((ix) => ix.name === 'submit_audit')!
      .accounts.find((item) => item.name === 'mint');
    if (!account || !('address' in account) || !account.address)
      throw new Error('IDL missing canonical mint');
    return new PublicKey(account.address);
  }
  private definition(name: string) {
    const found = this.#idl.types?.find((type) => type.name === name);
    if (!found) throw new Error(`Unknown IDL type ${name}`);
    return found.type;
  }
  private size(type: IdlType): number {
    if (typeof type === 'string') {
      const sizes: Record<string, number> = {
        bool: 1,
        u8: 1,
        u64: 8,
        i64: 8,
        u128: 16,
        pubkey: 32,
      };
      const size = sizes[type];
      if (size === undefined) throw new Error(`Unsupported IDL type ${type}`);
      return size;
    }
    if ('option' in type) return 1 + this.size(type.option);
    if ('vec' in type) return 4 + MAX_AUDITORS * this.size(type.vec);
    if ('array' in type && typeof type.array[1] === 'number')
      return type.array[1] * this.size(type.array[0]);
    if ('defined' in type) {
      const definition = this.definition(type.defined.name);
      if (definition.kind === 'enum' && definition.variants.every((variant) => !variant.fields))
        return 1;
      if (
        definition.kind === 'struct' &&
        definition.fields?.every(
          (field) => typeof field === 'object' && field !== null && 'name' in field,
        )
      ) {
        return definition.fields.reduce(
          (total, field) => total + this.size((field as { type: IdlType }).type),
          0,
        );
      }
    }
    throw new Error('Unsupported IDL shape');
  }
  accountSize(name: 'Policy' | 'Case' | 'StakeReceipt'): number {
    const discriminator = this.#idl.accounts!.find(
      (account) => account.name === name,
    )!.discriminator;
    return discriminator.length + this.size({ defined: { name } });
  }
  private transform(type: IdlType, value: unknown, encode: boolean): unknown {
    if (typeof type === 'string') {
      if (['u64', 'u128', 'i64'].includes(type)) {
        if (!encode) return BigInt((value as BN).toString(10));
        const amount = integer(
          value as bigint,
          type === 'i64' ? I64_MIN : 0n,
          type === 'i64' ? I64_MAX : type === 'u128' ? U128_MAX : U64_MAX,
        );
        return new Anchor.BN(amount.toString());
      }
      if (
        encode &&
        type === 'u8' &&
        (typeof value !== 'number' || !Number.isInteger(value) || value < 0 || value > 255)
      )
        throw new SdkError('InvalidArgument', 'Expected u8');
      if (type === 'pubkey') {
        if (!(value instanceof PublicKey))
          throw new SdkError('InvalidArgument', 'Expected PublicKey');
        return new PublicKey(value.toBytes());
      }
      return value;
    }
    if ('option' in type) return value === null ? null : this.transform(type.option, value, encode);
    if ('vec' in type || 'array' in type) {
      const values = value instanceof Uint8Array ? Array.from(value) : value;
      if (!Array.isArray(values)) throw new SdkError('InvalidArgument', 'Expected array');
      if ('vec' in type && values.length > MAX_AUDITORS)
        throw new SdkError(
          encode ? 'InvalidArgument' : 'InvalidState',
          'Committee capacity exceeded',
        );
      if ('array' in type && values.length !== type.array[1])
        throw new SdkError('InvalidArgument', 'Incorrect fixed array length');
      return values.map((item) =>
        this.transform('vec' in type ? type.vec : type.array[0], item, encode),
      );
    }
    if ('defined' in type) {
      const definition = this.definition(type.defined.name);
      if (definition.kind === 'enum') {
        const name = encode ? value : Object.keys(value as object)[0];
        const variant = definition.variants.find(
          (variant) => (encode ? camel(variant.name) : variant.name) === name,
        );
        if (!variant)
          throw new SdkError(encode ? 'InvalidArgument' : 'InvalidAccount', 'Unknown enum variant');
        return encode ? { [variant.name]: {} } : camel(variant.name);
      }
      if (definition.kind === 'struct') {
        const object = value as Record<string, unknown>;
        return Object.fromEntries(
          (definition.fields ?? []).map((field) => {
            if (typeof field !== 'object' || field === null || !('name' in field))
              throw new Error('Unsupported unnamed IDL field');
            return [
              encode ? field.name : camel(field.name),
              this.transform(field.type, object[encode ? camel(field.name) : field.name], encode),
            ];
          }),
        );
      }
    }
    throw new Error('Unsupported IDL type');
  }
  instruction(
    name: string,
    accounts: Record<string, PublicKey>,
    args: Record<string, unknown> = {},
    remaining: readonly AccountMeta[] = [],
  ): TransactionInstruction {
    const definition = this.#idl.instructions.find((instruction) => instruction.name === name);
    if (!definition) throw new Error(`Unknown instruction ${name}`);
    const keys = definition.accounts.map((account): AccountMeta => {
      if ('accounts' in account) throw new Error('Unexpected nested accounts');
      const pubkey = account.address ? new PublicKey(account.address) : accounts[account.name];
      if (!pubkey) throw new SdkError('InvalidArgument', `Missing ${account.name}`);
      return {
        pubkey: new PublicKey(pubkey.toBytes()),
        isSigner: account.signer ?? false,
        isWritable: account.writable ?? false,
      };
    });
    const values = Object.fromEntries(
      definition.args.map((arg) => [
        arg.name,
        this.transform(arg.type, args[camel(arg.name)], true),
      ]),
    );
    return new TransactionInstruction({
      programId: this.programId,
      keys: [...keys, ...remaining.map(copy)],
      data: this.#coder.instruction.encode(name, values),
    });
  }
  decode<T>(
    name: 'Policy' | 'Case' | 'StakeReceipt',
    account: AccountData,
    observation: Observation,
  ): T {
    checkObservation(observation);
    checkAccount(account, observation, this.programId);
    if (account.data.length !== this.accountSize(name))
      throw new SdkError('InvalidAccount', 'Incorrect account allocation');
    try {
      const bytes = Buffer.from(account.data);
      const raw = this.#coder.accounts.decode(name, bytes);
      // Re-encoding the consumed payload rejects noncanonical bool/option tags.
      // Allocated trailing bytes are intentionally ignored, as in Anchor Rust.
      const canonical = this.#coder.types.encode(name, raw);
      const offset = this.#coder.accounts.accountDiscriminator(name).length;
      if (!bytes.subarray(offset, offset + canonical.length).equals(canonical))
        throw new Error('Noncanonical encoding');
      return this.transform({ defined: { name } }, raw, false) as T;
    } catch (cause) {
      if (cause instanceof SdkError) throw cause;
      throw new SdkError('InvalidAccount', 'Invalid Anchor account encoding', { cause });
    }
  }
}
