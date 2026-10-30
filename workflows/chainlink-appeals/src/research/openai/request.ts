/** OpenAI Responses: provider-hosted reasoning and iterative web research. */
import { utf8 } from "@tekmerion/sdk/wire";
import type { ResearchTask } from "../types.js";
import { canonical } from "../../shared/json.js";
import type { AgentConfig } from "../config.js";
import type { AuditDocuments } from "../documents.js";
import { buildAuditInstructions, DECISION_SCHEMA } from "./prompt.js";

export const MODEL_ENDPOINT = "https://api.openai.com/v1/responses";

export function buildResearchRequest(
  config: AgentConfig,
  context: ResearchTask,
  documents: AuditDocuments,
): Uint8Array {
  const webSearchAllowed = config.allowedTools.includes("web_search");
  return utf8(
    canonical({
      model: config.model,
      instructions: buildAuditInstructions(config),
      input: [
        {
          role: "user",
          content: canonical({
            case: context.case,
            subject: context.subject,
            policy: context.policy,
            cutoffUnixSeconds: context.cutoff.toString(),
            documents,
          }),
        },
      ],
      reasoning: { effort: config.reasoningEffort },
      ...(webSearchAllowed
        ? {
            tools: [
              {
                type: "web_search",
                filters: { allowed_domains: config.allowedSearchDomains },
              },
            ],
            tool_choice: "auto",
            max_tool_calls: config.maxToolCalls,
            include: ["web_search_call.action.sources"],
          }
        : { tools: [], tool_choice: "none" }),
      max_output_tokens: config.maxOutputTokens,
      text: {
        format: {
          type: "json_schema",
          name: "audit_decision",
          strict: true,
          schema: DECISION_SCHEMA,
        },
      },
      store: false,
      background: false,
      truncation: "disabled",
    }),
  );
}
