import { createServer } from "node:http";
import { authenticate } from "./auth.mjs";

export function simulationServer(config, workflowId, queue) {
  return createServer(async (request, response) => {
    const reply = (status, body) => {
      response.writeHead(status, { "Content-Type": "application/json" });
      response.end(JSON.stringify(body));
    };
    if (request.method === "GET" && request.url === "/health") {
      return reply(queue.closed ? 503 : 200, {
        mode: "cre_simulation",
        network: "devnet",
      });
    }
    if (request.method !== "POST" || request.url !== "/") return reply(404, {});
    let body = "";
    try {
      if (!request.headers["content-type"]?.startsWith("application/json")) {
        return reply(415, {});
      }
      for await (const chunk of request) {
        body += chunk;
        if (Buffer.byteLength(body) > 4096) return reply(413, {});
      }
      const job = await authenticate(
        body,
        request.headers.authorization,
        config,
        workflowId,
        Math.floor(Date.now() / 1000),
      );
      await queue.enqueue(job);
      reply(202, {
        jsonrpc: "2.0",
        id: job.id,
        result: {
          status: "ACCEPTED",
          workflow_id: workflowId,
          // Admission identity only. The actual CRE execution is authenticated on-chain.
          workflow_execution_id: `simulation:${job.id}`,
        },
      });
    } catch {
      reply(400, { error: "Simulation request was not accepted" });
    }
  });
}
