/** Discover runnable requests without letting terminal history consume the queue. */
import type { Runtime } from "@chainlink/cre-sdk";
import {
  ADAPTER_IDS,
  profileAddress,
  requestAddress,
  decodeConfidentialRequest,
  REQUEST_ACCOUNT_BYTES,
  type ChainlinkNetwork,
} from "@tekmerion/chainlink-sdk/wire";
import type { AgentConfig } from "../../research/config.js";
import { scanFinalizedAccounts, solanaSelectors } from "../chainlink/solana.js";
import { readBinding } from "./accounts.js";

/** Filter terminal history at the RPC boundary; completed requests cannot fill this queue. */
export function pendingRequest(
  runtime: Runtime<unknown>,
  network: ChainlinkNetwork,
  policy: string,
  config: AgentConfig,
): { case: string } | null {
  const profile = profileAddress(policy, network)[0];
  const rows = [0, 2].flatMap((state) =>
    scanFinalizedAccounts(
      runtime,
      solanaSelectors[network],
      ADAPTER_IDS[network],
      {
        size: REQUEST_ACCOUNT_BYTES,
        offset: 10,
        key: profile,
        stateOffset: 106,
        state,
      },
    ),
  );
  rows.sort((a, b) => a.key.localeCompare(b.key));
  // Inspect a bounded rotating slice. Stale unclaimed cases cannot monopolize
  // every cron run; a direct Case trigger is also available for precise dispatch.
  const start = rows.length
    ? Math.floor(runtime.now().getTime() / 60000) % rows.length
    : 0;
  const candidates = Array.from(
    { length: Math.min(rows.length, 4) },
    (_, index) => rows[(start + index) % rows.length]!,
  );
  for (const row of candidates) {
    if (
      !row.account ||
      row.account.executable ||
      row.account.owner !== ADAPTER_IDS[network] ||
      !row.account.bytes
    )
      continue;
    try {
      const request = decodeConfidentialRequest(row.account.bytes);
      if (
        request.profile !== profile ||
        !["prepared", "retryable"].includes(request.state) ||
        requestAddress(profile, request.case, network)[0] !== row.key
      )
        continue;
      const { binding, now } = readBinding(runtime, network, request.case);
      if (
        binding.case.phase === 1 &&
        request.attemptsStarted < binding.profile.config.maxAttempts &&
        binding.case.deadline - now >=
          BigInt(config.maxExecutionSeconds + config.deliveryReserveSeconds)
      ) {
        return { case: request.case };
      }
    } catch {
      /* A malformed or unavailable candidate cannot authorize an execution. */
    }
  }
  return null;
}
