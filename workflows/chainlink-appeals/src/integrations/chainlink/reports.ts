/** Native execution identity, signed envelopes and Forwarder delivery. No audit semantics. */
import {
  cre,
  SolanaTxStatus,
  calculateAccountsHash,
  encodeForwarderReport,
  prepareSolanaReportRequest,
  solanaAccountMeta,
  solanaAccountMetasToJson,
  type TeeRuntime,
} from "@chainlink/cre-sdk";
import { address, check, hex } from "@tekmerion/sdk/wire";

export function identifyExecution<Config>(
  runtime: TeeRuntime<Config>,
  payload: Uint8Array,
) {
  const report = runtime
    .reportFromDon(prepareSolanaReportRequest(payload))
    .result();
  return { workflowId: report.workflowId(), executionId: report.executionId() };
}

export function deliverForwarderReport<Config>(
  runtime: TeeRuntime<Config>,
  target: {
    selector: bigint;
    receiver: string;
    accounts: { key: string; writable: boolean }[];
    computeLimit: number;
  },
  expected: { workflowId: string; executionId: string },
  payload: Uint8Array,
): boolean {
  const accounts = target.accounts.map((account) =>
    solanaAccountMeta(account.key, account.writable),
  );
  const report = runtime
    .reportFromDon(
      prepareSolanaReportRequest(
        encodeForwarderReport({
          accountHash: calculateAccountsHash(accounts),
          payload,
        }),
      ),
    )
    .result();
  check(
    report.executionId() === expected.executionId &&
      report.workflowId() === expected.workflowId,
    "Unexpected native report routing",
  );
  // The transmitter consumes state/authority and forwards the remaining receiver accounts.
  const result = new cre.capabilities.SolanaClient(target.selector)
    .writeReport(runtime.usingTheDons(), {
      receiver: "0x" + hex(address(target.receiver)),
      remainingAccounts: solanaAccountMetasToJson(accounts),
      computeConfig: { computeLimit: target.computeLimit },
      report,
    })
    .result();
  return result.txStatus === SolanaTxStatus.SUCCESS;
}
