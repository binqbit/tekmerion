/** Execute the actual compiled artifact with WASI and recorded CRE host replies.
 * The host has no network, environment credentials or filesystem preopens. */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { SDK_PB } from "@chainlink/cre-sdk/pb";

const input = JSON.parse(readFileSync(0, "utf8"));
const wasi = new WASI({
  version: "preview1",
  args: ["chainlink-appeals", input.request],
  env: {},
  returnOnExit: true,
});
const module = new WebAssembly.Module(readFileSync(input.wasm));
let instance;
const responses = [],
  calls = [],
  modes = [],
  secretRequests = [];
let versionCalls = 0,
  awaited = 0,
  secretsAwaited = 0;
const bytes = (pointer, length) =>
  new Uint8Array(instance.exports.memory.buffer, pointer, length);
const forbidden = () => {
  throw new Error("Unexpected WASM host operation");
};
function respond(data, destination, capacity) {
  assert.ok(data.length <= capacity, "Host response exceeds WASM capacity");
  bytes(destination, data.length).set(data);
  return BigInt(data.length);
}

function nativeReportReply(metadata) {
  const request = fromBinary(
    SDK_PB.CapabilityRequestSchema,
    Buffer.from(calls.at(-1), "base64"),
  );
  assert.equal(request.method, "Report");
  const report = fromBinary(SDK_PB.ReportRequestSchema, request.payload.value);
  const rawReport = Buffer.concat([
    Buffer.from(metadata, "base64"),
    report.encodedPayload,
  ]);
  return toBinary(
    SDK_PB.AwaitCapabilitiesResponseSchema,
    create(SDK_PB.AwaitCapabilitiesResponseSchema, {
      responses: {
        [request.callbackId]: {
          response: {
            case: "payload",
            value: {
              typeUrl:
                "type.googleapis.com/" + SDK_PB.ReportResponseSchema.typeName,
              value: toBinary(
                SDK_PB.ReportResponseSchema,
                create(SDK_PB.ReportResponseSchema, { rawReport }),
              ),
            },
          },
        },
      },
    }),
  );
}
const env = {
  version_v2_typescript: () => {
    versionCalls++;
  },
  random_seed: () => 1n,
  now: (pointer) => {
    new DataView(instance.exports.memory.buffer).setBigUint64(
      pointer,
      1000000000000n,
      true,
    );
    return 0;
  },
  switch_modes: (mode) => {
    modes.push(mode);
  },
  log: forbidden,
  get_secrets: (pointer, length) => {
    assert.ok(input.secretReplies, "Unexpected secret access");
    secretRequests.push({
      request: Buffer.from(bytes(pointer, length)).toString("base64"),
      afterCalls: calls.length,
    });
    return 0n;
  },
  await_secrets: (_pointer, _length, destination, capacity) => {
    const reply = input.secretReplies?.[secretsAwaited++];
    assert.ok(reply, "No recorded reply for WASM secret request");
    return respond(Buffer.from(reply, "base64"), destination, capacity);
  },
  call_capability: (pointer, length) => {
    calls.push(Buffer.from(bytes(pointer, length)).toString("base64"));
    return 0n;
  },
  await_capabilities: (_pointer, _length, destination, capacity) => {
    const reply = input.capabilityReplies?.[awaited++];
    assert.ok(reply, "No recorded reply for WASM capability call");
    const data =
      typeof reply === "string"
        ? Buffer.from(reply, "base64")
        : nativeReportReply(reply.reportMetadata);
    return respond(data, destination, capacity);
  },
  send_response: (pointer, length) => {
    responses.push(Buffer.from(bytes(pointer, length)).toString("base64"));
    return 0;
  },
};
instance = new WebAssembly.Instance(module, { env, ...wasi.getImportObject() });
const exit = wasi.start(instance);
assert.equal(exit, 0, "WASM entrypoint failed");
assert.equal(versionCalls, 1, "Entrypoint must initialize the CRE runtime");
assert.equal(
  responses.length,
  1,
  "Entrypoint must return exactly one CRE response",
);
process.stdout.write(
  JSON.stringify({
    response: responses[0],
    calls,
    modes,
    awaited,
    secretRequests,
    secretsAwaited,
  }),
);
