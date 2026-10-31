import { mkdir, open, readFile, readdir, rename } from "node:fs/promises";
import { join } from "node:path";

/** Durable admissions, one CLI process at a time; interrupted runs are never replayed. */
export class SimulationQueue {
  pending = [];
  running = false;
  closed = false;
  failure = null;
  admissions = Promise.resolve();

  constructor(directory, execute) {
    this.directory = directory;
    this.execute = execute;
  }

  async start() {
    await mkdir(this.directory, { recursive: true, mode: 0o700 });
    for (const name of (await readdir(this.directory)).sort()) {
      if (!name.endsWith(".json")) continue;
      const job = JSON.parse(
        await readFile(join(this.directory, name), "utf8"),
      );
      if (job.state === "running") {
        await this.save({ ...job, state: "interrupted" });
      } else if (job.state === "queued") this.pending.push(job);
    }
    this.pending.sort((left, right) => left.createdAt - right.createdAt);
    this.pump();
  }

  enqueue(request) {
    const operation = this.admissions.then(async () => {
      if (this.closed) throw new Error("Simulator is stopping");
      try {
        const existing = JSON.parse(
          await readFile(this.path(request.id), "utf8"),
        );
        if (existing.case !== request.case)
          throw new Error("Admission identity mismatch");
        return existing;
      } catch (error) {
        if (error.code !== "ENOENT") throw error;
      }
      if (this.pending.length >= 64)
        throw new Error("Simulation queue is full");
      const job = { ...request, state: "queued", createdAt: Date.now() };
      try {
        await this.save(job);
      } catch (error) {
        this.fail(error);
        throw error;
      }
      this.pending.push(job);
      this.pump();
      return job;
    });
    this.admissions = operation.catch(() => {});
    return operation;
  }

  path(id) {
    return join(this.directory, `${id}.json`);
  }

  async save(job) {
    const temporary = this.path(job.id) + ".tmp";
    const file = await open(temporary, "w", 0o600);
    try {
      await file.writeFile(JSON.stringify(job));
      await file.sync();
    } finally {
      await file.close();
    }
    await rename(temporary, this.path(job.id));
    const directory = await open(this.directory, "r");
    try {
      await directory.sync();
    } finally {
      await directory.close();
    }
  }

  pump() {
    if (this.running || this.closed) return;
    this.running = true;
    this.finished = this.drain()
      .catch((error) => {
        // A storage failure makes the launch history uncertain. Stop accepting
        // work; never rerun an execution to repair a failed status write.
        this.fail(error);
      })
      .finally(() => {
        this.running = false;
        if (this.pending.length && !this.closed) this.pump();
      });
  }

  async drain() {
    while (this.pending.length && !this.closed) {
      const job = this.pending.shift();
      await this.save({ ...job, state: "running" });
      if (this.closed) {
        // Shutdown may arrive while the durable launch marker is being written.
        // Nothing has executed yet, so this job can safely resume after restart.
        await this.save({ ...job, state: "queued" });
        break;
      }
      let state;
      try {
        await this.execute(job);
        state = "succeeded";
      } catch {
        state = this.closed ? "interrupted" : "failed";
      }
      await this.save({ ...job, state });
    }
  }

  stop() {
    this.closed = true;
  }

  fail(error) {
    this.failure = error;
    this.stop();
    console.error(
      "Simulation queue storage failed; repair storage before restarting",
    );
  }
}
