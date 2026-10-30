/** Read and authenticate one audit's finalized Solana accounts. */
import type { Runtime } from "@chainlink/cre-sdk";
import { CORE_ID, check, decodeCaseIdentity } from "@tekmerion/sdk/wire";
import {
  ADAPTER_IDS,
  profileAddress,
  requestAddress,
  validateConfidentialBinding,
  type ChainlinkNetwork,
} from "@tekmerion/chainlink-sdk/wire";
import type { AuditSnapshot } from "../../audit/ports.js";
import { readFinalizedAccounts, solanaSelectors } from "../chainlink/solana.js";

const CLOCK = "SysvarC1ock11111111111111111111111111111111";
const SYSVAR_OWNER = "Sysvar1111111111111111111111111111111111111";

export function readBinding(
  runtime: Runtime<unknown>,
  network: ChainlinkNetwork,
  caseKey: string,
): AuditSnapshot {
  const [identity] = readFinalizedAccounts(runtime, solanaSelectors[network], [
    caseKey,
  ]);
  check(identity!.owner === CORE_ID, "Wrong core owner");
  const caseIdentity = decodeCaseIdentity(identity!.bytes);
  const profileKey = profileAddress(caseIdentity.policy, network)[0];
  const requestKey = requestAddress(profileKey, caseKey, network)[0];
  const [profile, policy, caseAccount, request, clock] = readFinalizedAccounts(
    runtime,
    solanaSelectors[network],
    [profileKey, caseIdentity.policy, caseKey, requestKey, CLOCK],
  );
  check(
    profile!.owner === ADAPTER_IDS[network] &&
      request!.owner === ADAPTER_IDS[network],
    "Wrong adapter account owner",
  );
  check(
    policy!.owner === CORE_ID && caseAccount!.owner === CORE_ID,
    "Wrong core account owner",
  );
  check(
    clock!.owner === SYSVAR_OWNER && clock!.bytes.length === 40,
    "Invalid Clock",
  );
  const binding = validateConfidentialBinding({
    network,
    profileKey,
    profileBytes: profile!.bytes,
    policyBytes: policy!.bytes,
    caseKey,
    caseBytes: caseAccount!.bytes,
    requestKey,
    requestBytes: request!.bytes,
  });
  const now = new DataView(
    clock!.bytes.buffer,
    clock!.bytes.byteOffset,
    clock!.bytes.byteLength,
  ).getBigInt64(32, true);
  return { binding, now };
}
