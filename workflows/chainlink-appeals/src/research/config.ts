/** Semantic execution policy. Its canonical hash is pinned in the adapter Profile. */
import { check, utf8 } from "@tekmerion/sdk/wire";
import { sha256 } from "../shared/bytes.js";
import { object, canonical } from "../shared/json.js";
import { parseUrl, publicUrl } from "../shared/url.js";

export interface AgentConfig {
  model: string;
  reasoningEffort: "low" | "medium" | "high";
  allowedTools: "web_search"[];
  maxOutputTokens: number;
  maxToolCalls: number;
  maxExecutionSeconds: number;
  deliveryReserveSeconds: number;
  httpTimeoutSeconds: number;
  maxDocumentBytes: number;
  maxResponseBytes: number;
  gatewayOrigins: string[];
  allowedSearchDomains: string[];
}

export function parseAgentConfig(value: unknown): AgentConfig {
  const fields = object(value, [
    "model",
    "reasoningEffort",
    "allowedTools",
    "maxOutputTokens",
    "maxToolCalls",
    "maxExecutionSeconds",
    "deliveryReserveSeconds",
    "httpTimeoutSeconds",
    "maxDocumentBytes",
    "maxResponseBytes",
    "gatewayOrigins",
    "allowedSearchDomains",
  ]);
  check(
    typeof fields.model === "string" &&
      /^[a-z0-9][a-z0-9.-]{1,100}-\d{4}-\d{2}-\d{2}$/.test(fields.model),
    "Pin an explicit model revision",
  );
  check(
    typeof fields.reasoningEffort === "string" &&
      ["low", "medium", "high"].includes(fields.reasoningEffort),
    "Invalid reasoning effort",
  );
  const bounded = (key: string, min: number, max: number) => {
    check(
      Number.isSafeInteger(fields[key]) &&
        Number(fields[key]) >= min &&
        Number(fields[key]) <= max,
      "Invalid " + key,
    );
  };
  bounded("maxOutputTokens", 1024, 16000);
  bounded("maxToolCalls", 0, 16);
  bounded("maxExecutionSeconds", 30, 240);
  bounded("deliveryReserveSeconds", 30, 120);
  bounded("httpTimeoutSeconds", 5, 90);
  bounded("maxDocumentBytes", 1024, 16384);
  // Bound the provider response received within one CRE execution.
  bounded("maxResponseBytes", 8192, 100000);
  check(
    Array.isArray(fields.gatewayOrigins) &&
      fields.gatewayOrigins.length >= 1 &&
      fields.gatewayOrigins.length <= 3,
    "Use one to three IPFS gateways",
  );
  for (const origin of fields.gatewayOrigins) {
    publicUrl(String(origin) + "/");
    check(
      parseUrl(String(origin)).origin === origin,
      "Expected gateway origin",
    );
  }
  check(
    Array.isArray(fields.allowedTools) &&
      fields.allowedTools.every((tool) => tool === "web_search") &&
      new Set(fields.allowedTools).size === fields.allowedTools.length,
    "Explicitly allow supported tools without duplicates",
  );
  const webSearchAllowed = fields.allowedTools.includes("web_search");
  check(
    webSearchAllowed
      ? Number(fields.maxToolCalls) > 0
      : fields.maxToolCalls === 0,
    "Tool budget must match allowed tools",
  );
  check(
    Array.isArray(fields.allowedSearchDomains) &&
      (webSearchAllowed
        ? fields.allowedSearchDomains.length >= 1 &&
          fields.allowedSearchDomains.length <= 100
        : fields.allowedSearchDomains.length === 0),
    "Pin search domains only when web_search is allowed",
  );
  for (const domain of fields.allowedSearchDomains) {
    check(
      typeof domain === "string" && /^[a-z0-9.-]+\.[a-z]{2,}$/.test(domain),
      "Invalid search domain",
    );
    publicUrl("https://" + domain + "/");
  }
  const config = value as AgentConfig;
  check(
    new Set(config.gatewayOrigins).size === config.gatewayOrigins.length &&
      new Set(config.allowedSearchDomains).size ===
        config.allowedSearchDomains.length,
    "Duplicate execution policy entry",
  );
  // One model call plus both documents, including all gateway fallbacks.
  const longestHttpPath =
    (2 * config.gatewayOrigins.length + 1) * config.httpTimeoutSeconds;
  check(
    longestHttpPath <= config.maxExecutionSeconds,
    "HTTP budget cannot fit the execution window",
  );
  check(
    config.maxExecutionSeconds + config.deliveryReserveSeconds <= 300,
    "Execution and delivery exceed the CRE run limit",
  );
  return config;
}

export function agentConfigHash(config: AgentConfig): string {
  return sha256(utf8(canonical(parseAgentConfig(config))));
}
