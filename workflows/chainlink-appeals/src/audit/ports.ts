/** Effects required by the audit lifecycle; native CRE transports implement them. */
import type {
  AuditReport,
  validateConfidentialBinding,
} from "@tekmerion/chainlink-sdk/wire";
import type { ResearchTransport } from "../research/transport.js";

export type AuditBinding = ReturnType<typeof validateConfidentialBinding>;

export interface AuditSnapshot {
  binding: AuditBinding;
  now: bigint;
}

export interface ExecutionPorts {
  read(): AuditSnapshot;
  executionId(): string;
  deliver(report: AuditReport): void;
  publish(hash: string, bytes: Uint8Array): void;
  transport: ResearchTransport;
}
