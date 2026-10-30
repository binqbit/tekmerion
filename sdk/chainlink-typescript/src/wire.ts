/** Portable v3 adapter protocol. Safe to import in browsers and CRE WASM. */
export * from '@tekmerion/sdk/wire';
export * from './wire/addresses.js';
export * from './wire/accounts.js';
export * from './wire/binding.js';
export * from './wire/report.js';
export { BoundedReader, u16, u32 } from './wire/binary.js';
