/** CRE entrypoint: parse deployment settings and register the audit triggers. */
import { Runner } from "@chainlink/cre-sdk";
import { decodeText } from "./shared/bytes.js";
import { parseWorkflowConfig, type WorkflowConfig } from "./workflow/config.js";
import { createWorkflowHandlers } from "./workflow/handlers.js";

export async function main(): Promise<void> {
  const runner = await Runner.newRunner<WorkflowConfig>({
    configParser: (bytes) => parseWorkflowConfig(JSON.parse(decodeText(bytes))),
  });
  await runner.run(createWorkflowHandlers);
}
