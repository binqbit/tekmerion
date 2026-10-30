/** Research inputs and results, independent of native CRE accounts and reports. */
export interface ResearchTask {
  case: string;
  policy: string;
  subject: string;
  agreementHash: string;
  resultHash: string;
  cutoff: bigint;
}

export interface Decision {
  answer: number;
  reason: number;
  explanation: string;
  sources: string[];
}

export interface ResearchResult {
  decision: Decision;
  responseId: string;
}

/** Provider findings remain untrusted; source URLs come only from validated tool output. */
export interface Evidence {
  findings: string;
  sources: string[];
}
