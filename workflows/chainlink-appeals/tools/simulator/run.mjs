import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { parseWorkflowConfig } from "../../dist/workflow/config.js";
import { configure } from "../../docker/configure.mjs";
import { DEVNET, inspectDevnet, rpcClient } from "./devnet.mjs";
import { SimulationQueue } from "./queue.mjs";
import { simulationServer } from "./server.mjs";
import { runProcess } from "./process.mjs";

const workflow = fileURLToPath(new URL("../..", import.meta.url));
const configFile = "/run/cre/config.json";
const config = parseWorkflowConfig(
  JSON.parse(await readFile(configFile, "utf8")),
);
if (!process.env.CRE_SOLANA_PRIVATE_KEY)
  throw new Error("Set CRE_SOLANA_PRIVATE_KEY to a test wallet");
if (!process.env.TEKMERION_MODEL_KEY?.trim())
  throw new Error("Set TEKMERION_MODEL_KEY before starting the simulator");
await inspectDevnet(config, rpcClient(process.env.CRE_SOLANA_RPC_URL));
const runtime = join(process.env.HOME, "runtime");
await configure(configFile, runtime, process.env.CRE_SOLANA_RPC_URL);
const shutdown = new AbortController();
const queue = new SimulationQueue(
  join(process.env.HOME, "simulations"),
  (job) =>
    runProcess(
      "cre",
      [
        "workflow",
        "simulate",
        ".",
        "--non-interactive",
        "--target",
        "staging-settings",
        "--env",
        join(runtime, ".env"),
        "--wasm",
        join(workflow, "chainlink-appeals.wasm"),
        "--broadcast",
        "--trigger-index",
        "0",
        "--http-payload",
        JSON.stringify({ case: job.case }),
        ...(new URL(process.env.CRE_SOLANA_RPC_URL).protocol === "http:"
          ? ["--allow-insecure-rpc"]
          : []),
      ],
      { cwd: runtime, signal: shutdown.signal },
    ),
);
const server = simulationServer(config, DEVNET.workflowId, queue);
server.requestTimeout = 10000;
for (const signal of ["SIGTERM", "SIGINT"]) {
  process.once(signal, () => {
    queue.stop();
    server.close();
    shutdown.abort();
  });
}
try {
  await runProcess("cre", ["whoami", "--non-interactive"], {
    cwd: runtime,
    signal: shutdown.signal,
    timeoutMs: 30000,
    stdio: "ignore",
  });
} catch {
  throw new Error(
    "CRE authentication unavailable; run cre-login or check CRE_API_KEY and connectivity",
  );
}
await queue.start();
if (!shutdown.signal.aborted) {
  server.listen(2000, "0.0.0.0", () =>
    console.log("Devnet CRE simulation gateway listening on port 2000"),
  );
}
