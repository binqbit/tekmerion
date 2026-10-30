/** Actual SDK plumbing with synthetic capability responses; not live TEE evidence. */
import assert from "node:assert/strict";
import {
  test,
  newTestRuntime,
  HttpActionsMock,
  SolanaMock,
  ConsensusMock,
  addSolanaContractMock,
} from "@chainlink/cre-sdk/test";
import type { TeeRuntime } from "@chainlink/cre-sdk";
import {
  address,
  base58,
  unhex,
  utf8,
  concat,
  hash,
  hex,
} from "@tekmerion/sdk/wire";
import { decodeReport } from "@tekmerion/chainlink-sdk/wire";
import { onHttpTrigger, onCronTrigger } from "../../src/workflow/handlers.js";
import type { WorkflowConfig } from "../../src/workflow/config.js";
import { base64 } from "../../src/shared/bytes.js";
import { createAuditHttp } from "../../src/integrations/http.js";
import { readDocuments } from "../../src/research/documents.js";
import { f, agentConfig, scenario } from "../support/fixtures.mjs";

test("gateway refusals and oversized unverified responses cannot veto committed documents", () => {
  const content = utf8("Committed agreement");
  const digest = hex(hash(content));
  for (const failure of [403, 404, "oversized"] as const) {
    const config: WorkflowConfig = {
      network: "mainnet",
      policy: f.policy,
      archiveUrl: "https://archive.test/v1/audit-artifacts",
      triggerKey: "0x" + "22".repeat(20),
      agent: {
        ...agentConfig,
        httpTimeoutSeconds: 30,
        gatewayOrigins: ["https://unavailable.test", "https://valid.test"],
      },
    };
    const runtime = newTestRuntime(
      new Map(),
      { timeProvider: () => 0 },
      config,
    );
    const calls: string[] = [];
    HttpActionsMock.testInstance().sendRequest = (input) => {
      calls.push(input.url);
      const unavailable = input.url.startsWith("https://unavailable.test/");
      return {
        statusCode: unavailable && failure !== "oversized" ? failure : 200,
        body: base64(
          unavailable && failure === "oversized"
            ? new Uint8Array(config.agent.maxDocumentBytes + 1)
            : content,
        ),
      };
    };
    const { transport } = createAuditHttp(
      runtime as unknown as TeeRuntime<WorkflowConfig>,
      config,
    );
    const documents = readDocuments(
      config.agent,
      { agreementHash: digest, resultHash: digest },
      transport,
      () => {},
    );
    assert.equal(documents.agreement.text, "Committed agreement");
    assert.equal(documents.result.hash, digest);
    assert.deepEqual(
      calls.map((url) => new URL(url).hostname),
      ["unavailable.test", "valid.test", "unavailable.test", "valid.test"],
    );
  }
});

