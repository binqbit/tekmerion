/** Compose native transports around the audit lifecycle for one enrolled Case. */
import type { TeeRuntime } from "@chainlink/cre-sdk";
import { check } from "@tekmerion/sdk/wire";
import { executeAudit } from "../audit/execution.js";
import { createAuditHttp } from "../integrations/http.js";
import { readBinding } from "../integrations/solana/accounts.js";
import { createReportChannel } from "../integrations/solana/reports.js";
import { canonical } from "../shared/json.js";
import { parseWorkflowConfig, type WorkflowConfig } from "./config.js";
import type { RequestTrigger } from "./trigger.js";

export function runAudit(
  runtime: TeeRuntime<WorkflowConfig>,
  target: RequestTrigger,
): string {
  const settings = parseWorkflowConfig(runtime.config);
  const read = () =>
    readBinding(runtime.usingTheDons(), settings.network, target.case);
  const initial = read();
  check(
    initial.binding.case.policy === settings.policy,
    "Wrong enrolled Policy",
  );

  const http = createAuditHttp(runtime, settings);
  const reports = createReportChannel(
    runtime,
    settings.network,
    target.case,
    initial.binding,
  );
  const result = executeAudit(
    {
      read,
      executionId: reports.executionId,
      deliver: reports.deliver,
      publish: http.publish,
      transport: http.transport,
    },
    settings.agent,
  );

  return canonical({
    case: target.case,
    attempt: result.attemptIndex,
    stage: result.stage,
  });
}
