/** TEE networking: fixed provider endpoint, bounded requests and lazy credentials. */
import type { TeeRuntime } from "@chainlink/cre-sdk";
import { check } from "@tekmerion/sdk/wire";
import {
  createHttpClient,
  HttpLimitError,
  type HttpRequest,
} from "./chainlink/http.js";
import type { AgentConfig } from "../research/config.js";
import { MODEL_ENDPOINT } from "../research/openai/request.js";
import { AuditFailure, FailureCode } from "../research/failure.js";
import type { ResearchTransport, Response } from "../research/transport.js";

interface AuditHttpSettings {
  agent: AgentConfig;
  archiveUrl?: string;
}

export function createAuditHttp<Config>(
  runtime: TeeRuntime<Config>,
  settings: AuditHttpSettings,
): {
  transport: ResearchTransport;
  publish(hash: string, bytes: Uint8Array): void;
} {
  // Document fallbacks, optional research, one verdict and one report upload.
  const request = createHttpClient(runtime, {
    maxCalls: 2 * settings.agent.gatewayOrigins.length + 3,
  });
  let archiveKey: string | null | undefined;

  function send(input: HttpRequest): Response {
    try {
      return request(input);
    } catch (error) {
      if (!(error instanceof HttpLimitError)) throw error;
      if (error.limit === "calls")
        throw new AuditFailure(FailureCode.BudgetExceeded);
      if (input.method === "GET")
        throw new AuditFailure(FailureCode.SourceUnavailable);
      throw new AuditFailure(FailureCode.InvalidModelOutput);
    }
  }

  function callModel(
    body: Uint8Array,
    stage: "research" | "decision",
  ): Response {
    let key: string;
    try {
      key = runtime.getSecret({ id: "MODEL_KEY" }).result().value;
      check(key.length > 0, "Empty model credential");
    } catch {
      throw new AuditFailure(FailureCode.ModelCredentialsRejected);
    }
    return send({
      url: MODEL_ENDPOINT,
      method: "POST",
      body,
      headers: {
        "Content-Type": "application/json",
        Authorization: "Bearer " + key,
      },
      maxResponseBytes: settings.agent.maxResponseBytes,
      timeoutSeconds:
        stage === "research"
          ? settings.agent.researchTimeoutSeconds
          : settings.agent.decisionTimeoutSeconds,
    });
  }

  function publish(hash: string, bytes: Uint8Array): void {
    if (!settings.archiveUrl) return;
    if (archiveKey === undefined) {
      try {
        archiveKey =
          runtime.getSecret({ id: "ARCHIVE_KEY" }).result().value.trim() ||
          null;
      } catch {
        archiveKey = null;
      }
    }
    if (archiveKey === null) return;
    const response = send({
      url: settings.archiveUrl + "/" + hash,
      method: "PUT",
      body: bytes,
      headers: {
        "Content-Type": "text/markdown; charset=utf-8",
        Authorization: "Bearer " + archiveKey,
      },
      maxResponseBytes: 1024,
      timeoutSeconds: settings.agent.documentTimeoutSeconds,
    });
    check(
      response.status >= 200 && response.status < 300,
      "Report publication failed",
    );
  }

  return {
    transport: {
      now: () => runtime.now().getTime(),
      get: (url, maxResponseBytes) =>
        send({
          url,
          method: "GET",
          headers: {},
          maxResponseBytes,
          timeoutSeconds: settings.agent.documentTimeoutSeconds,
        }),
      model: callModel,
    },
    publish,
  };
}
