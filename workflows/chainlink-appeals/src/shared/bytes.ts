import { hash, hex } from "@tekmerion/sdk/wire";
import { Buffer } from "node:buffer";

export function decodeText(bytes: Uint8Array): string {
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}

export function sha256(bytes: Uint8Array): string {
  return hex(hash(bytes));
}

export function base64(bytes: Uint8Array): string {
  // CRE bundles Buffer, while Javy does not provide the browser btoa global.
  return Buffer.from(bytes).toString("base64");
}
