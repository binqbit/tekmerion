import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { parseWorkflowConfig } from "../../dist/workflow/config.js";
import {
  agentConfigHash,
  parseAgentConfig,
} from "../../dist/research/config.js";
import { agentConfig, f } from "../support/fixtures.mjs";

const settings = () => ({
  network: "mainnet",
  policy: f.policy,
  triggerKey: "0x" + "22".repeat(20),
  agent: agentConfig,
});

test("document fallbacks and the model share a bounded execution window", () => {
  const config = {
    ...agentConfig,
    documentTimeoutSeconds: 10,
    researchTimeoutSeconds: 110,
    decisionTimeoutSeconds: 70,
    gatewayOrigins: [
      "https://one.test",
      "https://two.test",
      "https://three.test",
    ],
  };
  assert.doesNotThrow(() => parseAgentConfig(config)); // 6 * 10 + 110 + 70 = 240
  assert.throws(
    () => parseAgentConfig({ ...config, documentTimeoutSeconds: 11 }),
    /HTTP budget/,
  );
  assert.throws(
    () => parseAgentConfig({ ...config, maxExecutionSeconds: 239 }),
    /HTTP budget/,
  );
  assert.throws(
    () => parseAgentConfig({ ...config, deliveryReserveSeconds: 61 }),
    /CRE run limit/,
  );
  for (const [field, maximum] of [
    ["documentTimeoutSeconds", 30],
    ["researchTimeoutSeconds", 180],
    ["decisionTimeoutSeconds", 180],
  ]) {
    for (const invalid of [0, 4, 5.5, maximum + 1, "10", null]) {
      assert.throws(() =>
        parseAgentConfig({ ...agentConfig, [field]: invalid }),
      );
    }
  }
});

test("all stage timeouts are committed and the old shared timeout cannot override them", () => {
  const digest = agentConfigHash(agentConfig);
  for (const field of [
    "documentTimeoutSeconds",
    "researchTimeoutSeconds",
    "decisionTimeoutSeconds",
  ]) {
    const changed = { ...agentConfig, [field]: agentConfig[field] - 1 };
    assert.notEqual(agentConfigHash(changed), digest);
    const missing = { ...agentConfig };
    delete missing[field];
    assert.throws(() => parseAgentConfig(missing));
  }
  assert.throws(() =>
    parseAgentConfig({ ...agentConfig, httpTimeoutSeconds: 60 }),
  );
});

test("the example only needs deployment identities replaced to pass configuration validation", () => {
  const example = JSON.parse(
    readFileSync(new URL("../../config.example.json", import.meta.url), "utf8"),
  );
  assert.deepEqual(
    Object.entries(example)
      .filter(
        ([, value]) =>
          typeof value === "string" && value.startsWith("REPLACE_WITH_"),
      )
      .map(([key]) => key)
      .sort(),
    ["policy", "triggerKey"],
  );
  assert.doesNotThrow(() =>
    parseWorkflowConfig({
      ...example,
      policy: f.policy,
      triggerKey: settings().triggerKey,
    }),
  );
});

test("archive configuration is optional and does not change the committed agent policy", () => {
  const disabled = parseWorkflowConfig(settings());
  const enabled = parseWorkflowConfig({
    ...settings(),
    archiveUrl: "https://archive.test/v1/audit-artifacts",
  });
  assert.equal(disabled.archiveUrl, undefined);
  assert.equal(agentConfigHash(disabled.agent), agentConfigHash(enabled.agent));
  assert.doesNotThrow(() =>
    parseWorkflowConfig({ ...settings(), archiveUrl: "" }),
  );
});

test("optional archiving never relaxes required settings or accepts hidden configuration", () => {
  const missingAgent = settings();
  delete missingAgent.agent;
  assert.throws(() => parseWorkflowConfig(missingAgent));
  assert.throws(() =>
    parseWorkflowConfig({ ...settings(), modelSecret: "OTHER_KEY" }),
  );
  assert.throws(() =>
    parseWorkflowConfig({ ...settings(), archiveSecret: "OTHER_KEY" }),
  );
  assert.throws(() => parseWorkflowConfig({ ...settings(), hidden: true }));
  assert.throws(() => parseWorkflowConfig({ ...settings(), archiveUrl: 42 }));
  assert.throws(() =>
    parseWorkflowConfig({ ...settings(), archiveUrl: "http://archive.test" }),
  );
});
