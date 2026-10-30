/** Research pipeline: verified documents → bounded model call → validated decision. */
import { parseAgentConfig, type AgentConfig } from "./config.js";
import { readDocuments } from "./documents.js";
import { buildResearchRequest } from "./openai/request.js";
import { parseResearchResponse } from "./openai/response.js";
import type { ResearchTask, ResearchResult } from "./types.js";
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
  const request = buildResearchRequest(config, context, documents);
  // Escaping the document text can enlarge JSON beyond its raw UTF-8 length.
  if (request.length > 120000)
    throw new AuditFailure(FailureCode.BudgetExceeded);
  const response = readResponse(() => transport.model(request), "model");
  checkBudget();
  return parseResearchResponse(response, config);
}
