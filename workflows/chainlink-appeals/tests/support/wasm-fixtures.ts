/** Recorded host replies for the compiled workflow; no workflow source or dist imports. */
import { readFileSync } from "node:fs";
import {
  create,
  toBinary,
  type DescMessage,
  type MessageInitShape,
} from "@bufbuild/protobuf";
import { address } from "@tekmerion/sdk/wire";
import { SDK_PB, SOLANA_PB, HTTP_CLIENT_PB } from "@chainlink/cre-sdk/pb";

const utf8 = (value: string) => new TextEncoder().encode(value);

// The artifact tests use only committed input data, never the TypeScript dist tree.
export const f = JSON.parse(
  readFileSync(
    new URL(
      "../../../../sdk/fixtures/chainlink/confidential-mainnet.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
export const agentConfig = f.agentConfig;

export function capabilityReply<S extends DescMessage>(
  id: number,
  schema: S,
  value: MessageInitShape<S>,
) {
  return toBinary(
    SDK_PB.AwaitCapabilitiesResponseSchema,
    create(SDK_PB.AwaitCapabilitiesResponseSchema, {
      responses: {
        [id]: {
          response: {
            case: "payload",
            value: {
              typeUrl: "type.googleapis.com/" + schema.typeName,
              value: toBinary(schema, create(schema, value)),
            },
          },
        },
      },
    }),
  );
}

export function recordedAudit(mode: string) {
  const replies: (Uint8Array | { reportMetadata: Uint8Array })[] = [];
  const secretReplies: Uint8Array[] = [];
  let callback = 0;
  function reply<S extends DescMessage>(schema: S, value: MessageInitShape<S>) {
    replies.push(capabilityReply(callback++, schema, value));
  }
  function read(stage: "prepared" | "claimed") {
    const records = f.snapshots[stage].map(
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
    reply(SOLANA_PB.GetMultipleAccountsWithOptsReplySchema, {
      value: [records[2]],
    });
    reply(SOLANA_PB.GetMultipleAccountsWithOptsReplySchema, {
      value: [
        ...records,
        {
          account: {
            owner: address("Sysvar1111111111111111111111111111111111111"),
            data: { body: { case: "raw", value: clock } },
          },
        },
      ],
    });
  }
  function nativeReport(id: number) {
    const metadata = new Uint8Array(109);
    metadata[0] = 1;
    metadata.set(new Uint8Array(32).fill(1), 1);
    metadata.set(new Uint8Array(32).fill(0x11), 45);
    metadata[108] = id;
    callback++;
    replies.push({ reportMetadata: metadata });
  }
  function secret(id: string) {
    secretReplies.push(
      toBinary(
        SDK_PB.AwaitSecretsResponseSchema,
        create(SDK_PB.AwaitSecretsResponseSchema, {
          responses: {
            [callback++]: {
              responses: [
                {
                  response: {
                    case: "secret",
                    value: {
                      id,
                      namespace: "main",
                      value: "synthetic-" + id,
                    },
                  },
                },
              ],
            },
          },
        }),
      ),
    );
  }
  const delivered = () =>
    reply(SOLANA_PB.WriteReportReplySchema, {
      txStatus: SOLANA_PB.TxStatus.SUCCESS,
      txSignature: new Uint8Array(64),
    });
  const http = (body: string, statusCode = 200) =>
    reply(HTTP_CLIENT_PB.ResponseSchema, { statusCode, body: utf8(body) });

  read("prepared");
  read("prepared");
  nativeReport(1);
  nativeReport(2);
  delivered();
  read("claimed");
  http(f.documents.agreement);
  http(f.documents.result);
  secret("MODEL_KEY");
  const answer = ["no", "unknown"].includes(mode) ? mode : "yes";
  http(
    JSON.stringify({
      id: "resp_recorded_wasm",
      model: agentConfig.model,
      status: "completed",
      usage: { output_tokens: 100 },
      ...(mode === "oversized-model"
        ? { padding: "x".repeat(agentConfig.maxResponseBytes) }
        : {}),
      output: [
        {
          type: "web_search_call",
          status: "completed",
          action: {
            type: mode === "no" ? "find_in_page" : "open_page",
            url: "https://evidence.test/review",
            ...(mode === "no" ? { pattern: "acceptance criteria" } : {}),
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
                findings: "Synthetic review establishes acceptance criteria.",
              }),
            },
          ],
        },
      ],
    }),
    mode === "provider-failure" ? 503 : 200,
  );
  if (!["provider-failure", "oversized-model"].includes(mode)) {
    secret("MODEL_KEY");
    http(
      JSON.stringify({
        id: "resp_recorded_decision",
        model: agentConfig.model,
        status: "completed",
        usage: { output_tokens: 100 },
        output: [
          {
            type: "message",
            role: "assistant",
            status: "completed",
            content: [
              {
                type: "output_text",
                text: JSON.stringify({
                  answer,
                  reason:
                    answer === "unknown" ? "conflicting_evidence" : "decisive",
                  explanation:
                    "### Findings\nSynthetic evidence for the recorded WASM audit. [S1]",
                  sourceIds: ["S1"],
                }),
              },
            ],
          },
        ],
      }),
      mode === "decision-failure" ? 503 : 200,
    );
  }
  nativeReport(3);
  delivered();
  if (mode !== "no-archive") {
    secret("ARCHIVE_KEY");
    http("{}", mode === "archive-failure" ? 503 : 200);
  }
  return { replies, secretReplies };
}
