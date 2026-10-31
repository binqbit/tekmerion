import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { once } from "node:events";
import { test } from "node:test";
import { privateKeyToAccount } from "viem/accounts";
import { authenticate } from "../../tools/simulator/auth.mjs";
import { SimulationQueue } from "../../tools/simulator/queue.mjs";
import { simulationServer } from "../../tools/simulator/server.mjs";

const account = privateKeyToAccount("0x" + "12".repeat(32));
const config = { triggerKey: account.address };
const workflowId = "1".repeat(64);
const caseKey = "7kuEAA3mSC1Tz8gQjnvH7bKFda9xSPRRin9SZbH49cNK";
const now = () => Math.floor(Date.now() / 1000);
const deferred = () => {
  let resolve;
  const promise = new Promise((r) => {
    resolve = r;
  });
  return { promise, resolve };
};
async function signed(patch = {}, claimsPatch = {}) {
  const request = {
    jsonrpc: "2.0",
    id: randomUUID(),
    method: "workflows.execute",
    params: { input: { case: caseKey }, workflow: { workflowID: workflowId } },
    ...patch,
  };
  const body = JSON.stringify(request);
  const b64 = (v) => Buffer.from(JSON.stringify(v)).toString("base64url");
  const message =
    b64({ alg: "ETH", typ: "JWT" }) +
    "." +
    b64({
      digest: "0x" + createHash("sha256").update(body).digest("hex"),
      iss: account.address,
      iat: now(),
      exp: now() + 60,
      jti: request.id,
      ...claimsPatch,
    });
  const signature = await account.signMessage({ message });
  return {
    request,
    body,
    authorization:
      "Bearer " +
      message +
      "." +
      Buffer.from(signature.slice(2), "hex").toString("base64url"),
  };
}

test("simulator authenticates the backend JWT and rejects input overrides and replayed bodies", async () => {
  const good = await signed();
  assert.deepEqual(
    await authenticate(
      good.body,
      good.authorization,
      config,
      workflowId,
      now(),
    ),
    { id: good.request.id, case: caseKey },
  );
  await assert.rejects(
    authenticate(
      good.body + " ",
      good.authorization,
      config,
      workflowId,
      now(),
    ),
  );
  await assert.rejects(
    authenticate(good.body, good.authorization, config, "2".repeat(64), now()),
  );
  await assert.rejects(
    authenticate(
      good.body,
      good.authorization,
      { triggerKey: "0x" + "33".repeat(20) },
      workflowId,
      now(),
    ),
  );
  for (const [patch, claims] of [
    [{}, { exp: now() - 1 }],
    [{}, { exp: now() + 3600 }],
    [
      {
        params: {
          input: { case: caseKey, verdict: "yes" },
          workflow: { workflowID: workflowId },
        },
      },
      {},
    ],
  ]) {
    const invalid = await signed(patch, claims);
    await assert.rejects(
      authenticate(
        invalid.body,
        invalid.authorization,
        config,
        workflowId,
        now(),
      ),
    );
  }
});

test("gateway persists admissions, authenticates HTTP and executes duplicate requests only once", async () => {
  const directory = await mkdtemp(join(tmpdir(), "cre-queue-"));
  const gate = deferred();
  const started = deferred();
  const executed = [];
  const queue = new SimulationQueue(directory, async (job) => {
    executed.push(job.id);
    started.resolve();
    await gate.promise;
  });
  await queue.start();
  const server = simulationServer(config, workflowId, queue);
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const url = `http://127.0.0.1:${server.address().port}/`;
  try {
    const good = await signed();
    const send = (authorization) =>
      fetch(url, {
        method: "POST",
        body: good.body,
        headers: {
          "Content-Type": "application/json",
          Authorization: authorization,
        },
      });
    assert.equal((await send("Bearer invalid")).status, 400);
    const accepted = await send(good.authorization);
    assert.equal(accepted.status, 202);
    assert.equal(
      (await accepted.json()).result.workflow_execution_id,
      `simulation:${good.request.id}`,
    );
    await started.promise;
    assert.equal((await send(good.authorization)).status, 202);
    gate.resolve();
    await queue.finished;
    assert.deepEqual(executed, [good.request.id]);
    assert.equal(
      JSON.parse(
        await readFile(join(directory, `${good.request.id}.json`), "utf8"),
      ).state,
      "succeeded",
    );
  } finally {
    gate.resolve();
    queue.stop();
    await queue.finished;
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    await rm(directory, { recursive: true, force: true });
  }
});

