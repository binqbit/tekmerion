/** Run the built WASM, not imported TypeScript, against the CRE host ABI. */
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
import { build } from "bun";
import { test } from "bun:test";
import {
  create,
  fromBinary,
  toBinary,
  type DescMessage,
  type MessageInitShape,
} from "@bufbuild/protobuf";
import { Value } from "@chainlink/cre-sdk";
import { address, hash, hex } from "@tekmerion/sdk/wire";
import { decodeReport } from "@tekmerion/chainlink-sdk/wire";
import type { WorkflowConfig } from "../../src/workflow/config.js";
import {
  SDK_PB,
  HTTP_TRIGGER_PB,
  CRON_TRIGGER_PB,
  SOLANA_PB,
  HTTP_CLIENT_PB,
} from "@chainlink/cre-sdk/pb";

import {
  f,
  agentConfig,
  capabilityReply,
  recordedAudit,
} from "../support/wasm-fixtures.js";

const wasm =
  process.env.WORKFLOW_WASM ??
  fileURLToPath(
    new URL("../../../../target/chainlink-appeals.wasm", import.meta.url),
  );
// Bundle the host's protobuf schemas for Node; the SDK publishes extensionless imports.
const hostBuild = await build({
  entrypoints: [
    fileURLToPath(new URL("../support/wasm-host.mjs", import.meta.url)),
  ],
  outdir: dirname(wasm),
  naming: "wasm-test-host.mjs",
  target: "node",
});
assert.equal(hostBuild.success, true, String(hostBuild.logs));
const host = hostBuild.outputs[0]!.path;
const settings = {
  network: "mainnet",
  policy: f.policy,
  archiveUrl: "https://archive.test/v1/audit-artifacts",
  triggerKey: "0x" + "22".repeat(20),
  agent: agentConfig,
};
const utf8 = (value: string) => new TextEncoder().encode(value);
const base64 = (bytes: Uint8Array) => Buffer.from(bytes).toString("base64");

function run(
  request: MessageInitShape<typeof SDK_PB.ExecuteRequestSchema>["request"],
  config: WorkflowConfig = settings,
  capabilityReplies: (Uint8Array | { reportMetadata: Uint8Array })[] = [],
  secretReplies?: Uint8Array[],
) {
  const encoded = toBinary(
    SDK_PB.ExecuteRequestSchema,
    create(SDK_PB.ExecuteRequestSchema, {
      config: utf8(JSON.stringify(config)),
      maxResponseSize: 1000000n,
      request,
    }),
  );
  const output = JSON.parse(
    execFileSync("node", [host], {
      input: JSON.stringify({
        wasm,
        request: base64(encoded),
        capabilityReplies: capabilityReplies.map((reply) =>
          reply instanceof Uint8Array
            ? base64(reply)
            : { reportMetadata: base64(reply.reportMetadata) },
        ),
        secretReplies: secretReplies?.map(base64),
      }),
      encoding: "utf8",
      env: { PATH: process.env.PATH ?? "" },
      timeout: 5000,
      stdio: ["pipe", "pipe", "pipe"],
    }),
  );
  const result = fromBinary(
    SDK_PB.ExecutionResultSchema,
    Buffer.from(output.response, "base64"),
  ).result;
  if (result.case !== "error") {
    assert.equal(
      output.secretsAwaited,
      secretReplies?.length ?? 0,
      "Unused secret replies",
    );
    assert.equal(
      output.awaited,
      capabilityReplies.length,
      "Unused host replies",
    );
  }
  return {
    result,
    secretRequests: output.secretRequests.map(
      (item: { request: string; afterCalls: number }) => ({
        request: fromBinary(
          SDK_PB.GetSecretsRequestSchema,
          Buffer.from(item.request, "base64"),
        ),
        afterCalls: item.afterCalls,
      }),
    ),
    calls: output.calls.map((call: string) =>
      fromBinary(SDK_PB.CapabilityRequestSchema, Buffer.from(call, "base64")),
    ),
  };
}
const trigger = <S extends DescMessage>(
  id: bigint,
  schema: S,
  payload: MessageInitShape<S>,
) => ({
  case: "trigger" as const,
  value: {
    id,
    payload: {
      typeUrl: "type.googleapis.com/" + schema.typeName,
      value: toBinary(schema, create(schema, payload)),
    },
  },
});

