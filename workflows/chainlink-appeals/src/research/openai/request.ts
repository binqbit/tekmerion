/** Two bounded stages: source research, then one tool-free decision. */
import { utf8 } from "@tekmerion/sdk/wire";
import type { ResearchTask, Evidence } from "../types.js";
import { canonical } from "../../shared/json.js";
import type { AgentConfig } from "../config.js";
import type { AuditDocuments } from "../documents.js";
import {
  buildAuditInstructions,
  buildResearchInstructions,
  EVIDENCE_SCHEMA,
  decisionSchema,
} from "./prompt.js";

export const MODEL_ENDPOINT = "https://api.openai.com/v1/responses";

function input(context: ResearchTask, documents: AuditDocuments) {
  return {
    case: context.case,
    subject: context.subject,
    policy: context.policy,
    cutoffUnixSeconds: context.cutoff.toString(),
    cutoffUtc: new Date(Number(context.cutoff) * 1000).toISOString(),
    documents,
  };
}

function settings(config: AgentConfig) {
  return {
    model: config.model,
    reasoning: { effort: config.reasoningEffort },
    max_output_tokens: config.maxOutputTokens,
    store: false,
    background: false,
    truncation: "disabled",
  };
}

export function buildResearchRequest(
  config: AgentConfig,
  context: ResearchTask,
  documents: AuditDocuments,
): Uint8Array {
  return utf8(
    canonical({
      ...settings(config),
      instructions: buildResearchInstructions(config),
      input: [{ role: "user", content: canonical(input(context, documents)) }],
      tools: [
        {
          type: "web_search",
          ...(config.allowedSearchDomains.length
            ? { filters: { allowed_domains: config.allowedSearchDomains } }
            : {}),
        },
      ],
      tool_choice: "auto",
      max_tool_calls: config.maxToolCalls,
      include: ["web_search_call.action.sources"],
      text: {
        format: {
          type: "json_schema",
          name: "audit_research",
          strict: true,
          schema: EVIDENCE_SCHEMA,
        },
      },
    }),
  );
}

export function buildDecisionRequest(
  config: AgentConfig,
  context: ResearchTask,
  documents: AuditDocuments,
  evidence: Evidence,
): Uint8Array {
  return utf8(
    canonical({
      ...settings(config),
      instructions: buildAuditInstructions(),
      input: [
        {
          role: "user",
          content: canonical({
            ...input(context, documents),
            findings: evidence.findings,
            evidenceCatalogue: evidence.sources.map((url, index) => ({
              id: `S${index + 1}`,
              url,
            })),
          }),
        },
      ],
      tools: [],
      tool_choice: "none",
      text: {
        format: {
          type: "json_schema",
          name: "audit_decision",
          strict: true,
          schema: decisionSchema(evidence.sources.length),
        },
      },
    }),
  );
}
