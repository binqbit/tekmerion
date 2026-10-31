#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import { parseWorkflowConfig } from "../dist/workflow/config.js";
import { privateKeyToAccount } from "viem/accounts";
import {
  createRelease,
  enrollmentConfig,
  rpcClient,
} from "./simulator/devnet.mjs";

try {
  const [command, file, value] = process.argv.slice(2);
  if (command === "trigger-address") {
    const key = (await readFile(file, "utf8")).trim().replace(/^0x/, "");
    if (!/^[0-9a-fA-F]{64}$/.test(key))
      throw new Error("Trigger key must contain 32 bytes in hexadecimal");
    let address;
    try {
      address = privateKeyToAccount(`0x${key}`).address;
    } catch {
      throw new Error("Invalid trigger signing key");
    }
    console.log(address);
    process.exit(0);
  }
  const config = parseWorkflowConfig(JSON.parse(await readFile(file, "utf8")));
  const result =
    command === "enrollment"
      ? enrollmentConfig(config, Number(value))
      : command === "release"
        ? await createRelease(
            config,
            rpcClient(process.env.CRE_SOLANA_RPC_URL),
            value ?? "http://cre-simulator:2000/",
            process.env.CRE_CONSUMER_PROGRAM,
          )
        : (() => {
            throw new Error(
              "Usage: devnet.mjs enrollment CONFIG MAX_ATTEMPTS | release CONFIG [GATEWAY_URL]",
            );
          })();
  console.log(JSON.stringify(result, null, 2));
} catch (error) {
  console.error(`Devnet configuration failed: ${error.message}`);
  process.exitCode = 1;
}
