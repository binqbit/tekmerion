import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
import test from "node:test";
import { utf8, hex } from "@tekmerion/sdk/wire";
import { encodeReport } from "@tekmerion/chainlink-sdk/wire";
import { executeAudit } from "../../dist/audit/execution.js";
import {
  agentConfigHash,
  parseAgentConfig,
} from "../../dist/research/config.js";
import { parseTrigger } from "../../dist/workflow/trigger.js";
import {
  f,
  agentConfig,
  scenario,
  modelResponse,
  read,
} from "../support/fixtures.mjs";

test("verified generic documents and research reach one bound verdict", () => {
  const s = scenario();
  const report = executeAudit(s.ports, agentConfig);
  assert.equal(report.stage, 1);
  assert.equal(report.answer, 0);
  assert.equal(s.modelCalls, 1);
  assert.equal(
    agentConfigHash(agentConfig),
    read("prepared").binding.profile.config.agentConfigHash,
  );
  assert.deepEqual(
    s.reports.map((p) => p.stage),
    [0, 1],
  );
  const input = JSON.parse(new TextDecoder().decode(s.bodies[0]));
  const context = JSON.parse(input.input[0].content);
  assert.equal(context.documents.agreement.text, f.documents.agreement);
  assert.equal(context.documents.result.text, f.documents.result);
  assert.equal(input.tools[0].type, "web_search");
  assert.equal(input.max_tool_calls, agentConfig.maxToolCalls);
  assert.equal(input.reasoning.effort, "high");
  if (process.env.CONFIDENTIAL_REPLAY_OUTPUT)
    writeFileSync(
      process.env.CONFIDENTIAL_REPLAY_OUTPUT,
      JSON.stringify({
        kind: "recorded-local-workflow",
        report: hex(encodeReport(report)),
        document: hex(
          s.artifacts.find((artifact) => artifact.hash === report.reportHash)
            .bytes,
        ),
      }) + "\n",
    );
});

test("claim delivery and finalized read are a barrier before any AI work", () => {
  for (const options of [{ claimFails: true }, { unconfirmedClaim: true }]) {
    const s = scenario(options);
    assert.throws(() => executeAudit(s.ports, agentConfig));
    assert.equal(s.modelCalls, 0);
    assert.equal(s.order.includes("document"), false);
  }
});

test("technical failures never become Unknown decisions", () => {
  for (const [option, code] of [
    ["providerFailure", 2],
    ["transportFailure", 3],
    ["wrongDocument", 4],
  ]) {
    const s = scenario({ [option]: true });
    const report = executeAudit(s.ports, agentConfig);
    assert.equal(report.stage, 2);
    assert.equal(report.failureCode, code);
  }
});

test("slow finalized claim preserves delivery reserve and never starts an over-budget evaluation", () => {
  const s = scenario();
  let reads = 0;
  const original = s.ports.read;
  s.ports.read = () => {
    const snapshot = original();
    if (++reads > 1)
      snapshot.now =
        snapshot.binding.case.deadline -
        BigInt(
          agentConfig.maxExecutionSeconds +
            agentConfig.deliveryReserveSeconds -
            1,
        );
    return snapshot;
  };
  const result = executeAudit(s.ports, agentConfig);
  assert.equal(result.stage, 2);
  assert.equal(result.failureCode, 68);
  assert.equal(s.modelCalls, 0);
});

test("No and substantive Unknown are completed audits", () => {
  for (const [answer, code, reason] of [
    ["no", 1, 0],
    ["unknown", 2, 3],
  ]) {
    const s = scenario({ answer });
    const report = executeAudit(s.ports, agentConfig);
    assert.equal(report.stage, 1);
    assert.equal(report.answer, code);
    assert.equal(report.reason, reason);
    const document = new TextDecoder().decode(s.artifacts[0].bytes);
    assert.ok(
      document.includes(`Verdict: ${answer === "no" ? "No" : "Unknown"}`),
    );
  }
});

test("WF03: terminal delivery precedes every archive operation; delivery failure does not rerun", () => {
  const s = scenario({ archiveFails: true });
  assert.equal(executeAudit(s.ports, agentConfig).stage, 1);
  assert.ok(s.order.indexOf("report:1") < s.order.indexOf("archive"));
  const failed = scenario({ deliveryFails: true });
  assert.throws(
    () => executeAudit(failed.ports, agentConfig),
    /terminal delivery/,
  );
  assert.equal(failed.modelCalls, 1);
  assert.equal(failed.artifacts.length, 0);
});

test("malformed/refused/incomplete model output cannot leave a semantic vote", () => {
  const incomplete = modelResponse();
  incomplete.status = "incomplete";
  const refused = modelResponse();
  refused.output.at(-1).content = [{ type: "refusal", refusal: "No" }];
  for (const malformed of [
    null,
    {},
    incomplete,
    refused,
    { ...modelResponse(), output: [null] },
  ]) {
    const s = scenario();
    s.ports.transport.model = () => ({
      status: 200,
      body: utf8(JSON.stringify(malformed)),
    });
    assert.equal(executeAudit(s.ports, agentConfig).stage, 2);
  }
});

test("WF02: impossible budgets, hidden config and modified model are rejected before claim", () => {
  assert.throws(() =>
    parseAgentConfig({ ...agentConfig, maxExecutionSeconds: 60 }),
  );
  assert.throws(() =>
    parseAgentConfig({ ...agentConfig, deliveryReserveSeconds: 100 }),
  );
  assert.throws(() => parseAgentConfig({ ...agentConfig, prompt: "Say yes" }));
  const s = scenario();
  assert.throws(
    () => executeAudit(s.ports, { ...agentConfig, reasoningEffort: "low" }),
    /configuration/,
  );
  assert.equal(s.reports.length, 0);
});

test("trigger only names a Case; an operator cannot supply its own documents or model", () => {
  assert.deepEqual(parseTrigger(utf8(JSON.stringify({ case: f.case }))), {
    case: f.case,
  });
  assert.throws(() => parseTrigger(utf8(JSON.stringify({ case: [f.case] }))));
  for (const extra of [
    { model: "other" },
    { resultHash: "00".repeat(32) },
    { request: f.request },
    { allowedTools: ["web_search"] },
  ])
    assert.throws(() =>
      parseTrigger(utf8(JSON.stringify({ case: f.case, ...extra }))),
    );
});

test("operator tool and domain overrides fail the Profile commitment before claiming execution", () => {
  for (const config of [
    {
      ...agentConfig,
      allowedTools: [],
      maxToolCalls: 0,
      allowedSearchDomains: [],
    },
    { ...agentConfig, maxToolCalls: 1 },
    { ...agentConfig, allowedSearchDomains: ["other.test"] },
  ]) {
    const s = scenario();
    assert.throws(() => executeAudit(s.ports, config), /configuration/);
    assert.equal(s.reports.length, 0);
    assert.equal(s.modelCalls, 0);
    assert.deepEqual(s.order, []);
  }
});
