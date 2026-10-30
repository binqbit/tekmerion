/** Bounded HTTP responses and stable technical-failure classification. */
import { decodeText } from "../shared/bytes.js";
import { AuditFailure, FailureCode } from "./failure.js";

export interface Response {
  status: number;
  body: Uint8Array;
}

export interface ResearchTransport {
  get(url: string, limit: number): Response;
  model(body: Uint8Array): Response;
  now(): number;
}

export function readResponse(
  send: () => Response,
  target: "source" | "model",
): Uint8Array {
  let response: Response;
  try {
    response = send();
  } catch (error) {
    if (error instanceof AuditFailure) throw error;
    throw new AuditFailure(
      target === "source"
        ? FailureCode.SourceUnavailable
        : FailureCode.ModelTransportFailed,
    );
  }
  if (response.status === 429) {
    throw new AuditFailure(
      target === "source"
        ? FailureCode.SourceUnavailable
        : FailureCode.ProviderRateLimited,
    );
  }
  if (response.status >= 500) {
    throw new AuditFailure(
      target === "source"
        ? FailureCode.SourceUnavailable
        : FailureCode.ProviderUnavailable,
    );
  }
  if (response.status === 401 || response.status === 403) {
    throw new AuditFailure(
      target === "source"
        ? FailureCode.SourceRejected
        : FailureCode.ModelCredentialsRejected,
    );
  }
  if (response.status < 200 || response.status >= 300) {
    throw new AuditFailure(
      target === "source"
        ? FailureCode.SourceRejected
        : FailureCode.InvalidModelOutput,
    );
  }
  return response.body;
}

export function validatedText(bytes: Uint8Array, max: number): string {
  const hasByteOrderMark =
    bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf;
  if (bytes.length < 1 || bytes.length > max || hasByteOrderMark) {
    throw new AuditFailure(FailureCode.InvalidDocument);
  }
  try {
    return decodeText(bytes);
  } catch {
    throw new AuditFailure(FailureCode.InvalidDocument);
  }
}
