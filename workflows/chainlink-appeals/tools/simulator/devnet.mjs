import { createHash } from "node:crypto";
import { PublicKey, TekmerionClient, TOKEN_PROGRAM_ID } from "@tekmerion/sdk";
import {
  ADAPTER_IDS,
  decodeConfidentialProfile,
  profileAddress,
} from "@tekmerion/chainlink-sdk/wire";
import { agentConfigHash } from "../../dist/research/config.js";

// CRE CLI 1.39.0's deployed simulation transport and standalone engine identity.
export const DEVNET = Object.freeze({
  forwarderProgram: "7kuEAA3mSC1Tz8gQjnvH7bKFda9xSPRRin9SZbH49cNK",
  forwarderState: "5Tipz3yhTBdVsDbaBxZkrp7Gjf3brGq5SKkxReefPMP7",
  workflowId: "1".repeat(64),
  genesisHash: "EtWTRABZaYq6iMfeYKouRu166VU2xqa1",
});
const loader = "BPFLoaderUpgradeab1e11111111111111111111111";
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

export function enrollmentConfig(config, maxAttempts) {
  if (config.network !== "devnet")
    throw new Error("Simulation requires Devnet");
  if (
    !Number.isInteger(maxAttempts) ||
    maxAttempts < 1 ||
    maxAttempts > 65535
  ) {
    throw new Error("Set maxAttempts between 1 and 65535");
  }
  return {
    forwarderProgram: DEVNET.forwarderProgram,
    forwarderState: DEVNET.forwarderState,
    workflowId: DEVNET.workflowId,
    agentConfigHash: agentConfigHash(config.agent),
    maxAttempts,
  };
}

export function rpcClient(url) {
  const endpoint = new URL(url);
  if (!["https:", "http:"].includes(endpoint.protocol))
    throw new Error("Invalid RPC URL");
  return async (method, params = []) => {
    const response = await fetch(endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
      signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) throw new Error(`RPC HTTP ${response.status}`);
    const value = await response.json();
    if (value.error || value.result === undefined)
      throw new Error(`RPC ${method} failed`);
    return value.result;
  };
}

/** Read-only deployment qualification. Never edits a Policy, Profile or binary. */
export async function inspectDevnet(config, rpc) {
  const expected = enrollmentConfig(config, 1);
  if ((await rpc("getGenesisHash")) !== DEVNET.genesisHash)
    throw new Error("RPC is not Devnet");
  const core = new TekmerionClient("devnet");
  const profileKey = profileAddress(config.policy, "devnet")[0];
  const keys = [profileKey, config.policy, DEVNET.forwarderState];
  const observed = await rpc("getMultipleAccounts", [
    keys,
    { encoding: "base64", commitment: "finalized" },
  ]);
  const [profileAccount, policyAccount, stateAccount] = observed.value;
  if (
    !profileAccount ||
    profileAccount.owner !== ADAPTER_IDS.devnet ||
    profileAccount.executable
  ) {
    throw new Error(
      "Deploy the Auditor and enroll this Policy's Profile first",
    );
  }
  if (
    !stateAccount ||
    stateAccount.owner !== DEVNET.forwarderProgram ||
    stateAccount.executable
  ) {
    throw new Error("The official mock Forwarder state is unavailable");
  }
  const profileBytes = Buffer.from(profileAccount.data[0], "base64");
  const profile = decodeConfidentialProfile(profileBytes);
  for (const key of [
    "forwarderProgram",
    "forwarderState",
    "workflowId",
    "agentConfigHash",
  ]) {
    if (profile.config[key] !== expected[key])
      throw new Error(`Profile ${key} does not match CRE simulation`);
  }
  if (profile.policy !== config.policy)
    throw new Error("Profile belongs to another Policy");
  if (!policyAccount) throw new Error("Policy is missing");
  const slot = BigInt(observed.context.slot);
  const { policy } = core.readPolicy(
    { slot, unixTimestamp: 0n },
    new PublicKey(config.policy),
    {
      key: new PublicKey(config.policy),
      owner: new PublicKey(policyAccount.owner),
      executable: policyAccount.executable,
      data: Buffer.from(policyAccount.data[0], "base64"),
      slot,
    },
  );
  if (
    policy.rules.auditors.length !== 1 ||
    policy.rules.auditors[0].authority.toBase58() !== profileKey
  ) {
    throw new Error(
      "Policy must select this Chainlink Profile as its sole auditor",
    );
  }
  const feeDestination = PublicKey.findProgramAddressSync(
    [
      policy.rules.auditors[0].feeRecipient.toBuffer(),
      TOKEN_PROGRAM_ID.toBuffer(),
      core.mint.toBuffer(),
    ],
    new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"),
  )[0].toBase58();
  return {
    profile,
    profileKey,
    profileHash: sha256(profileBytes),
    feeDestination,
    slot,
  };
}

export async function createRelease(config, rpc, gatewayUrl, consumerProgram) {
  if (!consumerProgram)
    throw new Error("Set CRE_CONSUMER_PROGRAM to the integrating program ID");
  new PublicKey(consumerProgram);
  const snapshot = await inspectDevnet(config, rpc);
  const core = new TekmerionClient("devnet");
  const programs = [
    core.programId.toBase58(),
    ADAPTER_IDS.devnet,
    DEVNET.forwarderProgram,
    consumerProgram,
  ];
  const keys = programs.flatMap((program) => [
    program,
    PublicKey.findProgramAddressSync(
      [new PublicKey(program).toBuffer()],
      new PublicKey(loader),
    )[0].toBase58(),
  ]);
  const observed = await rpc("getMultipleAccounts", [
    keys,
    {
      encoding: "base64",
      commitment: "finalized",
      minContextSlot: Number(snapshot.slot),
    },
  ]);
  const pins = programs.map((program, index) => {
    const [account, data] = observed.value.slice(index * 2, index * 2 + 2);
    if (
      !account?.executable ||
      account.owner !== loader ||
      data?.owner !== loader ||
      data.executable
    ) {
      throw new Error(`Deploy program ${program} on Devnet first`);
    }
    const programBytes = Buffer.from(account.data[0], "base64");
    const bytes = Buffer.from(data.data[0], "base64");
    if (
      programBytes.length !== 36 ||
      programBytes.readUInt32LE() !== 2 ||
      new PublicKey(programBytes.subarray(4)).toBase58() !==
        keys[index * 2 + 1] ||
      bytes.length <= 45 ||
      bytes.readUInt32LE() !== 3 ||
      ![0, 1].includes(bytes[12])
    ) {
      throw new Error(`Invalid ProgramData for ${program}`);
    }
    return {
      program,
      binaryHash: sha256(bytes.subarray(45)),
      upgradeAuthority: bytes[12]
        ? new PublicKey(bytes.subarray(13, 45)).toBase58()
        : null,
    };
  });
  return {
    version: 3,
    programs: pins,
    network: "devnet",
    genesisHash: DEVNET.genesisHash,
    policy: config.policy,
    profile: snapshot.profileKey,
    profileHash: snapshot.profileHash,
    workflowId: DEVNET.workflowId,
    agentConfigHash: agentConfigHash(config.agent),
    forwarderProgram: DEVNET.forwarderProgram,
    forwarderState: DEVNET.forwarderState,
    feeDestination: snapshot.feeDestination,
    triggerAddress: config.triggerKey,
    gatewayUrl,
  };
}
