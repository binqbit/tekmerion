/** Finalized Solana reads through the official capability. No HTTP RPC fallback. */
import { cre, type Runtime } from "@chainlink/cre-sdk";
import { address, base58, check } from "@tekmerion/sdk/wire";
import { base64 } from "../../shared/bytes.js";

export const solanaSelectors = {
  mainnet: 124615329519749607n,
  devnet: 16423721717087811551n,
} as const;

export interface ProgramAccount {
  key: string;
  account: {
    owner: string;
    executable: boolean;
    bytes: Uint8Array | null;
  } | null;
}

export function readFinalizedAccounts(
  runtime: Runtime<unknown>,
  selector: bigint,
  keys: string[],
) {
  const values = new cre.capabilities.SolanaClient(selector)
    .getMultipleAccountsWithOpts(runtime, {
      accounts: keys.map((key) => base64(address(key))),
      opts: {
        encoding: "ENCODING_TYPE_BASE64",
        commitment: "COMMITMENT_TYPE_FINALIZED",
      },
    })
    .result().value;
  check(values.length === keys.length, "Incomplete official chain read");
  return values.map((value) => {
    const account = value.account;
    check(
      account && !account.executable && account.data?.body.case === "raw",
      "Missing/invalid account",
    );
    return { owner: base58(account.owner), bytes: account.data.body.value };
  });
}

export function scanFinalizedAccounts(
  runtime: Runtime<unknown>,
  selector: bigint,
  program: string,
  filter: {
    size: number;
    offset: number;
    key: string;
    stateOffset: number;
    state: number;
  },
): ProgramAccount[] {
  const rows = new cre.capabilities.SolanaClient(selector)
    .getProgramAccounts(runtime, {
      program: base64(address(program)),
      opts: {
        encoding: "ENCODING_TYPE_BASE64",
        commitment: "COMMITMENT_TYPE_FINALIZED",
        filters: [
          { dataSize: String(filter.size) },
          {
            memcmp: {
              offset: String(filter.stateOffset),
              bytes: base64(Uint8Array.of(filter.state)),
            },
          },
          {
            memcmp: {
              offset: String(filter.offset),
              bytes: base64(address(filter.key)),
            },
          },
        ],
      },
    })
    .result().value;
  return rows.map(({ pubkey, account }) => ({
    key: base58(pubkey),
    account: account
      ? {
          owner: base58(account.owner),
          executable: account.executable,
          bytes:
            account.data?.body.case === "raw" ? account.data.body.value : null,
        }
      : null,
  }));
}
