/** Validate the provider envelope and decision; substantive judgment stays with the researcher. */
import { validateDecision } from "@tekmerion/sdk/wire";
import { decodeText } from "../../shared/bytes.js";
import { object } from "../../shared/json.js";
import { parseUrl, publicUrl } from "../../shared/url.js";
import type { AgentConfig } from "../config.js";
import { AuditFailure, FailureCode } from "../failure.js";
import type { Decision, ResearchResult, Evidence } from "../types.js";

export function parseEvidenceResponse(
  bytes: Uint8Array,
  config: AgentConfig,
): Evidence {
  return validated(() => {
    const response = parseResponseEnvelope(bytes, config);
    const { consulted, messages } = parseResearchOutput(
      response.output,
      config,
    );
    if (messages.length !== 1) throw new Error("Expected one research summary");
    const output = object(JSON.parse(messages[0]!), ["findings"]);
    if (typeof output.findings !== "string" || !output.findings.trim())
      throw new Error("Missing research findings");
    if (consulted.size > 100)
      throw new AuditFailure(FailureCode.BudgetExceeded);
    return { findings: output.findings, sources: [...consulted] };
  });
}

export function parseDecisionResponse(
  bytes: Uint8Array,
  config: AgentConfig,
  evidence: Evidence,
): ResearchResult {
  return validated(() => {
    const response = parseResponseEnvelope(bytes, config);
    // No tool call may be smuggled into the decision stage, even if its URL is allowed.
    const { messages } = parseResearchOutput(response.output, {
      ...config,
      allowedTools: [],
      maxToolCalls: 0,
    });
    return {
      responseId: response.id,
      decision: parseDecision(messages, evidence, config),
    };
  });
}

function validated<T>(parse: () => T): T {
  try {
    return parse();
  } catch (error) {
    if (error instanceof AuditFailure) throw error;
    throw new AuditFailure(FailureCode.InvalidModelOutput);
  }
}

function parseResponseEnvelope(bytes: Uint8Array, config: AgentConfig) {
  if (bytes.length > config.maxResponseBytes)
    throw new Error("Oversized model response");
  const response = record(JSON.parse(decodeText(bytes)));
  if (response.model !== config.model)
    throw new AuditFailure(FailureCode.ModelRevisionMismatch);
  if (
    response.status !== "completed" ||
    response.error ||
    response.incomplete_details
  ) {
    throw new Error("Research did not complete");
  }
  if (
    typeof response.id !== "string" ||
    !response.id ||
    !Array.isArray(response.output)
  ) {
    throw new Error("Missing provider response identity");
  }
  const usage = record(response.usage);
  if (
    !Number.isSafeInteger(usage.output_tokens) ||
    Number(usage.output_tokens) < 0 ||
    Number(usage.output_tokens) > config.maxOutputTokens
  ) {
    throw new AuditFailure(FailureCode.BudgetExceeded);
  }

  return { id: response.id, output: response.output };
}

function parseResearchOutput(output: unknown[], config: AgentConfig) {
  let toolCalls = 0;
  const consulted = new Set<string>();
  const messages: string[] = [];
  for (const value of output) {
    const item = record(value);
    if (item.type === "web_search_call") {
      if (!config.allowedTools.includes("web_search"))
        throw new Error(
          "Research tool is not permitted by the audit configuration",
        );
      if (item.status !== "completed") throw new Error("Research tool failed");
      const action = record(item.action);
      if (
        !["search", "open_page", "find_in_page"].includes(String(action.type))
      )
        throw new Error("Unapproved research action");
      if (action.url !== undefined)
        consulted.add(approvedSource(action.url, config));
      if (action.sources !== undefined) {
        if (!Array.isArray(action.sources))
          throw new Error("Invalid consulted sources");
        for (const source of action.sources)
          consulted.add(approvedSource(record(source).url, config));
      }
      toolCalls++;
    } else if (item.type === "message") {
      if (
        item.role !== "assistant" ||
        item.status !== "completed" ||
        !Array.isArray(item.content)
      ) {
        throw new Error("Incomplete research message");
      }
      for (const part of item.content) {
        const content = record(part);
        if (
          content.type !== "output_text" ||
          typeof content.text !== "string"
        ) {
          throw new Error("Refused or unsupported response");
        }
        messages.push(content.text);
      }
    } else if (item.type !== "reasoning") {
      throw new Error("Unexpected tool or output");
    }
  }
  if (toolCalls > config.maxToolCalls)
    throw new AuditFailure(FailureCode.BudgetExceeded);
  return { consulted, messages };
}

function parseDecision(
  decisions: string[],
  evidence: Evidence,
  config: AgentConfig,
): Decision {
  if (decisions.length !== 1) throw new Error("Expected one final decision");
  const output = object(JSON.parse(decisions[0]!), [
    "answer",
    "reason",
    "explanation",
    "sourceIds",
  ]);
  if (typeof output.answer !== "string" || typeof output.reason !== "string")
    throw new Error("Expected string decision fields");
  const answer = ["yes", "no", "unknown"].indexOf(output.answer);
  const reason = [
    "decisive",
    "missing_evidence",
    "stale_evidence",
    "conflicting_evidence",
    "unsupported_evidence",
    "inconclusive",
  ].indexOf(output.reason);
  validateDecision(answer, reason);
  if (
    typeof output.explanation !== "string" ||
    !output.explanation.trim() ||
    !Array.isArray(output.sourceIds) ||
    output.sourceIds.length > 100
  ) {
    throw new Error("Invalid decision explanation");
  }
  const ids = output.sourceIds;
  if (
    ids.some((id) => typeof id !== "string" || !/^S[1-9][0-9]*$/.test(id)) ||
    new Set(ids).size !== ids.length
  ) {
    throw new Error("Invalid source citation IDs");
  }
  const catalogue = new Map(
    evidence.sources.map((source, index) => [
      `S${index + 1}`,
      approvedSource(source, config),
    ]),
  );
  const sources = ids.map((id) => {
    const url = catalogue.get(String(id));
    if (!url) throw new Error("Unsubstantiated source citation");
    return url;
  });
  // URLs are rendered by the workflow; the model cannot add an unvisited link in prose.
  if (/https?:|www\.|\]\s*\(/i.test(output.explanation))
    throw new Error("Use catalogue citations, not raw links");
  const explanation = output.explanation.replace(
    /\[(S[0-9]+)\]/g,
    (_marker, id: string) => {
      if (!ids.includes(id) || !catalogue.has(id))
        throw new Error("Undeclared source citation");
      return `[${id}](<${catalogue.get(id)}>)`;
    },
  );
  return { answer, reason, explanation, sources };
}

function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("Expected provider object");
  return value as Record<string, unknown>;
}

function approvedSource(value: unknown, config: AgentConfig): string {
  if (typeof value !== "string" || value.length > 2048) {
    throw new AuditFailure(FailureCode.SourcePolicyViolation);
  }
  // Providers may omit a root slash or vary host casing for the same public URL.
  const parsed = parseUrl(value);
  const url = publicUrl(parsed.href);
  const host = parsed.hostname;
  if (
    !config.allowedSearchDomains.some(
      (domain) => host === domain || host.endsWith("." + domain),
    )
  ) {
    throw new AuditFailure(FailureCode.SourcePolicyViolation);
  }
  return url;
}
