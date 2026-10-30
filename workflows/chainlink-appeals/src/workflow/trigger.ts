import { address, check } from "@tekmerion/sdk/wire";
import { object } from "../shared/json.js";
import { decodeText } from "../shared/bytes.js";

export type RequestTrigger = { case: string };
export function parseTrigger(bytes: Uint8Array): RequestTrigger {
  check(bytes.length <= 128, "Trigger too large");
  const target = object(JSON.parse(decodeText(bytes)), ["case"]);
  check(typeof target.case === "string", "Expected Case address string");
  address(target.case);
  return target as RequestTrigger;
}