test("compiled workflow registers authorized HTTP and minute cron handlers with TEE requirements", () => {
  for (const agent of [
    agentConfig,
    {
      ...agentConfig,
      allowedTools: [],
      maxToolCalls: 0,
      allowedSearchDomains: [],
    },
  ]) {
    const { result, calls } = run(
      { case: "subscribe", value: {} },
      { ...settings, agent },
    );
    assert.equal(result.case, "triggerSubscriptions", String(result.value));
    assert.equal(calls.length, 0);
    if (result.case !== "triggerSubscriptions")
      throw new Error(String(result.value));
    const [http, cron] = result.value.subscriptions;
    assert.equal(result.value.subscriptions.length, 2);
    const httpConfig = fromBinary(
      HTTP_TRIGGER_PB.ConfigSchema,
      http!.payload!.value,
    );
    assert.deepEqual(
      httpConfig.authorizedKeys.map((key) => key.publicKey),
      [settings.triggerKey],
    );
    const cronConfig = fromBinary(
      CRON_TRIGGER_PB.ConfigSchema,
      cron!.payload!.value,
    );
    assert.equal(cronConfig.schedule, "0 * * * * *");
    for (const handler of [http!, cron!]) {
      const tee = handler.requirements!.tee!.item;
      assert.equal(tee.case, "teeTypesAndRegions");
      if (tee.case !== "teeTypesAndRegions")
        throw new Error("Missing TEE constraint");
      assert.equal(
        tee.value.teeTypeAndRegions[0]!.type,
        SDK_PB.TeeType.AWS_NITRO,
      );
      assert.deepEqual(tee.value.teeTypeAndRegions[0]!.regions, ["us-west-2"]);
    }
  }
});

test("compiled workflow starts without archive settings or secret access", () => {
  const { archiveUrl, ...withoutArchive } = settings;
  const { result, calls } = run(
    { case: "subscribe", value: {} },
    withoutArchive,
  );
  assert.equal(result.case, "triggerSubscriptions", String(result.value));
  assert.equal(calls.length, 0);
});

test("compiled HTTP entrypoint rejects caller overrides before chain, secrets or AI access", () => {
  const { result, calls } = run(
    trigger(0n, HTTP_TRIGGER_PB.PayloadSchema, {
      input: utf8(JSON.stringify({ case: f.case, model: "forged" })),
    }),
  );
  assert.equal(result.case, "error");
  assert.match(String(result.value), /Unexpected schema fields/);
  assert.equal(calls.length, 0);
});

test("compiled entrypoint rejects incompatible tool policy during configuration parsing", () => {
  const { result, calls } = run(
    { case: "subscribe", value: {} },
    {
      ...settings,
      agent: { ...agentConfig, allowedTools: ["shell"] },
    },
  );
  assert.equal(result.case, "error");
  assert.match(String(result.value), /Explicitly allow supported tools/);
  assert.equal(calls.length, 0);
});

test("compiled cron executes filtered official Solana reads and leaves an empty queue untouched", () => {
  const replies = [0, 1].map((id) =>
    capabilityReply(id, SOLANA_PB.GetProgramAccountsReplySchema, { value: [] }),
  );
  const { result, calls } = run(
    trigger(1n, CRON_TRIGGER_PB.PayloadSchema, {}),
    settings,
    replies,
  );
  assert.equal(result.case, "value", String(result.value));
  if (result.case !== "value") throw new Error(String(result.value));
  assert.equal(
    Value.wrap(result.value).unwrap(),
    "No eligible prepared request",
  );
  assert.equal(calls.length, 2);
  for (const [index, call] of calls.entries()) {
    assert.equal(call.method, "GetProgramAccounts");
    const request = fromBinary(
      SOLANA_PB.GetProgramAccountsRequestSchema,
      call.payload!.value,
    );
    assert.equal(request.opts!.commitment, SOLANA_PB.CommitmentType.FINALIZED);
    assert.equal(request.opts!.filters[0]!.dataSize, 232n);
    const state = request.opts!.filters.find(
      (entry) => entry.memcmp?.offset === 106n,
    )!.memcmp!;
    assert.deepEqual([...state.bytes], [index === 0 ? 0 : 2]);
  }
});