test("restart resumes queued jobs but never repeats an interrupted or completed execution", async () => {
  const directory = await mkdtemp(join(tmpdir(), "cre-recovery-"));
  const jobs = ["running", "queued", "succeeded"].map((state) => ({
    id: randomUUID(),
    case: caseKey,
    state,
    createdAt: Date.now(),
  }));
  try {
    for (const job of jobs)
      await writeFile(join(directory, `${job.id}.json`), JSON.stringify(job));
    const executed = [];
    const queue = new SimulationQueue(directory, async (job) => {
      executed.push(job.id);
    });
    await queue.start();
    await queue.finished;
    assert.deepEqual(executed, [jobs[1].id]);
    assert.equal(
      JSON.parse(await readFile(join(directory, `${jobs[0].id}.json`), "utf8"))
        .state,
      "interrupted",
    );
    for (const job of jobs) await queue.enqueue({ id: job.id, case: caseKey });
    await queue.finished;
    assert.deepEqual(executed, [jobs[1].id]);
    await assert.rejects(
      queue.enqueue({
        id: jobs[0].id,
        case: "11111111111111111111111111111111",
      }),
    );
    queue.stop();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("a failed execution does not discard later queued cases", async () => {
  const directory = await mkdtemp(join(tmpdir(), "cre-failure-"));
  const executed = [];
  const queue = new SimulationQueue(directory, async (job) => {
    executed.push(job.id);
    if (executed.length === 1) throw new Error("CLI failure");
  });
  try {
    await queue.start();
    await queue.enqueue({ id: randomUUID(), case: caseKey });
    await queue.enqueue({ id: randomUUID(), case: caseKey });
    while (queue.running || queue.pending.length) await queue.finished;
    assert.equal(executed.length, 2);
  } finally {
    queue.stop();
    await rm(directory, { recursive: true, force: true });
  }
});

test("shutdown during durable admission leaves the job queued without starting a CLI", async () => {
  const directory = await mkdtemp(join(tmpdir(), "cre-stop-"));
  const executing = [];
  const queue = new SimulationQueue(directory, async (job) =>
    executing.push(job.id),
  );
  const persisted = deferred();
  const release = deferred();
  const save = queue.save.bind(queue);
  queue.save = async (job) => {
    await save(job);
    if (job.state === "running") {
      persisted.resolve();
      await release.promise;
    }
  };
  try {
    await queue.start();
    const job = await queue.enqueue({ id: randomUUID(), case: caseKey });
    await persisted.promise;
    queue.stop();
    release.resolve();
    await queue.finished;
    assert.deepEqual(executing, []);
    assert.equal(
      JSON.parse(await readFile(queue.path(job.id), "utf8")).state,
      "queued",
    );
  } finally {
    release.resolve();
    queue.stop();
    await queue.finished;
    await rm(directory, { recursive: true, force: true });
  }
});

test("a failed durable status write stops the queue and exposes unhealthy readiness", async () => {
  const directory = await mkdtemp(join(tmpdir(), "cre-storage-failure-"));
  const queue = new SimulationQueue(directory, async () => {});
  const save = queue.save.bind(queue);
  queue.save = (job) =>
    job.state === "succeeded"
      ? Promise.reject(new Error("disk full"))
      : save(job);
  const server = simulationServer(config, workflowId, queue);
  try {
    await queue.start();
    const job = await queue.enqueue({ id: randomUUID(), case: caseKey });
    await queue.finished;
    assert.equal(queue.closed, true);
    assert.equal(queue.failure.message, "disk full");
    // The durable record still prevents another execution after a restart.
    assert.equal(
      JSON.parse(await readFile(queue.path(job.id), "utf8")).state,
      "running",
    );
    await assert.rejects(
      queue.enqueue({ id: randomUUID(), case: caseKey }),
      /stopping/,
    );
    server.listen(0, "127.0.0.1");
    await once(server, "listening");
    const response = await fetch(
      `http://127.0.0.1:${server.address().port}/health`,
    );
    assert.equal(response.status, 503);
  } finally {
    queue.stop();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    await rm(directory, { recursive: true, force: true });
  }
});
