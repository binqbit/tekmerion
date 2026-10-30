#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import { parseWorkflowConfig } from "../dist/workflow/config.js";
import { agentConfigHash } from "../dist/research/config.js";

try {
  const config = parseWorkflowConfig(
    JSON.parse(await readFile(process.argv[2], "utf8")),
  );
  console.log(
    JSON.stringify(
      {
        network: config.network,
        policy: config.policy,
        agentConfigHash: agentConfigHash(config.agent),
      },
      null,
      2,
    ),
  );
} catch (error) {
  console.error("Invalid configuration:", error.message);
  process.exitCode = 1;
}