for (const mode of [
  "http",
  "cron",
  "cron-history",
  "wrong-workflow",
  "duplicate-claim",
  "unconfirmed-claim",
  "no-archive",
  "missing-archive-key",
  "empty-archive-key",
  "detailed-report",
] as const)
  test(`CCA-022/027: ${mode} preserves the verified execution boundary`, () => {
    const s = scenario(),
      artifacts = new Set<string>();
    const archiveBytes = new Map<string, Uint8Array>();
    if (mode === "detailed-report") {
      const model = s.ports.transport.model;
      s.ports.transport.model = (body) => {
        const response = model(body);
        const value = JSON.parse(new TextDecoder().decode(response.body));
        const decision = JSON.parse(value.output.at(-1).content[0].text);
        decision.explanation =
          "Verified the agreement criteria against the published release. ".repeat(
            180,
          );
        value.output.at(-1).content[0].text = JSON.stringify(decision);
        value.provider_metadata = "internal metadata excluded from the report";
        return { ...response, body: utf8(JSON.stringify(value)) };
      };
    }
    let claimed = false,
      reports = 0,
      writes = 0,
      scans = 0;
    const c: WorkflowConfig = {
      network: "mainnet",
      policy: f.policy,
      archiveUrl: "https://archive.test/v1/audit-artifacts",
      triggerKey: "0x" + "22".repeat(20),
      agent: agentConfig,
    };
    if (mode === "no-archive") {
      delete c.archiveUrl;
    }
    const secrets = new Map([["MODEL_KEY", "synthetic-model-key"]]);
    if (mode !== "missing-archive-key") {
      secrets.set(
        "ARCHIVE_KEY",
        mode === "empty-archive-key" ? "" : "synthetic-archive-key",
      );
    }
    const consensus = ConsensusMock.testInstance();
    const runtime = newTestRuntime(
      new Map([["main", secrets]]),
      { timeProvider: () => 1000000 },
      c,
    );
    // The exported SDK test runtime is DON-only. This adapter exercises its real
    // capabilities without claiming that a local process is a TEE attestation.
    const tee = {
      config: c,
      usingTheDons: () => runtime,
      reportFromDon: runtime.report.bind(runtime),
      getSecret: runtime.getSecret.bind(runtime),
      getSecrets: runtime.getSecrets.bind(runtime),
      now: runtime.now.bind(runtime),
      log: runtime.log.bind(runtime),
      callCapability: runtime.callCapability.bind(runtime),
    } as unknown as TeeRuntime<WorkflowConfig>;
    const raw = (s: string) => unhex(s, s.length / 2);
    const account = (a: { owner: string; data: string }) => ({
      owner: base64(address(a.owner)),
      data: { raw: base64(raw(a.data)) },
      executable: false,
      lamports: "1",
    });
    const solana = SolanaMock.testInstance(124615329519749607n);
    solana.getMultipleAccountsWithOpts = (input) => {
      assert.equal(input.opts?.commitment, 1); // FINALIZED enum in the pinned SDK.
      return {
        value: input.accounts.map((key) => {
          const k = base58(key);
          if (k === "SysvarC1ock11111111111111111111111111111111") {
            const data = new Uint8Array(40);
            new DataView(data.buffer).setBigInt64(32, 1000n, true);
            return {
              account: {
                owner: base64(
                  address("Sysvar1111111111111111111111111111111111111"),
                ),
                data: { raw: base64(data) },
                executable: false,
              },
            };
          }
          const a = f.snapshots[claimed ? "claimed" : "prepared"].find(
            (a: { key: string }) => a.key === k,
          );
          assert.ok(a, `Unknown official read ${k}`);
          return { account: account(a) };
        }),
      };
    };
    solana.getProgramAccounts = (input) => {
      scans++;
      assert.equal(input.opts?.commitment, 1);
      assert.equal(input.opts?.filters[0]?.dataSize, 232n);
      const filter = input.opts?.filters.find(
        (filter) => filter.memcmp?.offset === 106n,
      )?.memcmp;
      assert.ok(
        filter,
        "Discovery must filter by execution state on the server",
      );
      const wanted = filter.bytes[0];
      assert.ok(wanted === 0 || wanted === 2);
      const pending = f.snapshots.prepared[3];
      const history =
        mode === "cron-history"
          ? Array.from({ length: 100 }, () => f.snapshots.applied[3])
          : [];
      const rows = [...history, pending].filter(
        (row) => raw(row.data)[106] === wanted,
      );
      return {
        value: rows.map((row) => ({
          pubkey: base64(address(row.key)),
          account: account(row),
        })),
      };
    };
    consensus.report = (input) => {
      const metadata = new Uint8Array(109);
      metadata[0] = 1;
      metadata.set(new Uint8Array(32).fill(1), 1);
      metadata.set(
        new Uint8Array(32).fill(mode === "wrong-workflow" ? 0x12 : 0x11),
        45,
      );
      metadata[108] = ++reports;
      return {
        rawReport: base64(concat(metadata, input.encodedPayload)),
        sigs: [],
      };
    };
    const receiver = addSolanaContractMock(solana, {
      programId: f.snapshots.prepared[0].owner,
    });
    receiver.writeReport = ({ report, remainingAccounts, computeConfig }) => {
      writes++;
      assert.equal(remainingAccounts.length, 4);
      assert.equal(computeConfig?.computeLimit, 400000);
      const p = decodeReport(report.rawReport.slice(145));
      if (p.stage === 0) {
        claimed = mode !== "unconfirmed-claim";
        assert.equal(s.modelCalls, 0);
      } else assert.equal(p.stage, 1);
      s.ports.deliver(p);
      return {
        txStatus:
          p.stage === 0 &&
          (mode === "duplicate-claim" || mode === "unconfirmed-claim")
            ? "TX_STATUS_ABORTED"
            : "TX_STATUS_SUCCESS",
        txSignature: base64(new Uint8Array(64)),
      };
    };
    HttpActionsMock.testInstance().sendRequest = (input) => {
      const body = input.body
        ? new Uint8Array(Buffer.from(input.body, "base64"))
        : null;
      if (input.method === "PUT") {
        assert.equal(writes, 2, "Archive must follow terminal delivery");
        const digest = input.url.split("/").at(-1)!;
        assert.equal(hex(hash(body!)), digest);
        archiveBytes.set(digest, body!);
        artifacts.add(digest);
        return { statusCode: 200, body: base64(utf8("{}")) };
      }
      const reply =
        input.method === "POST"
          ? s.ports.transport.model(body!)
          : s.ports.transport.get(input.url, 32768);
      return { statusCode: reply.status, body: base64(reply.body) };
    };
    const run = () =>
      !mode.startsWith("cron")
        ? onHttpTrigger(tee, {
            input: utf8(JSON.stringify({ case: f.case })),
          } as any)
        : onCronTrigger(tee);
    if (mode === "wrong-workflow" || mode === "unconfirmed-claim") {
      assert.throws(
        run,
        mode === "wrong-workflow"
          ? /Native workflow identity mismatch/
          : /Unconfirmed execution delivery/,
      );
      assert.equal(s.modelCalls, 0);
      assert.equal(writes, mode === "wrong-workflow" ? 0 : 1);
      assert.equal(artifacts.size, 0);
      return;
    }
    const output = run();
    assert.equal(JSON.parse(output).stage, 1);
    assert.equal(reports, 3);
    assert.equal(writes, 2);
    assert.equal(s.modelCalls, 1);
    assert.equal(
      artifacts.size,
      ["no-archive", "missing-archive-key", "empty-archive-key"].includes(mode)
        ? 0
        : 1,
    );
    assert.equal(scans, mode.startsWith("cron") ? 2 : 0);
    if (mode === "detailed-report") {
      const terminal = s.reports.at(-1);
      assert.equal(terminal.stage, 1);
      const document = new TextDecoder().decode(
        archiveBytes.get(terminal.reportHash)!,
      );
      assert.match(document, /^# Audit report/);
      assert.ok(document.length > 8000);
      assert.ok(document.includes("https://evidence.test/release"));
      assert.ok(!document.includes("internal metadata"));
    }
  });