test("compiled HTTP reads committed accounts and rejects a config mismatch before claiming or calling AI", () => {
  const records = f.snapshots.prepared.map(
    (account: { owner: string; data: string }) => ({
      account: {
        owner: address(account.owner),
        data: {
          body: {
            case: "raw" as const,
            value: Buffer.from(account.data, "hex"),
          },
        },
      },
    }),
  );
  const clock = new Uint8Array(40);
  new DataView(clock.buffer).setBigInt64(32, 1000n, true);
  const clockAccount = {
    account: {
      owner: address("Sysvar1111111111111111111111111111111111111"),
      data: { body: { case: "raw" as const, value: clock } },
    },
  };
  const replies = [0, 1, 2, 3].map((id) =>
    capabilityReply(id, SOLANA_PB.GetMultipleAccountsWithOptsReplySchema, {
      value: id % 2 === 0 ? [records[2]] : [...records, clockAccount],
    }),
  );
  const { result, calls } = run(
    trigger(0n, HTTP_TRIGGER_PB.PayloadSchema, {
      input: utf8(JSON.stringify({ case: f.case })),
    }),
    { ...settings, agent: { ...agentConfig, reasoningEffort: "low" } },
    replies,
  );
  assert.equal(result.case, "error");
  assert.equal(result.value, "Agent configuration changed");
  assert.equal(calls.length, 4);
  assert.ok(
    calls.every((call) => call.method === "GetMultipleAccountsWithOpts"),
  );
});

