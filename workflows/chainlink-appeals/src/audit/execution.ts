/** One attested execution claim, one evaluation, one terminal report. */
import { check, hash, hex } from "@tekmerion/sdk/wire";
import {
  encodeReport,
  type AuditReport,
  type ReportPrefix,
} from "@tekmerion/chainlink-sdk/wire";
import { runResearch } from "../research/agent.js";
import { agentConfigHash, type AgentConfig } from "../research/config.js";
import { AuditFailure, FailureCode } from "../research/failure.js";
import { createAuditResult, failureReport } from "./report.js";
import type { AuditBinding, AuditSnapshot, ExecutionPorts } from "./ports.js";

export function executeAudit(
  ports: ExecutionPorts,
  config: AgentConfig,
): AuditReport {
  const audit = ports.read();
  validateStart(audit, config);
  const claim: ReportPrefix = {
    version: 3,
    stage: 0,
    requestDigest: audit.binding.request.digest,
    attemptIndex: audit.binding.request.attemptsStarted,
    executionId: ports.executionId(),
  };
  ports.deliver({ ...claim, stage: 0 });
  const confirmed = ports.read();
  validateConfirmedClaim(confirmed, claim, audit.binding.case.deadline);

  const { report, document } = evaluateAudit(
    ports,
    config,
    audit.binding,
    claim,
    confirmed.now,
  );
  encodeReport(report);
  // Keep delivery outside the evaluator catch: a transport failure cannot rerun the AI.
  ports.deliver(report);
  // Archiving is best effort only after the terminal state has been delivered.
  try {
    ports.publish(report.reportHash, document);
  } catch {
    /* Verdict already delivered. */
  }
  return report;
}

function validateStart(audit: AuditSnapshot, config: AgentConfig): void {
  const { request, profile } = audit.binding;
  check(
    profile.config.agentConfigHash === agentConfigHash(config),
    "Agent configuration changed",
  );
  check(
    (request.state === "prepared" || request.state === "retryable") &&
      audit.binding.case.phase === 1,
    "Request cannot start",
  );
  check(request.attemptsStarted < profile.config.maxAttempts, "Attempt limit");
  check(
    audit.binding.case.deadline - audit.now >=
      BigInt(config.maxExecutionSeconds + config.deliveryReserveSeconds),
    "Insufficient audit time",
  );
}

function validateConfirmedClaim(
  confirmed: AuditSnapshot,
  claim: ReportPrefix,
  deadline: bigint,
): void {
  check(
    confirmed.binding.request.state === "active" &&
      confirmed.binding.request.attemptsStarted === claim.attemptIndex + 1 &&
      confirmed.binding.request.executionId === claim.executionId &&
      confirmed.binding.request.digest === claim.requestDigest &&
      confirmed.now < deadline,
    "Claim was not finalized for this execution",
  );
}

function evaluateAudit(
  ports: ExecutionPorts,
  config: AgentConfig,
  binding: AuditBinding,
  claim: ReportPrefix,
  now: bigint,
) {
  try {
    if (
      binding.case.deadline - now <
      BigInt(config.maxExecutionSeconds + config.deliveryReserveSeconds)
    ) {
      throw new AuditFailure(FailureCode.BudgetExceeded);
    }
    const research = runResearch(config, binding.context, ports.transport);
    const result = createAuditResult(config, binding.context, claim, research);
    const report: AuditReport = {
      ...claim,
      stage: 1,
      answer: result.answer,
      reason: result.reason,
      reportHash: result.reportHash,
    };
    return {
      report,
      document: result.document,
    };
  } catch (error) {
    if (!(error instanceof AuditFailure)) throw error;
    const bytes = failureReport(config, binding.context, claim, error.code);
    const reportHash = hex(hash(bytes));
    const report: AuditReport = {
      ...claim,
      stage: 2,
      failureCode: error.code,
      reportHash,
    };
    return { report, document: bytes };
  }
}
