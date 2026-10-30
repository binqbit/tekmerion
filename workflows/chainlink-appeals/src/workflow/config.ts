/** Deployment routing is separate from the committed semantic agent policy. */
import { address, check } from "@tekmerion/sdk/wire";
import type { ChainlinkNetwork } from "@tekmerion/chainlink-sdk/wire";
import { object } from "../shared/json.js";
import { publicUrl } from "../shared/url.js";
import { parseAgentConfig, type AgentConfig } from "../research/config.js";

export interface WorkflowConfig {
  network: ChainlinkNetwork;
  policy: string;
  archiveUrl?: string;
  triggerKey: string;
  agent: AgentConfig;
}

export function parseWorkflowConfig(value: unknown): WorkflowConfig {
  const settings = object(
    value,
    ["network", "policy", "triggerKey", "agent"],
    ["archiveUrl"],
  );
  check(
    settings.network === "mainnet" || settings.network === "devnet",
    "Unsupported network",
  );
  address(String(settings.policy));
  if (settings.archiveUrl !== undefined) {
    check(typeof settings.archiveUrl === "string", "Invalid archive URL");
    if (settings.archiveUrl !== "") publicUrl(settings.archiveUrl);
  }
  check(
    typeof settings.triggerKey === "string" &&
      /^0x[0-9a-fA-F]{40}$/.test(settings.triggerKey),
    "Invalid trigger key",
  );
  parseAgentConfig(settings.agent);
  return value as WorkflowConfig;
}
