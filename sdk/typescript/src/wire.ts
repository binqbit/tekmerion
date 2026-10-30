/** Portable Tekmerion wire primitives and committed Case identity.
 * No Node globals, wallet, RPC, provider or application dependencies. */
import { sha256 } from '@noble/hashes/sha256';
import { ed25519 } from '@noble/curves/ed25519';
import bs58 from 'bs58';

export type Network = 'mainnet' | 'devnet';
export const CORE_ID = 'TEKxbA5q3EhSDSBeYiW9AT3GGPyzruBSQUcuQXxdy3s';
export const utf8 = (value: string): Uint8Array => new TextEncoder().encode(value);
export function check(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
export function concat(...parts: readonly Uint8Array[]): Uint8Array {
  const result = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}
export function equal(a: Uint8Array, b: Uint8Array): boolean {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}
export const hash = (bytes: Uint8Array): Uint8Array => sha256(bytes);
export const hex = (bytes: Uint8Array): string =>
  Array.from(bytes, (value) => value.toString(16).padStart(2, '0')).join('');
export function unhex(value: string, size = 32): Uint8Array {
  check(new RegExp(`^[0-9a-f]{${size * 2}}$`).test(value), 'Invalid canonical hex');
  return Uint8Array.from(value.match(/../g)!.map((part) => parseInt(part, 16)));
}
export function address(value: string): Uint8Array {
  const bytes = bs58.decode(value);
  check(bytes.length === 32 && bs58.encode(bytes) === value, 'Invalid address');
  return bytes;
}
export const base58 = (bytes: Uint8Array): string => bs58.encode(bytes);
export function integerBytes(value: bigint, signed = false): Uint8Array {
  check(
    typeof value === 'bigint' &&
      value >= (signed ? -(1n << 63n) : 0n) &&
      value < (signed ? 1n << 63n : 1n << 64n),
    'Integer out of range',
  );
  const bytes = new Uint8Array(8);
  const view = new DataView(bytes.buffer);
  if (signed) view.setBigInt64(0, value, true);
  else view.setBigUint64(0, value, true);
  return bytes;
}
export function pda(seeds: readonly Uint8Array[], program: string): [string, number] {
  check(seeds.length < 16 && seeds.every((seed) => seed.length <= 32), 'Invalid PDA seeds');
  for (let bump = 255; bump >= 0; bump--) {
    const candidate = hash(
      concat(...seeds, Uint8Array.of(bump), address(program), utf8('ProgramDerivedAddress')),
    );
    try {
      ed25519.ExtendedPoint.fromHex(candidate);
    } catch {
      return [base58(candidate), bump];
    }
  }
  throw new Error('No PDA bump');
}
export const discriminator = (namespace: 'account' | 'global', name: string): Uint8Array =>
  hash(utf8(`${namespace}:${name}`)).slice(0, 8);

export function validateDecision(answer: number, reason: number): void {
  check(
    Number.isInteger(answer) &&
      Number.isInteger(reason) &&
      (answer === 0 || answer === 1 ? reason === 0 : answer === 2 && reason >= 1 && reason <= 5),
    'Invalid decision',
  );
}
export class Reader {
  offset = 0;
  constructor(readonly bytes: Uint8Array) {}
  take(size: number): Uint8Array {
    check(
      Number.isSafeInteger(size) && size >= 0 && this.offset + size <= this.bytes.length,
      'Truncated account',
    );
    const value = this.bytes.slice(this.offset, this.offset + size);
    this.offset += size;
    return value;
  }
  u8(): number {
    return this.take(1)[0]!;
  }
  u32(): number {
    const b = this.take(4);
    return new DataView(b.buffer, b.byteOffset, b.byteLength).getUint32(0, true);
  }
  u64(): bigint {
    const b = this.take(8);
    return new DataView(b.buffer, b.byteOffset, b.byteLength).getBigUint64(0, true);
  }
  i64(): bigint {
    const b = this.take(8);
    return new DataView(b.buffer, b.byteOffset, b.byteLength).getBigInt64(0, true);
  }
  key(): string {
    return base58(this.take(32));
  }
  option<T>(read: () => T): T | null {
    const tag = this.u8();
    check(tag === 0 || tag === 1, 'Invalid option');
    return tag === 1 ? read() : null;
  }
  account(name: string, length: number): void {
    check(
      this.bytes.length === length && equal(this.take(8), discriminator('account', name)),
      'Invalid account layout',
    );
  }
}
export interface CaseIdentity {
  policy: string;
  subject: string;
  author: string;
  mode: 'challenge' | 'direct';
  nonce: bigint;
  agreementHash: string;
  resultHash: string;
  deadline: bigint;
  phase: number;
  challengeTarget: bigint;
  challengePrincipal: bigint;
  auditVotes: { auditorIndex: number; answer: number; reason: number }[];
  auditResponse: { answer: number; reason: number } | null;
  result: number | null;
  finalizedAt: bigint | null;
}
/** Read the committed identity and lifecycle needed for dispatch. Full financial
 * account validation remains in the core SDK and on-chain program. */
export function decodeCaseIdentity(bytes: Uint8Array): CaseIdentity {
  const r = new Reader(bytes);
  r.account('Case', 356);
  const policy = r.key(),
    subject = r.key(),
    author = r.key();
  const nonce = r.u64();
  r.option(() => r.key());
  const agreementHash = hex(r.take(32)),
    resultHash = hex(r.take(32));
  const mode = r.u8();
  check(mode <= 1, 'Invalid Case mode');
  r.u64(); // Author principal; financial validation belongs to CaseView.
  const challengeTarget = r.u64();
  const challengePrincipal = r.u64();
  r.i64(); // Funding deadline.
  const deadline = r.option(() => r.i64());
  const votes = r.u32();
  check(votes <= 9, 'Too many votes');
  const auditVotes = Array.from({ length: votes }, () => {
    const auditorIndex = r.u8();
    const answer = r.u8();
    const reason = r.u8();
    check(auditorIndex < 9, 'Invalid auditor index');
    validateDecision(answer, reason);
    return { auditorIndex, answer, reason };
  });
  const auditResponse = r.option(() => {
    const answer = r.u8();
    const reason = r.u8();
    validateDecision(answer, reason);
    return { answer, reason };
  });
  const phase = r.u8();
  check(phase <= 2 && deadline !== null, 'Case has no audit');
  const result = r.option(() => {
    const value = r.u8();
    check(value <= 2, 'Invalid Case result');
    return value;
  });
  const finalizedAt = r.option(() => r.i64());
  return {
    policy,
    subject,
    author,
    mode: mode === 0 ? 'challenge' : 'direct',
    nonce,
    agreementHash,
    resultHash,
    deadline,
    phase,
    challengeTarget,
    challengePrincipal,
    auditVotes,
    auditResponse,
    result,
    finalizedAt,
  };
}
