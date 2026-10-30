/** Human-readable final reports; the on-chain hash commits these exact UTF-8 bytes. */
import { utf8 } from "@tekmerion/sdk/wire";
import type { AuditContext, ReportPrefix } from "@tekmerion/chainlink-sdk/wire";
import { sha256 } from "../shared/bytes.js";
import { agentConfigHash, type AgentConfig } from "../research/config.js";
import type { ResearchResult, Decision } from "../research/types.js";

type ExecutionIdentity = Pick<
  ReportPrefix,
  "requestDigest" | "executionId" | "attemptIndex"
>;

export interface AuditResult extends Pick<Decision, "answer" | "reason"> {
  reportHash: string;
  document: Uint8Array;
}

function contextLines(
  config: AgentConfig,
  context: AuditContext,
  identity: ExecutionIdentity,
): string[] {
  return [
    `Case: ${context.case}`,
    `Policy: ${context.policy}`,
    `Agreement SHA-256: ${context.agreementHash}`,
    `Proposed result SHA-256: ${context.resultHash}`,
    `Audit cutoff (Unix seconds): ${context.cutoff}`,
    `Request digest: ${identity.requestDigest}`,
    `Execution ID: ${identity.executionId}`,
    `Attempt: ${identity.attemptIndex + 1}`,
    `Model: ${config.model}`,
    `Reasoning effort: ${config.reasoningEffort}`,
    `Agent configuration SHA-256: ${agentConfigHash(config)}`,
  ].map((line) => `- ${line}`);
}

function formatAssessment(markdown: string): string {
  const lines = markdown.trim().split(/\r?\n/);
  const headings = new Map<number, { level: number; title: string }>();
  let fence: string | undefined;
  let offset = 0;

  for (const [index, line] of lines.entries()) {
    const delimiter = line.match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
    const marker = delimiter?.[1];
    if (fence) {
      if (
        marker &&
        marker[0] === fence[0] &&
        marker.length >= fence.length &&
        delimiter?.[2]?.trim() === ""
      )
        fence = undefined;
      continue;
    }
    if (marker) {
      fence = marker;
      continue;
    }
    const heading = line.match(/^ {0,3}(#{1,6})(?:[ \t]+(.*)|[ \t]*)$/);
    const prefix = heading?.[1];
    if (prefix) {
      headings.set(index, { level: prefix.length, title: heading?.[2] ?? "" });
      offset = Math.max(offset, 3 - prefix.length);
    }
  }

  return lines
    .map((line, index) => {
      const heading = headings.get(index);
      if (!heading) return line;
      const level = Math.min(6, heading.level + offset);
      const before = lines[index - 1]?.trim() ? "\n" : "";
      const after = lines[index + 1]?.trim() ? "\n" : "";
      return `${before}${"#".repeat(level)} ${heading.title}${after}`;
    })
    .join("\n");
}

export function createAuditResult(
  config: AgentConfig,
  context: AuditContext,
  identity: ExecutionIdentity,
  result: ResearchResult,
): AuditResult {
  const { decision } = result;
  const reasons = [
    "Decisive evidence",
    "Missing evidence",
    "Stale evidence",
    "Conflicting evidence",
    "Unsupported evidence",
    "Inconclusive",
  ];
  const document = utf8(
    [
      "# Audit report",
      "",
      "## Audit context",
      "",
      ...contextLines(config, context, identity),
      `- Provider response ID: ${result.responseId}`,
      "",
      "## Decision",
      "",
      `- Verdict: ${["Yes", "No", "Unknown"][decision.answer]}`,
      `- Reason: ${reasons[decision.reason]}`,
      "",
      "## Assessment",
      "",
      formatAssessment(decision.explanation),
      "",
      "## Sources",
      "",
      ...(decision.sources.length
        ? decision.sources.map((url) => `- ${url}`)
        : [
            "No external sources were cited; the assessment uses the supplied documents.",
          ]),
      "",
    ].join("\n"),
  );
  return {
    answer: decision.answer,
    reason: decision.reason,
    reportHash: sha256(document),
    document,
  };
}

export function failureReport(
  config: AgentConfig,
  context: AuditContext,
  identity: ExecutionIdentity,
  failureCode: number,
): Uint8Array {
  return utf8(
    [
      "# Audit execution failure",
      "",
      "## Audit context",
      "",
      ...contextLines(config, context, identity),
      "",
      "## Outcome",
      "",
      "No completed audit verdict was produced.",
      "",
      `Technical failure code: ${failureCode}`,
      "",
    ].join("\n"),
  );
}
