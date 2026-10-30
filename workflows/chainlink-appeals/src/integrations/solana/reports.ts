/** Route native reports and confirm duplicate deliveries without a second evaluation. */
import type { TeeRuntime } from "@chainlink/cre-sdk";
import { address, check, hash, hex, pda, utf8 } from "@tekmerion/sdk/wire";
import {
  ADAPTER_IDS,
  encodeReport,
  requestAddress,
  reportId,
  type AuditReport,
  type ChainlinkNetwork,
} from "@tekmerion/chainlink-sdk/wire";
import { canonical } from "../../shared/json.js";
import {
  identifyExecution,
  deliverForwarderReport,
} from "../chainlink/reports.js";
import { readBinding } from "./accounts.js";
import { solanaSelectors } from "../chainlink/solana.js";
import type { AuditBinding, ExecutionPorts } from "../../audit/ports.js";

export function createReportChannel<Config>(
  runtime: TeeRuntime<Config>,
  network: ChainlinkNetwork,
  caseKey: string,
  binding: AuditBinding,
): Pick<ExecutionPorts, "executionId" | "deliver"> {
  const config = binding.profile.config;
  let executionId = "";
  function beginExecution(): string {
    const identity = identifyExecution(
      runtime,
      utf8(
        canonical({
          kind: "context",
          request: binding.request.digest,
        }),
      ),
    );
    check(
      identity.workflowId === config.workflowId,
      "Native workflow identity mismatch",
    );
    executionId = identity.executionId;
    return executionId;
  }
  function deliver(report: AuditReport): void {
    const payload = encodeReport(report);
    const delivered = deliverForwarderReport(
      runtime,
      {
        selector: solanaSelectors[network],
        receiver: ADAPTER_IDS[network],
        accounts: receiverAccounts(binding, network),
        computeLimit: 400000,
      },
      {
        workflowId: config.workflowId,
        executionId,
        reportId: reportId(report.stage),
      },
      payload,
    );
    if (delivered) return;
    const { request } = readBinding(
      runtime.usingTheDons(),
      network,
      caseKey,
    ).binding;
    check(
      request.executionId === report.executionId &&
        request.attemptsStarted === report.attemptIndex + 1 &&
        request.digest === report.requestDigest,
      "Unconfirmed execution delivery",
    );
    check(
      report.stage === 0
        ? request.state !== "prepared"
        : request.terminalPayloadHash === hex(hash(payload)),
      "Report was not applied",
    );
  }
  return { executionId: beginExecution, deliver };
}

function receiverAccounts(binding: AuditBinding, network: ChainlinkNetwork) {
  const config = binding.profile.config;
  const authority = pda(
    [
      utf8("forwarder"),
      address(config.forwarderState),
      address(ADAPTER_IDS[network]),
    ],
    config.forwarderProgram,
  )[0];
  return [
    { key: config.forwarderState, writable: false },
    { key: authority, writable: false },
    { key: binding.context.profile, writable: false },
    { key: binding.case.policy, writable: false },
    { key: binding.context.case, writable: false },
    {
      key: requestAddress(
        binding.context.profile,
        binding.context.case,
        network,
      )[0],
      writable: true,
    },
  ];
}
