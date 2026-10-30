import assert from "node:assert/strict";
import test from "node:test";
import { hash, hex, utf8 } from "@tekmerion/sdk/wire";
import { executeAudit } from "../../dist/audit/execution.js";
import {
  agentConfig,
  f,
  modelResponse,
  scenario,
} from "../support/fixtures.mjs";

test("one final text report binds the verdict, assessment and sources without retaining a dialogue", () => {
  const s = scenario();
  const response = modelResponse();
  response.provider_metadata = "private-transport-marker";
  const decision = JSON.parse(response.output.at(-1).content[0].text);
  decision.explanation =
    "## Agreement criteria\nThe published release satisfies the requirements.\n\n## Supporting facts\nSee https://evidence.test/release for delivery and https://evidence.test/review for confirmation.";
  response.output.at(-1).content[0].text = JSON.stringify(decision);
  s.ports.transport.model = () => ({
    status: 200,
    body: utf8(JSON.stringify(response)),
  });
  const result = executeAudit(s.ports, agentConfig);
  assert.equal(result.stage, 1);
  assert.equal(s.artifacts.length, 1);
  const document = s.artifacts[0].bytes;
  assert.equal(result.reportHash, hex(hash(document)));
  const text = new TextDecoder().decode(document);
  assert.match(text, /^# Audit report\n/);
  assert.ok(text.includes("## Audit context\n\n- Case: " + f.case));
  assert.ok(text.includes("### Agreement criteria\n\nThe published release"));
  assert.ok(text.includes("\n\n### Supporting facts\n\nSee https://"));
  assert.equal(text.includes("\n## Agreement criteria"), false);
  assert.ok(text.includes(f.case));
  for (const url of decision.sources) assert.ok(text.includes(url));
  assert.ok(!text.includes("private-transport-marker"));
  assert.ok(!text.includes("web_search_call"));
  assert.ok(!text.includes('"instructions"'));
  assert.equal("receiptHash" in result, false);
  assert.equal("evidenceHash" in result, false);
});

test("assessment headings stay nested and spaced without changing lists or fenced evidence", () => {
  const s = scenario();
  const response = modelResponse();
  const decision = JSON.parse(response.output.at(-1).content[0].text);
  const evidence = [
    "```markdown",
    "# Quoted source heading",
    "Source content without an added blank line.",
    "",
    "",
    "```",
  ].join("\n");
  decision.explanation = [
    "# Agreement criteria",
    "The release must include:",
    "",
    "- A published version",
    "  - Passing export checks",
    "## Evidence",
    evidence,
    "# Conclusion",
    "The criteria are satisfied.",
  ].join("\r\n");
  response.output.at(-1).content[0].text = JSON.stringify(decision);
  s.ports.transport.model = () => ({
    status: 200,
    body: utf8(JSON.stringify(response)),
  });
  const result = executeAudit(s.ports, agentConfig);
  const document = s.artifacts[0].bytes;
  const text = new TextDecoder().decode(document);
  assert.equal(result.reportHash, hex(hash(document)));
  assert.ok(
    text.includes("### Agreement criteria\n\nThe release must include:"),
  );
  assert.ok(text.includes("- A published version\n  - Passing export checks"));
  assert.ok(text.includes("\n\n#### Evidence\n\n" + evidence));
  assert.ok(text.includes("\n\n### Conclusion\n\nThe criteria are satisfied."));
});

test("long final assessments are preserved without an extra artifact-size policy", () => {
  const s = scenario();
  const response = modelResponse();
  const decision = JSON.parse(response.output.at(-1).content[0].text);
  decision.explanation = "The evidence supports this criterion. ".repeat(300);
  response.output.at(-1).content[0].text = JSON.stringify(decision);
  s.ports.transport.model = () => ({
    status: 200,
    body: utf8(JSON.stringify(response)),
  });
  assert.equal(executeAudit(s.ports, agentConfig).stage, 1);
  assert.ok(
    new TextDecoder()
      .decode(s.artifacts[0].bytes)
      .includes(decision.explanation.trim()),
  );
});

test("technical failures produce an explicit failure notice without a fabricated agent report", () => {
  const s = scenario();
  s.ports.transport.model = () => {
    throw new Error("secret-transport-detail");
  };
  const result = executeAudit(s.ports, agentConfig);
  assert.equal(result.stage, 2);
  assert.equal(s.artifacts.length, 1);
  const document = s.artifacts[0].bytes;
  assert.equal(result.reportHash, hex(hash(document)));
  const text = new TextDecoder().decode(document);
  assert.match(text, /^# Audit execution failure/);
  assert.ok(text.includes("No completed audit verdict"));
  assert.ok(!text.includes("secret-transport-detail"));
});
