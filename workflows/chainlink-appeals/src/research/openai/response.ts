/** Validate the provider envelope and decision; substantive judgment stays with the researcher. */
import { validateDecision } from "@tekmerion/sdk/wire";
import { decodeText } from "../../shared/bytes.js";
import { object } from "../../shared/json.js";
import { parseUrl, publicUrl } from "../../shared/url.js";
import type { AgentConfig } from "../config.js";
import { AuditFailure, FailureCode } from "../failure.js";
import type { Decision, ResearchResult } from "../types.js";

export function parseResearchResponse(
  bytes: Uint8Array,
  config: AgentConfig,
): ResearchResult {
  try {
    const response = parseResponseEnvelope(bytes, config);
    const { consulted, decisions } = parseResearchOutput(
      response.output,
      config,
    );
    const decision = parseDecision(decisions, consulted, config);
    return { responseId: response.id, decision };
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
  const decisions: string[] = [];
  for (const value of output) {
    const item = record(value);
    if (item.type === "web_search_call") {
      if (!config.allowedTools.includes("web_search"))
        throw new Error(
          "Research tool is not permitted by the audit configuration",
        );
      if (item.status !== "completed") throw new Error("Research tool failed");
      const action = record(item.action);
      if (!["search", "open_page", "find"].includes(String(action.type)))
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
        decisions.push(content.text);
      }
    } else if (item.type !== "reasoning") {
      throw new Error("Unexpected tool or output");
    }
  }
  if (toolCalls > config.maxToolCalls)
    throw new AuditFailure(FailureCode.BudgetExceeded);
  return { consulted, decisions };
}

function parseDecision(
  decisions: string[],
  consulted: Set<string>,
  config: AgentConfig,
): Decision {
  if (decisions.length !== 1) throw new Error("Expected one final decision");
  const output = object(JSON.parse(decisions[0]!), [
    "answer",
    "reason",
    "explanation",
    "sources",
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
    !Array.isArray(output.sources) ||
    output.sources.length > 100
  ) {
    throw new Error("Invalid decision explanation");
  }
  const sources = output.sources.map((source) =>
    approvedSource(source, config),
  );
  if (
    new Set(sources).size !== sources.length ||
    sources.some((source) => !consulted.has(source))
  ) {
    throw new Error("Unsubstantiated source citation");
  }
  return { answer, reason, explanation: output.explanation, sources };
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
