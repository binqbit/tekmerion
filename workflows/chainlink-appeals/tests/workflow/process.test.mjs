import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { test } from "node:test";
import { runProcess } from "../../tools/simulator/process.mjs";

const options = (signal = new AbortController().signal) => ({
  signal,
  stdio: "ignore",
  timeoutMs: 5000,
  graceMs: 50,
});

test("CLI runner distinguishes success, failure and missing executable", async () => {
  await runProcess(process.execPath, ["-e", "process.exit(0)"], options());
  await assert.rejects(
    runProcess(process.execPath, ["-e", "process.exit(2)"], options()),
    /failed/,
  );
  await assert.rejects(runProcess("/missing/cre", [], options()), {
    code: "ENOENT",
  });
});

test("a cancelled or timed out CLI cannot be recorded as success", async () => {
  const controller = new AbortController();
  controller.abort();
  assert.throws(
    () => runProcess("/must-not-be-launched", [], options(controller.signal)),
    { name: "AbortError" },
  );
  await assert.rejects(
    runProcess(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
      ...options(),
      timeoutMs: 150,
    }),
    /interrupted/,
  );
});

test("shutdown reaps the CLI even when its SIGTERM handler exits successfully", async () => {
  const directory = await mkdtemp(join(tmpdir(), "cre-process-"));
  const marker = join(directory, "ready");
  const controller = new AbortController();
  const run = runProcess(
    process.execPath,
    [
      "-e",
      `
    require('node:fs').writeFileSync(process.argv[1], String(process.pid));
    process.on('SIGTERM', () => process.exit(0));
    setInterval(() => {}, 1000);
  `,
      marker,
    ],
    options(controller.signal),
  );
  const result = assert.rejects(run, /interrupted/);
  try {
    let pid;
    for (let i = 0; i < 200 && !pid; i++) {
      try {
        pid = Number(await readFile(marker, "utf8"));
      } catch (error) {
        if (error.code !== "ENOENT") throw error;
      }
      if (!pid) await delay(10);
    }
    assert.ok(pid, "CLI started");
    controller.abort();
    await result;
    assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
  } finally {
    controller.abort();
    await result;
    await rm(directory, { recursive: true, force: true });
  }
});
