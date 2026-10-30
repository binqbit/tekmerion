import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { configure } from "../../docker/configure.mjs";

const example = JSON.parse(
  await readFile(new URL("../../config.example.json", import.meta.url), "utf8"),
);
const config = {
  ...example,
  policy: "11111111111111111111111111111111",
  triggerKey: "0x" + "22".repeat(20),
};

async function withFiles(run) {
  const directory = await mkdtemp(join(tmpdir(), "tekmerion-cre-"));
  try {
    await run(
      directory,
      join(directory, "config.json"),
      join(directory, "runtime"),
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

test("Docker routing follows the configured network and preserves the committed agent config", async () => {
  await withFiles(async (_directory, file, runtime) => {
    for (const network of ["devnet", "mainnet"]) {
      const original = JSON.stringify({ ...config, network });
      await writeFile(file, original);
      const rpc = "https://rpc.example/path?token=value&label=a:b#fragment";
      await configure(file, runtime, rpc);
      const project = JSON.parse(
        await readFile(join(runtime, "project.yaml"), "utf8"),
      );
      const workflow = JSON.parse(
        await readFile(join(runtime, "workflow.yaml"), "utf8"),
      );
      assert.deepEqual(project["staging-settings"].rpcs, [
        { "chain-name": `solana-${network}`, url: rpc },
      ]);
      assert.equal(
        workflow["staging-settings"]["workflow-artifacts"]["config-path"],
        file,
      );
      assert.equal(await readFile(file, "utf8"), original);
      assert.equal(await readFile(join(runtime, ".env"), "utf8"), "");
    }
  });
});

test("Docker setup rejects missing or non-HTTP RPC before creating runtime files", async () => {
  await withFiles(async (directory, file, runtime) => {
    await writeFile(file, JSON.stringify(config));
    for (const rpc of [
      undefined,
      "",
      "file:///etc/passwd",
      "ws://rpc.example",
    ]) {
      await assert.rejects(configure(file, runtime, rpc));
    }
    assert.deepEqual(await readdir(directory), ["config.json"]);
  });
});

test("Docker setup applies the real workflow configuration validation", async () => {
  await withFiles(async (directory, file, runtime) => {
    await writeFile(file, JSON.stringify(example));
    await assert.rejects(
      configure(file, runtime, "https://api.devnet.solana.com"),
    );
    assert.deepEqual(await readdir(directory), ["config.json"]);
  });
});
