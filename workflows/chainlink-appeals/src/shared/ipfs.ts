/** Raw CIDv1, SHA-256. Case commitments refer to exact UTF-8 document bytes. */
import { unhex, concat } from "@tekmerion/sdk/wire";

export function rawCid(hash: string): string {
  const bytes = concat(Uint8Array.of(1, 0x55, 0x12, 0x20), unhex(hash));
  const alphabet = "abcdefghijklmnopqrstuvwxyz234567";
  let value = 0,
    bits = 0,
    output = "b";
  for (const byte of bytes) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      output += alphabet[(value >>> bits) & 31];
    }
  }
  if (bits) output += alphabet[(value << (5 - bits)) & 31];
  return output;
}
