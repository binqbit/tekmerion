/** Fetch committed text, verify bytes, then expose it to the researcher as data. */
import { rawCid } from "../shared/ipfs.js";
import { sha256 } from "../shared/bytes.js";
import type { AgentConfig } from "./config.js";
import { AuditFailure, FailureCode } from "./failure.js";
import {
  readResponse,
  validatedText,
  type ResearchTransport,
} from "./transport.js";

export interface AuditDocuments {
  agreement: { hash: string; text: string };
  result: { hash: string; text: string };
}

export function readDocuments(
  config: AgentConfig,
  context: { agreementHash: string; resultHash: string },
  transport: ResearchTransport,
  checkBudget: () => void,
): AuditDocuments {
  function read(hash: string): { hash: string; text: string } {
    for (const origin of config.gatewayOrigins) {
      checkBudget();
      let bytes: Uint8Array;
      try {
        bytes = readResponse(
          () =>
            transport.get(
              origin + "/ipfs/" + rawCid(hash),
              config.maxDocumentBytes,
            ),
          "source",
        );
      } catch (error) {
        if (
          error instanceof AuditFailure &&
          (error.code === FailureCode.SourceUnavailable ||
            error.code === FailureCode.SourceRejected)
        )
          continue;
        throw error;
      }
      // A bad gateway response cannot establish that the committed document is invalid.
      if (sha256(bytes) !== hash) continue;
      return { hash, text: validatedText(bytes, config.maxDocumentBytes) };
    }
    throw new AuditFailure(FailureCode.SourceUnavailable);
  }
  return {
    agreement: read(context.agreementHash),
    result: read(context.resultHash),
  };
}
