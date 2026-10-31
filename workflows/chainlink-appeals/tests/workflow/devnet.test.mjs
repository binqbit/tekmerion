import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { PublicKey } from "@tekmerion/sdk";
import { encodeConfig } from "@tekmerion/chainlink-sdk/wire";
import {
  createRelease,
  DEVNET,
  enrollmentConfig,
  inspectDevnet,
} from "../../tools/simulator/devnet.mjs";

const fixture = JSON.parse(
  await readFile(
    new URL(
      "../../../../sdk/fixtures/chainlink/confidential-devnet.json",
      import.meta.url,
    ),
  ),
);
const example = JSON.parse(
  await readFile(new URL("../../config.example.json", import.meta.url)),
);
const config = {
  ...example,
  network: "devnet",
  policy: fixture.policy,
  triggerKey: "0x" + "12".repeat(20),
  agent: fixture.agentConfig,
};
const consumer = "MNTxcR8N6rWisHYbt73SPyvBZqQUQiW1NKhGa3N1etM";
const loader = "BPFLoaderUpgradeab1e11111111111111111111111";

function rpcFixture() {
  const records = fixture.snapshots.prepared.map((a) => ({
    owner: a.owner,
    executable: false,
    data: [Buffer.from(a.data, "hex").toString("base64"), "base64"],
  }));
  const bytes = Buffer.from(records[0].data[0], "base64");
  bytes.set(encodeConfig(enrollmentConfig(config, 3)), 74);
  records[0].data[0] = bytes.toString("base64");
  const calls = [];
  const rpc = async (method, params) => {
    calls.push([method, params]);
    if (method === "getGenesisHash") return DEVNET.genesisHash;
    if (params[0].length === 3)
      return {
        context: { slot: 123 },
        value: [
          records[0],
          records[1],
          {
            owner: DEVNET.forwarderProgram,
            executable: false,
            data: ["", "base64"],
          },
        ],
      };
    return {
      context: { slot: 124 },
      value: params[0].map((key, index) => {
        const bytes = Buffer.alloc(index % 2 ? 49 : 36);
        bytes.writeUInt32LE(index % 2 ? 3 : 2);
        if (index % 2 === 0)
          bytes.set(new PublicKey(params[0][index + 1]).toBytes(), 4);
        else bytes.set([1, 2, 3, 4], 45);
        return {
          owner: loader,
          executable: index % 2 === 0,
          data: [bytes.toString("base64"), "base64"],
        };
      }),
    };
  };
  return { records, calls, rpc };
}

test("Devnet enrollment binds the official CLI transport and the exact agent configuration", () => {
  const enrollment = enrollmentConfig(config, 3);
  assert.equal(enrollment.workflowId, "1".repeat(64));
  assert.equal(enrollment.forwarderProgram, DEVNET.forwarderProgram);
  assert.equal(enrollment.maxAttempts, 3);
  for (const count of [0, -1, 1.5, 65536])
    assert.throws(() => enrollmentConfig(config, count));
  assert.throws(() => enrollmentConfig({ ...config, network: "mainnet" }, 1));
});

test("release export reads finalized accounts and pins actual program bytes without transactions", async () => {
  const f = rpcFixture();
  const release = await createRelease(
    config,
    f.rpc,
    "http://cre-simulator:2000/",
    consumer,
  );
  assert.equal(release.profile, fixture.profile);
  assert.equal(
    release.profileHash,
    createHash("sha256")
      .update(Buffer.from(f.records[0].data[0], "base64"))
      .digest("hex"),
  );
  assert.equal(release.programs.length, 4);
  assert.ok(
    release.programs.every(
      (p) =>
        p.binaryHash ===
        createHash("sha256")
          .update(Buffer.from([1, 2, 3, 4]))
          .digest("hex"),
    ),
  );
  assert.equal(release.forwarderState, DEVNET.forwarderState);
  assert.equal(f.calls.at(-1)[1][1].minContextSlot, 123);
  assert.ok(
    f.calls.every(
      ([method]) =>
        method === "getGenesisHash" || method === "getMultipleAccounts",
    ),
  );
});

test("preflight rejects the wrong cluster, missing programs and incompatible immutable Profiles", async () => {
  const f = rpcFixture();
  await assert.rejects(
    inspectDevnet(config, async (method, params) =>
      method === "getGenesisHash" ? "wrong-chain" : f.rpc(method, params),
    ),
    /not Devnet/,
  );
  const bytes = Buffer.from(f.records[0].data[0], "base64");
  bytes[74] ^= 1;
  f.records[0].data[0] = bytes.toString("base64");
  await assert.rejects(inspectDevnet(config, f.rpc), /forwarderProgram/);
  const valid = rpcFixture();
  await assert.rejects(
    createRelease(
      config,
      async (method, params) =>
        params?.[0]?.length === 8
          ? { context: { slot: 124 }, value: Array(8).fill(null) }
          : valid.rpc(method, params),
      "http://cre-simulator:2000/",
      consumer,
    ),
    /Deploy program/,
  );
});
