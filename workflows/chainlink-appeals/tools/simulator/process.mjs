import { spawn } from "node:child_process";

/** Run one CLI process group; cancellation must finish before another job starts. */
export function runProcess(
  command,
  args,
  { cwd, signal, timeoutMs = 480000, graceMs = 10000, stdio = "inherit" },
) {
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio, detached: true });
    let stopping = false;
    let forcedStop;
    const kill = (name) => {
      if (!child.pid) return;
      try {
        // CRE may have subprocesses. Signalling only its PID can orphan them.
        process.kill(-child.pid, name);
      } catch (error) {
        if (error.code !== "ESRCH") throw error;
      }
    };
    const stop = () => {
      if (stopping) return;
      stopping = true;
      kill("SIGTERM");
      forcedStop = setTimeout(() => kill("SIGKILL"), graceMs).unref();
    };
    const deadline = setTimeout(stop, timeoutMs).unref();
    signal.addEventListener("abort", stop, { once: true });
    const cleanup = () => {
      clearTimeout(deadline);
      clearTimeout(forcedStop);
      signal.removeEventListener("abort", stop);
    };
    child.once("error", (error) => {
      cleanup();
      reject(error);
    });
    child.once("close", (code) => {
      if (stopping) kill("SIGKILL");
      cleanup();
      if (stopping) reject(new Error("CRE execution interrupted"));
      else if (code !== 0) reject(new Error("CRE execution failed"));
      else resolve();
    });
  });
}
