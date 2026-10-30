import { readFileSync } from "node:fs";
import { utf8, unhex } from "@tekmerion/sdk/wire";
import { validateConfidentialBinding } from "@tekmerion/chainlink-sdk/wire";
import { rawCid } from "../../dist/shared/ipfs.js";

export const f = JSON.parse(
  readFileSync(
    new URL(
      "../../../../sdk/fixtures/chainlink/confidential-mainnet.json",
      import.meta.url,
    ),
  ),
);
export const agentConfig = JSON.parse(
  readFileSync(
    new URL(
      "../../../../tests/fixtures/confidential/agent-config.json",
      import.meta.url,
    ),
  ),
);
const bytes = (s) => unhex(s, s.length / 2);

export function read(stage) {
  const a = f.snapshots[stage];
  const binding = validateConfidentialBinding({
    network: f.network,
    profileKey: f.profile,
    profileBytes: bytes(a[0].data),
    policyBytes: bytes(a[1].data),
    caseKey: f.case,
    caseBytes: bytes(a[2].data),
    requestKey: f.request,
    requestBytes: bytes(a[3].data),
  });
  return { binding, now: 1000n };
}

function researchFixture(options = {}) {
  const sources = [
    "https://evidence.test/release",
    "https://evidence.test/review",
  ];
  const response = {
    id: "resp_local_research",
    model: agentConfig.model,
    status: "completed",
    usage: { output_tokens: 3500 },
    output: [
      { type: "reasoning", id: "reasoning_1", summary: [] },
      {
        type: "web_search_call",
        id: "search_1",
        status: "completed",
        action: {
          type: "search",
          query: "Atlas Renderer delivery acceptance independent review",
          sources: sources.map((url) => ({ type: "url", url })),
        },
      },
      {
        type: "web_search_call",
        id: "open_1",
        status: "completed",
        action: { type: "open_page", url: sources[0] },
      },
      {
        type: "web_search_call",
        id: "search_2",
        status: "completed",
        action: {
          type: "search",
          query: "Atlas Renderer review contradictions delivery deadline",
          sources: [{ type: "url", url: sources[1] }],
        },
      },
      {
        type: "message",
        role: "assistant",
        status: "completed",
        content: [
          {
            type: "output_text",
            text: JSON.stringify({
              answer: options.answer ?? "yes",
              reason:
                options.answer === "unknown"
                  ? "conflicting_evidence"
                  : "decisive",
              explanation: [
                "### Agreement criteria",
                "",
                "The agreement requires a publicly available Atlas Renderer 2.4.0 release, an independent review confirming that image export acceptance checks pass, and publication of both sources before the audit cutoff. A planned release or incomplete review is insufficient.",
                "",
                "### Findings and sources",
                "",
                "The [official release](https://evidence.test/release) records public availability of version 2.4.0. The [independent review](https://evidence.test/review) records successful image export acceptance checks. In this synthetic scenario, both were published at 1970-01-01T00:10:00Z, before the cutoff at 1970-01-01T00:16:40Z.",
                "",
                "### Conclusion",
                "",
                "These observations support the proposed delivery claim: the version is released, the required review is complete, and both publications are timely. This fixture models a completed acceptance assessment; the cited evidence.test URLs are test sources, not live publications.",
              ].join("\n"),
              sources,
            }),
          },
        ],
      },
    ],
  };
  return response;
}

export const evidenceSources = [
  "https://evidence.test/release",
  "https://evidence.test/review",
];
export const evidence = {
  findings: "The release and independent review establish timely delivery.",
  sources: evidenceSources,
};

export function evidenceResponse() {
  const response = researchFixture();
  response.output.at(-1).content[0].text = JSON.stringify({
    findings: evidence.findings,
  });
  return response;
}

export function providerResponse(body, options = {}) {
  const request = JSON.parse(new TextDecoder().decode(body));
  return request.text.format.name === "audit_research"
    ? evidenceResponse()
    : modelResponse(options);
}

export function modelResponse(options = {}) {
  const response = researchFixture(options);
  response.id = "resp_local_decision";
  response.output = response.output.filter(
    (item) => item.type !== "web_search_call",
  );
  const decision = JSON.parse(response.output.at(-1).content[0].text);
  delete decision.sources;
  decision.sourceIds = ["S1", "S2"];
  decision.explanation = decision.explanation
    .replace(
      "[official release](https://evidence.test/release)",
      "official release [S1]",
    )
    .replace(
      "[independent review](https://evidence.test/review)",
      "independent review [S2]",
    );
  response.output.at(-1).content[0].text = JSON.stringify(decision);
  if (options.withoutTools) {
    response.output = response.output.filter(
      (item) => item.type !== "web_search_call",
    );
    const decision = JSON.parse(response.output.at(-1).content[0].text);
    decision.sourceIds = [];
    decision.explanation =
      "The supplied documents establish the agreement's criteria.";
    response.output.at(-1).content[0].text = JSON.stringify(decision);
  }
  return response;
}

export function scenario(options = {}) {
  let claimed = false,
    modelCalls = 0;
  const reports = [],
    bodies = [],
    order = [],
    artifacts = [];
  const ports = {
    read: () =>
      read(claimed && !options.unconfirmedClaim ? "claimed" : "prepared"),
    executionId: () => "01".repeat(32),
    deliver: (report) => {
      reports.push(report);
      order.push("report:" + report.stage);
      if (report.stage === 0) {
        if (options.claimFails) throw new Error("claim delivery unavailable");
        claimed = true;
      } else if (options.deliveryFails)
        throw new Error("terminal delivery unavailable");
    },
    publish: (hash, bytes) => {
      order.push("archive");
      artifacts.push({ hash, bytes });
      if (options.archiveFails) throw new Error("archive offline");
    },
    transport: {
      now: () => 1000000,
      get: (url) => {
        order.push("document");
        const context = read("prepared").binding.context;
        const which = url.endsWith(rawCid(context.agreementHash))
          ? "agreement"
          : "result";
        const body = utf8(f.documents[which]);
        if (options.wrongDocument) body[0] ^= 1;
        return { status: 200, body };
      },
      model: (body) => {
        order.push("model");
        modelCalls++;
        bodies.push(body);
        if (options.providerFailure) return { status: 503, body: utf8("{}") };
        if (options.transportFailure) throw new Error("transport timeout");
        return {
          status: 200,
          body: utf8(JSON.stringify(providerResponse(body, options))),
        };
      },
    },
  };
  return {
    ports,
    reports,
    bodies,
    order,
    artifacts,
    get modelCalls() {
      return modelCalls;
    },
  };
}
