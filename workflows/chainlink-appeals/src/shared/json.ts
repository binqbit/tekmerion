import { check } from "@tekmerion/sdk/wire";

/** Stable key ordering for protocol bytes and content-addressed artifacts. */
export function canonical(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  const object = value as Record<string, unknown>;
  return (
    "{" +
    Object.keys(object)
      .sort()
      .map((key) => JSON.stringify(key) + ":" + canonical(object[key]))
      .join(",") +
    "}"
  );
}

/** Require the declared fields and reject undeclared configuration overrides. */
export function object(
  value: unknown,
  keys: readonly string[],
  optionalKeys: readonly string[] = [],
): Record<string, unknown> {
  check(
    value !== null && typeof value === "object" && !Array.isArray(value),
    "Expected object",
  );
  const fields = value as Record<string, unknown>;
  check(
    Object.keys(fields).sort().join(",") ===
      [...keys, ...optionalKeys.filter((key) => Object.hasOwn(fields, key))]
        .sort()
        .join(","),
    "Unexpected schema fields",
  );
  return fields;
}
