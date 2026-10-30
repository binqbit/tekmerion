/** Bounded binary primitives shared by the confidential wire codecs. */
import { Reader, check } from '@tekmerion/sdk/wire';

export function u16(value: number): Uint8Array {
  check(Number.isInteger(value) && value >= 0 && value <= 65535, 'Invalid u16');
  return Uint8Array.of(value & 255, value >>> 8);
}
export function u32(value: number): Uint8Array {
  check(Number.isInteger(value) && value >= 0 && value <= 0xffffffff, 'Invalid u32');
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setUint32(0, value, true);
  return bytes;
}
export class BoundedReader extends Reader {
  u16(): number {
    const b = this.take(2);
    return b[0]! | (b[1]! << 8);
  }
  end(): void {
    check(this.offset === this.bytes.length, 'Trailing data');
  }
}
