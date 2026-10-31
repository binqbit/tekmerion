import { createHash } from "node:crypto";
import { recoverMessageAddress } from "viem";
import { address } from "@tekmerion/sdk/wire";

const uuid =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

/** Authenticate a CRE ETH JWT; the caller may supply only a Case. */
export async function authenticate(
  body,
  authorization,
  config,
  workflowId,
  now,
) {
  if (!authorization?.startsWith("Bearer ")) throw new Error("Unauthorized");
  const parts = authorization.slice(7).split(".");
  if (parts.length !== 3 || parts.some((part) => !/^[\w-]+$/.test(part))) {
    throw new Error("Unauthorized");
  }
  const [header, claims] = parts
    .slice(0, 2)
    .map((part) => JSON.parse(Buffer.from(part, "base64url").toString("utf8")));
  const signature = Buffer.from(parts[2], "base64url");
  if (header.alg !== "ETH" || header.typ !== "JWT" || signature.length !== 65) {
    throw new Error("Unauthorized");
  }
  const digest = "0x" + createHash("sha256").update(body).digest("hex");
  if (
    claims.digest !== digest ||
    claims.iss?.toLowerCase() !== config.triggerKey.toLowerCase() ||
    !Number.isSafeInteger(claims.iat) ||
    !Number.isSafeInteger(claims.exp) ||
    claims.iat > now + 5 ||
    claims.iat < now - 60 ||
    claims.exp <= now ||
    claims.exp > claims.iat + 60
  )
    throw new Error("Unauthorized");
  const signer = await recoverMessageAddress({
    message: `${parts[0]}.${parts[1]}`,
    signature: "0x" + signature.toString("hex"),
  });
  if (signer.toLowerCase() !== config.triggerKey.toLowerCase()) {
    throw new Error("Unauthorized");
  }
  const request = JSON.parse(body);
  if (
    request.jsonrpc !== "2.0" ||
    request.method !== "workflows.execute" ||
    !uuid.test(request.id) ||
    request.id !== claims.jti ||
    request.params?.workflow?.workflowID !== workflowId ||
    Object.keys(request).sort().join() !== "id,jsonrpc,method,params" ||
    Object.keys(request.params).sort().join() !== "input,workflow" ||
    Object.keys(request.params.workflow).join() !== "workflowID" ||
    Object.keys(request.params.input ?? {}).join() !== "case"
  )
    throw new Error("Invalid execution request");
  address(request.params.input.case);
  return { id: request.id, case: request.params.input.case };
}
