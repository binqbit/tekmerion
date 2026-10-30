import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseWorkflowConfig } from "../dist/workflow/config.js";

export async function configure(configPath, runtimeDirectory, rpcUrl) {
  const config = parseWorkflowConfig(
    JSON.parse(await readFile(configPath, "utf8")),
  );
  const rpc = new URL(rpcUrl);
  if (!["https:", "http:"].includes(rpc.protocol)) {
    throw new Error("CRE_SOLANA_RPC_URL must use HTTP or HTTPS");
  }
  const workflow = fileURLToPath(new URL("..", import.meta.url));
  const target = "staging-settings";
  await mkdir(runtimeDirectory, { recursive: true, mode: 0o700 });
  // JSON is valid YAML; quoting also preserves RPC URLs with YAML punctuation.
  await writeFile(
    join(runtimeDirectory, "project.yaml"),
    JSON.stringify({
      [target]: {
        rpcs: [{ "chain-name": `solana-${config.network}`, url: rpc.href }],
      },
    }),
    { mode: 0o600 },
  );
  await writeFile(
    join(runtimeDirectory, "workflow.yaml"),
    JSON.stringify({
      [target]: {
        "user-workflow": { "workflow-name": "tekmerion-local-simulation" },
        "workflow-artifacts": {
          "workflow-path": join(workflow, "src/main.ts"),
          "config-path": configPath,
          "secrets-path": join(workflow, "secrets.yaml"),
        },
      },
    }),
    { mode: 0o600 },
  );
  await writeFile(join(runtimeDirectory, ".env"), "", { mode: 0o600 });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    await configure(
      process.argv[2],
      process.argv[3],
      process.env.CRE_SOLANA_RPC_URL,
    );
  } catch (error) {
    console.error(`Cannot configure CRE simulation: ${error.message}`);
    process.exitCode = 1;
  }
}
