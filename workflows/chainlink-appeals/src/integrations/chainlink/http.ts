/** Native CRE HTTP requests with a shared call budget and bounded responses. */
import { cre, type TeeRuntime } from "@chainlink/cre-sdk";
import { base64 } from "../../shared/bytes.js";

export interface HttpRequest {
  url: string;
  method: "GET" | "POST" | "PUT";
  body?: Uint8Array;
  headers: Record<string, string>;
  maxResponseBytes: number;
}

export interface HttpResponse {
  status: number;
  body: Uint8Array;
}

export class HttpLimitError extends Error {
  constructor(readonly limit: "calls" | "response") {
    super(`CRE HTTP ${limit} limit exceeded`);
  }
}

export function createHttpClient<Config>(
  runtime: TeeRuntime<Config>,
  limits: { maxCalls: number; timeoutSeconds: number },
): (request: HttpRequest) => HttpResponse {
  const client = new cre.capabilities.HTTPClient();
  let calls = 0;

  return (request) => {
    if (++calls > limits.maxCalls) throw new HttpLimitError("calls");
    const reply = client
      .sendRequest(runtime, {
        url: request.url,
        method: request.method,
        multiHeaders: Object.fromEntries(
          Object.entries(request.headers).map(([key, value]) => [
            key,
            { values: [value] },
          ]),
        ),
        ...(request.body ? { body: base64(request.body) } : {}),
        timeout: `${limits.timeoutSeconds}s`,
      })
      .result();
    if (reply.body.length > request.maxResponseBytes)
      throw new HttpLimitError("response");
    return { status: reply.statusCode, body: reply.body };
  };
}
