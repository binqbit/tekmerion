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
