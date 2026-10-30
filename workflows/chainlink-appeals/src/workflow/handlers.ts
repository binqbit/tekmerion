/** HTTP selects one Case; cron discovers eligible Cases. Both run the same audit. */
import { cre, type HTTPPayload, type TeeRuntime } from "@chainlink/cre-sdk";
import { pendingRequest } from "../integrations/solana/discovery.js";
import { parseWorkflowConfig, type WorkflowConfig } from "./config.js";
import { runAudit } from "./run.js";
import { parseTrigger } from "./trigger.js";

export function onHttpTrigger(
  runtime: TeeRuntime<WorkflowConfig>,
  payload: HTTPPayload,
): string {
  return runAudit(runtime, parseTrigger(payload.input));
}

export function onCronTrigger(runtime: TeeRuntime<WorkflowConfig>): string {
  const settings = parseWorkflowConfig(runtime.config);
  const target = pendingRequest(
    runtime.usingTheDons(),
    settings.network,
    settings.policy,
    settings.agent,
  );
  return target ? runAudit(runtime, target) : "No eligible prepared request";
}

export function createWorkflowHandlers(config: WorkflowConfig) {
  parseWorkflowConfig(config);
  return [
    cre.handlerInTee(
      new cre.capabilities.HTTPCapability().trigger({
        authorizedKeys: [
          { type: "KEY_TYPE_ECDSA_EVM", publicKey: config.triggerKey },
        ],
      }),
      onHttpTrigger,
      [{ tee: "nitro", regions: ["us-west-2"] }],
    ),
    cre.handlerInTee(
      new cre.capabilities.CronCapability().trigger({
        schedule: "0 * * * * *",
      }),
      onCronTrigger,
      [{ tee: "nitro", regions: ["us-west-2"] }],
    ),
  ];
}
