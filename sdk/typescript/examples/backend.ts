import {
  Connection,
  PublicKey,
  Transaction,
  type Signer,
  type TransactionInstruction,
} from '@solana/web3.js';
import {
  createClient,
  TekmerionRpc,
  type AuditAnswer,
  type AuditReason,
  type CaseAddress,
  type Network,
  type WatchCallbacks,
  type CaseView,
  type PolicyView,
} from '../dist/index.js';

// Importing this example does not contact a network or submit a transaction.
// The application owns the Connection and supplies its own signing authority.
export async function lockAndReadPolicy(
  connection: Connection,
  network: Network,
  policy: PublicKey,
  subject: PublicKey,
  payer: Signer,
  clientAuthority: Signer,
): Promise<PolicyView> {
  const rpc = new TekmerionRpc(connection, network, 'confirmed');
  let view = await rpc.fetchPolicy(policy);
  if (!view.policy.clientAuthority.equals(clientAuthority.publicKey))
    throw new Error('The signer is not the configured client authority');

  if (view.policy.activeUses === 0n) {
    const instruction = rpc.client.acquirePolicy(policy, clientAuthority.publicKey, subject);
    const transaction = new Transaction().add(instruction);
    transaction.feePayer = payer.publicKey;
    const confirmation = await rpc.sendAndConfirm(transaction, {
      signers: [payer, clientAuthority],
    });
    view = await rpc.fetchPolicy(policy, confirmation.slot);
  }

  if (view.policy.activeUses === 0n)
    throw new Error('Policy lock has not been observed; refetch before creating a Case');

  // Ask the author to review these rules before building/signing createCase.
  // Acquisition may have locked rules that changed since the preliminary read.
  return view;
}

export async function contribute(
  connection: Connection,
  network: Network,
  identity: CaseAddress,
  owner: Signer,
  source: PublicKey,
  requested: bigint,
) {
  const rpc = new TekmerionRpc(connection, network, 'confirmed');
  const view = await rpc.fetchCase(identity);
  const accepted = view.quoteContribution(requested);
  const instruction = rpc.client.contribute(
    identity,
    { payer: owner.publicKey, owner: owner.publicKey, source },
    accepted,
  );
  const transaction = new Transaction().add(instruction);
  transaction.feePayer = owner.publicKey;
  return rpc.sendAndConfirm(transaction, { signers: [owner] });
}

export async function submitDecision(
  connection: Connection,
  network: Network,
  identity: CaseAddress,
  auditor: Signer,
  answer: AuditAnswer,
  reason: AuditReason,
  feeTokenAccounts: ReadonlyMap<string, PublicKey>,
) {
  const rpc = new TekmerionRpc(connection, network);
  const view = await rpc.fetchCase(identity);
  const payouts = view.quoteAudit(auditor.publicKey, answer, reason);
  const destinations = payouts.map(({ feeRecipient }) => {
    const tokenAccount = feeTokenAccounts.get(feeRecipient.toBase58());
    if (!tokenAccount) throw new Error(`Missing fee token account for ${feeRecipient.toBase58()}`);
    return tokenAccount;
  });
  const instruction = rpc.client.submitAudit(
    identity,
    auditor.publicKey,
    answer,
    reason,
    destinations,
  );
  const transaction = new Transaction().add(instruction);
  transaction.feePayer = auditor.publicKey;
  return rpc.sendAndConfirm(transaction, { signers: [auditor] });
}

export async function observeCase(
  connection: Connection,
  network: Network,
  identity: CaseAddress,
  callbacks: WatchCallbacks<CaseView>,
) {
  const rpc = new TekmerionRpc(connection, network);
  // Await the returned function when the application finishes watching.
  return rpc.watchCase(identity, callbacks);
}

// The unsigned instruction example also appears in README.md.
export function contributionInstruction(
  identity: CaseAddress,
  payer: PublicKey,
  owner: PublicKey,
  source: PublicKey,
  amount: bigint,
): TransactionInstruction {
  return createClient('devnet').contribute(identity, { payer, owner, source }, amount);
}
