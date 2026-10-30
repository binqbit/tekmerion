/** Research pipeline: verified documents → source research → one validated decision. */
import { parseAgentConfig, type AgentConfig } from "./config.js";
import { readDocuments } from "./documents.js";
import {
  buildResearchRequest,
  buildDecisionRequest,
} from "./openai/request.js";
import {
  parseEvidenceResponse,
  parseDecisionResponse,
} from "./openai/response.js";
import type { ResearchTask, ResearchResult, Evidence } from "./types.js";
import { readResponse, type ResearchTransport } from "./transport.js";
import { AuditFailure, FailureCode } from "./failure.js";

export function runResearch(
  config: AgentConfig,
  context: ResearchTask,
  transport: ResearchTransport,
): ResearchResult {
  parseAgentConfig(config);
  const startedAt = transport.now();
  function checkBudget(): void {
    const elapsed = transport.now() - startedAt;
    if (elapsed < 0 || elapsed >= config.maxExecutionSeconds * 1000) {
      throw new AuditFailure(FailureCode.BudgetExceeded);
    }
  }

  const documents = readDocuments(config, context, transport, checkBudget);
  checkBudget();
  function call(
    request: Uint8Array,
    stage: "research" | "decision",
  ): Uint8Array {
    // Escaping the document text can enlarge JSON beyond its raw UTF-8 length.
    if (request.length > 120000)
      throw new AuditFailure(FailureCode.BudgetExceeded);
    checkBudget();
    const response = readResponse(
      () => transport.model(request, stage),
      "model",
    );
    checkBudget();
    return response;
  }
  let evidence: Evidence = {
    findings:
      "No external research was requested; evaluate only the verified documents.",
    sources: [],
  };
  if (config.allowedTools.includes("web_search")) {
    evidence = parseEvidenceResponse(
      call(buildResearchRequest(config, context, documents), "research"),
      config,
    );
  }
  return parseDecisionResponse(
    call(
      buildDecisionRequest(config, context, documents, evidence),
      "decision",
    ),
    config,
    evidence,
  );
}
