/** Authorized execution claims, verdicts and technical failures. */
import { check, concat, hex, unhex, validateDecision } from '@tekmerion/sdk/wire';
import { BoundedReader, u16 } from './binary.js';

export function retryableFailure(code: number): boolean {
  check(
    Number.isInteger(code) && ((code >= 1 && code <= 4) || (code >= 64 && code <= 70)),
    'Invalid failure code',
  );
  return code <= 4;
}

export interface ReportPrefix {
  version: 3;
  stage: 0 | 1 | 2;
  requestDigest: string;
  attemptIndex: number;
  executionId: string;
}

export type AuditReport = ReportPrefix &
  (
    | { stage: 0 }
    | { stage: 1; reportHash: string; answer: number; reason: number }
    | { stage: 2; failureCode: number; reportHash: string }
  );

export function encodeReport(report: AuditReport): Uint8Array {
  check(report.version === 3, 'Unsupported report');
  for (const value of [report.requestDigest, report.executionId])
    check(value !== '0'.repeat(64), 'Empty execution commitment');
  const prefix = concat(
    Uint8Array.of(3, report.stage),
    unhex(report.requestDigest),
    u16(report.attemptIndex),
    unhex(report.executionId),
  );
  if (report.stage === 0) return prefix;
  check(report.stage === 1 || report.stage === 2, 'Invalid report stage');
  check(report.reportHash !== '0'.repeat(64), 'Empty report');
  if (report.stage === 1) {
    validateDecision(report.answer, report.reason);
    return concat(prefix, unhex(report.reportHash), Uint8Array.of(report.answer, report.reason));
  }
  retryableFailure(report.failureCode);
  return concat(prefix, Uint8Array.of(report.failureCode), unhex(report.reportHash));
}

export function decodeReport(bytes: Uint8Array): AuditReport {
  const reader = new BoundedReader(bytes);
  const version = reader.u8();
  const stage = reader.u8();
  check(version === 3 && stage <= 2 && bytes.length === [68, 102, 101][stage], 'Invalid report');
  const prefix = {
    version: 3 as const,
    requestDigest: hex(reader.take(32)),
    attemptIndex: reader.u16(),
    executionId: hex(reader.take(32)),
  };
  const report: AuditReport =
    stage === 0
      ? { ...prefix, stage: 0 }
      : stage === 1
        ? {
            ...prefix,
            stage: 1,
            reportHash: hex(reader.take(32)),
            answer: reader.u8(),
            reason: reader.u8(),
          }
        : { ...prefix, stage: 2, failureCode: reader.u8(), reportHash: hex(reader.take(32)) };
  reader.end();
  encodeReport(report);
  return report;
}