for (const mode of [
  "yes",
  "no",
  "unknown",
  "provider-failure",
  "oversized-model",
  "decision-failure",
  "archive-failure",
  "no-archive",
]) {
  test(`compiled WASM completes ${mode} through documents, model, native reports and optional archive`, () => {
    const recorded = recordedAudit(mode);
    const config: WorkflowConfig = { ...settings };
    if (mode === "no-archive") delete config.archiveUrl;
    const { result, calls, secretRequests } = run(
      trigger(0n, HTTP_TRIGGER_PB.PayloadSchema, {
        input: utf8(JSON.stringify({ case: f.case })),
      }),
      config,
      recorded.replies,
      recorded.secretReplies,
    );
    assert.equal(result.case, "value", String(result.value));
    if (result.case !== "value") throw new Error(String(result.value));
    assert.deepEqual(JSON.parse(String(Value.wrap(result.value).unwrap())), {
      case: f.case,
      attempt: 0,
      stage: [
        "provider-failure",
        "oversized-model",
        "decision-failure",
      ].includes(mode)
        ? 2
        : 1,
    });
    const writes = calls
      .filter((call) => call.method === "WriteReport")
      .map((call) =>
        fromBinary(SOLANA_PB.WriteReportRequestSchema, call.payload!.value),
      );
    assert.equal(writes.length, 2);
    for (const write of writes) {
      assert.equal(
        hex(write.receiver),
        hex(address(f.snapshots.prepared[0].owner)),
      );
      assert.equal(write.remainingAccounts.length, 4);
      assert.equal(write.computeConfig!.computeLimit, 400000);
    }
    const claim = decodeReport(writes[0]!.report!.rawReport.slice(145));
    const terminal = decodeReport(writes[1]!.report!.rawReport.slice(145));
    assert.equal(claim.stage, 0);
    assert.equal(terminal.requestDigest, f.requestDigest);
    assert.equal(terminal.executionId, claim.executionId);
    assert.equal(terminal.attemptIndex, claim.attemptIndex);
    if (
      ["provider-failure", "oversized-model", "decision-failure"].includes(mode)
    ) {
      assert.equal(terminal.stage, 2);
      if (terminal.stage === 2)
        assert.equal(terminal.failureCode, mode === "oversized-model" ? 67 : 2);
    } else {
      assert.equal(terminal.stage, 1);
      if (terminal.stage === 1) {
        assert.equal(
          terminal.answer,
          mode === "no" ? 1 : mode === "unknown" ? 2 : 0,
        );
        assert.equal(terminal.reason, mode === "unknown" ? 3 : 0);
      }
    }
    const requests = calls
      .filter((call) => call.method === "SendRequest")
      .map((call) =>
        fromBinary(HTTP_CLIENT_PB.RequestSchema, call.payload!.value),
      );
    assert.deepEqual(
      requests.map((request) => request.method),
      [
        "GET",
        "GET",
        "POST",
        ...(["provider-failure", "oversized-model"].includes(mode)
          ? []
          : ["POST"]),
        ...(mode === "no-archive" ? [] : ["PUT"]),
      ],
    );
    for (const request of requests) {
      assert.equal(
        request.timeout?.seconds,
        BigInt(
          request.method === "POST"
            ? JSON.parse(new TextDecoder().decode(request.body)).text.format
                .name === "audit_research"
              ? agentConfig.researchTimeoutSeconds
              : agentConfig.decisionTimeoutSeconds
            : agentConfig.documentTimeoutSeconds,
        ),
      );
      assert.equal(request.timeout?.nanos, 0);
    }
    const model = JSON.parse(new TextDecoder().decode(requests[2]!.body));
    const context = JSON.parse(model.input[0].content);
    assert.equal(context.documents.agreement.text, f.documents.agreement);
    assert.equal(context.documents.result.text, f.documents.result);
    assert.equal(context.cutoffUnixSeconds, "1000");
    assert.equal(context.cutoffUtc, "1970-01-01T00:16:40.000Z");
    assert.equal(model.model, agentConfig.model);
    assert.equal(model.reasoning.effort, agentConfig.reasoningEffort);
    assert.equal(requests[2]!.url, "https://api.openai.com/v1/responses");
    assert.deepEqual(requests[2]!.multiHeaders.Authorization!.values, [
      "Bearer synthetic-MODEL_KEY",
    ]);
    for (const documentRequest of requests.slice(0, 2)) {
      assert.equal(documentRequest.multiHeaders.Authorization, undefined);
      assert.ok(
        documentRequest.url.startsWith(
          agentConfig.gatewayOrigins[0] + "/ipfs/",
        ),
      );
    }
    assert.deepEqual(
      model.tools[0].filters.allowed_domains,
      agentConfig.allowedSearchDomains,
    );
    assert.equal(model.max_tool_calls, agentConfig.maxToolCalls);
    assert.deepEqual(
      secretRequests.map((item) => item.request.requests[0]!.id),
      [
        "MODEL_KEY",
        ...(["provider-failure", "oversized-model"].includes(mode)
          ? []
          : ["MODEL_KEY"]),
        ...(mode === "no-archive" ? [] : ["ARCHIVE_KEY"]),
      ],
    );
    assert.ok(
      secretRequests[0]!.afterCalls >
        calls.indexOf(calls.find((call) => call.method === "WriteReport")!),
    );
    if (mode !== "no-archive") {
      const archive = requests.at(-1)!;
      assert.deepEqual(archive.multiHeaders.Authorization!.values, [
        "Bearer synthetic-ARCHIVE_KEY",
      ]);
      assert.deepEqual(archive.multiHeaders["Content-Type"]!.values, [
        "text/markdown; charset=utf-8",
      ]);
      assert.equal(terminal.stage === 0, false);
      if (terminal.stage !== 0) {
        assert.equal(
          archive.url,
          settings.archiveUrl + "/" + terminal.reportHash,
        );
        assert.equal(hex(hash(archive.body)), terminal.reportHash);
      }
      const text = new TextDecoder().decode(archive.body);
      assert.match(
        text,
        ["provider-failure", "oversized-model", "decision-failure"].includes(
          mode,
        )
          ? /^# Audit execution failure/
          : /^# Audit report/,
      );
      assert.ok(!text.includes("synthetic-MODEL_KEY"));
      assert.ok(!text.includes("synthetic-ARCHIVE_KEY"));
      if (
        !["provider-failure", "oversized-model", "decision-failure"].includes(
          mode,
        )
      )
        assert.ok(text.includes("### Findings\n\nSynthetic evidence"));
      assert.equal(calls.at(-1)!.method, "SendRequest");
      assert.equal(secretRequests.at(-1)!.afterCalls, calls.length - 1);
      if (mode === "yes" && process.env.CONFIDENTIAL_REPLAY_OUTPUT) {
        writeFileSync(
          process.env.CONFIDENTIAL_REPLAY_OUTPUT,
          JSON.stringify({
            kind: "recorded-local-workflow",
            report: hex(writes[1]!.report!.rawReport.slice(145)),
            document: hex(archive.body),
          }) + "\n",
        );
      }
    }
  });
}
