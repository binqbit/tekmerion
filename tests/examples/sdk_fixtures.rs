//! Cross-language SDK vectors from Rust builders and real local program execution.
use serde_json::{Value, json};
use solana_address::Address;
use solana_instruction::Instruction;
use tekmerion::{AuditAnswer, AuditReason, CasePhase, CaseResult, CompletionMethod, PolicyRules};
use tekmerion_contract_tests::support::{
    Actor, BALANCE, BOND, MINT, TestContext, committee_rules, observed_account, pk,
};
use tekmerion_sdk::{
    accounts::ClaimState,
    instructions::{self as sdk, CaseAddress, ClaimDestination},
};

const ACTORS: [(&str, Actor); 11] = [
    ("payer", Actor::Payer),
    ("admin", Actor::Admin),
    ("author", Actor::Author),
    ("alice", Actor::Alice),
    ("bob", Actor::Bob),
    ("clientAuthority", Actor::ClientAuthority),
    ("auditor1", Actor::Auditor1),
    ("auditor2", Actor::Auditor2),
    ("auditor3", Actor::Auditor3),
    ("feeRecipient", Actor::FeeRecipient),
    ("feeRecipient2", Actor::FeeRecipient2),
];

const CLOCK: Address = Address::from_str_const("SysvarC1ock11111111111111111111111111111111");

fn main() {
    let rules = committee_rules(2, 3);
    let mut ctx = TestContext::with_rules(rules.clone(), BALANCE);
    let actors: serde_json::Map<_, _> = ACTORS
        .iter()
        .map(|(name, actor)| (name.to_string(), json!(ctx.actor(*actor).to_string())))
        .collect();
    let tokens: serde_json::Map<_, _> = ACTORS
        .iter()
        .map(|(name, actor)| (name.to_string(), json!(ctx.token(*actor).to_string())))
        .collect();
    let policy_input = json!({
        "payer": ctx.actor(Actor::Payer).to_string(),
        "admin": ctx.actor(Actor::Admin).to_string(),
        "clientAuthority": ctx.authority.to_string(),
        "identifier": "0",
        "rules": policy_rules(&rules),
    });
    let policy_before = json!({
        "observation": observation(&ctx),
        "account": account(&ctx, ctx.policy),
        "clockAccount": account(&ctx, CLOCK),
    });
    let mut instructions = vec![
        instruction(
            "createPolicy",
            "createPolicy",
            policy_input.clone(),
            ctx.create_policy_ix(rules.clone()),
        ),
        instruction(
            "updatePolicy",
            "updatePolicy",
            json!({"admin": ctx.actor(Actor::Admin).to_string(), "identifier": "0", "rules": policy_rules(&rules)}),
            ctx.update_policy_ix(rules.clone()),
        ),
        instruction(
            "acquirePolicy",
            "acquirePolicy",
            json!({"policy": ctx.policy.to_string(), "clientAuthority": ctx.authority.to_string(), "subject": ctx.subject.to_string()}),
            sdk::acquire_policy(pk(ctx.policy), pk(ctx.authority), pk(ctx.subject)),
        ),
        instruction(
            "finalizeUnchallenged",
            "finalizeUnchallenged",
            json!({"caseAddress": case_address(ctx.case_address())}),
            ctx.finalize_ix(),
        ),
        instruction(
            "finalizeTimeout",
            "finalizeTimeout",
            json!({"caseAddress": case_address(ctx.case_address())}),
            ctx.finalize_timeout_ix(),
        ),
    ];

    for nonce in [0, 1, u64::MAX] {
        let address = CaseAddress {
            nonce,
            ..ctx.case_address()
        };
        let input = sdk::CreateCase {
            payer: pk(ctx.actor(Actor::Payer)),
            client_authority: pk(ctx.authority),
            author: pk(ctx.actor(Actor::Author)),
            source: pk(ctx.token(Actor::Author)),
            author_principal: BOND,
            agreement_hash: [0xa1; 32],
            result_hash: [0x91; 32],
        };
        instructions.push(instruction("createCase", &format!("createCase-{nonce}"), json!({
            "caseAddress": case_address(address),
            "payer": input.payer.to_string(), "clientAuthority": input.client_authority.to_string(),
            "author": input.author.to_string(), "source": input.source.to_string(),
            "authorPrincipal": BOND.to_string(), "agreementHashHex": hex(&input.agreement_hash), "resultHashHex": hex(&input.result_hash),
        }), sdk::create_case(address, input)));
    }

    for requested in [1, u64::MAX] {
        instructions.push(instruction("contribute", &format!("contribute-{requested}"), json!({
            "caseAddress": case_address(ctx.case_address()),
            "payer": ctx.actor(Actor::Payer).to_string(), "owner": ctx.actor(Actor::Alice).to_string(),
            "source": ctx.token(Actor::Alice).to_string(), "requested": requested.to_string(),
        }), ctx.contribute_ix(Actor::Alice, requested)));
    }

    for (answer, reason) in [
        (AuditAnswer::Yes, AuditReason::Decisive),
        (AuditAnswer::No, AuditReason::Decisive),
        (AuditAnswer::Unknown, AuditReason::MissingEvidence),
        (AuditAnswer::Unknown, AuditReason::StaleEvidence),
        (AuditAnswer::Unknown, AuditReason::ConflictingEvidence),
        (AuditAnswer::Unknown, AuditReason::UnsupportedEvidence),
        (AuditAnswer::Unknown, AuditReason::Inconclusive),
    ] {
        for with_destinations in [false, true] {
            let destinations = if with_destinations {
                vec![
                    pk(ctx.token(Actor::FeeRecipient)),
                    pk(ctx.token(Actor::FeeRecipient2)),
                ]
            } else {
                vec![]
            };
            instructions.push(instruction("submitAudit", &format!("submitAudit-{answer:?}-{reason:?}-{with_destinations}"), json!({
                "caseAddress": case_address(ctx.case_address()), "auditor": ctx.actor(Actor::Auditor2).to_string(),
                "answer": answer_name(answer), "reason": reason_name(reason),
                "feeDestinations": destinations.iter().map(ToString::to_string).collect::<Vec<_>>(),
            }), sdk::submit_audit(ctx.case_address(), pk(ctx.actor(Actor::Auditor2)), answer, reason, &destinations)));
        }
    }

    for actor in [Actor::Author, Actor::Alice] {
        let owner = pk(ctx.actor(actor));
        let destination = pk(ctx.token(actor));
        instructions.push(instruction("claim", &format!("claim-{actor:?}"), json!({
            "caseAddress": case_address(ctx.case_address()), "author": ctx.actor(Actor::Author).to_string(),
            "owner": owner.to_string(), "destination": destination.to_string(),
        }), sdk::claim(ctx.case_address(), pk(ctx.actor(Actor::Author)), ClaimDestination { owner, destination })));
    }

    let identity = case_address(ctx.case_address());
    let mut snapshots = Vec::new();
    for outcome in ["yes", "no", "unknown", "timeout", "optimistic"] {
        if outcome != "yes" {
            ctx = TestContext::with_rules(rules.clone(), BALANCE);
        }
        ctx.create_case();
        if outcome == "yes" {
            snapshots.push(snapshot("funding-empty", &ctx));
        }
        ctx.contribute(Actor::Alice, 33);
        if outcome == "yes" {
            snapshots.push(snapshot("funding-partial", &ctx));
        }
        if outcome == "optimistic" {
            ctx.set_time(ctx.case_state().funding_deadline);
            ctx.ok(&[ctx.finalize_ix()], &[]);
        } else {
            ctx.contribute(Actor::Bob, 67);
            if outcome == "yes" {
                snapshots.push(snapshot("funded", &ctx));
            }
            if outcome == "timeout" {
                ctx.set_time(ctx.case_state().audit.deadline.unwrap());
                snapshots.push(snapshot("audit-expired", &ctx));
                ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
            } else {
                let answer = match outcome {
                    "yes" => AuditAnswer::Yes,
                    "no" => AuditAnswer::No,
                    _ => AuditAnswer::Unknown,
                };
                let decision = ctx.audit_decision(answer);
                ctx.vote(Actor::Auditor3, &decision);
                snapshots.push(snapshot(&format!("{outcome}-partial-vote"), &ctx));
                ctx.vote(Actor::Auditor1, &decision);
            }
        }
        snapshots.push(snapshot(&format!("{outcome}-finalized"), &ctx));
        ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
        snapshots.push(snapshot(&format!("{outcome}-alice-claimed"), &ctx));
        if outcome != "optimistic" {
            ctx.ok(&[ctx.contributor_claim_ix(Actor::Bob)], &[Actor::Bob]);
        }
        ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
        snapshots.push(snapshot(&format!("{outcome}-all-claimed"), &ctx));
    }

    println!("{}", serde_json::to_string_pretty(&json!({
        "network": if cfg!(feature="devnet") { "devnet" } else { "mainnet" },
        "programId": tekmerion::ID.to_string(), "mint": MINT.to_string(),
        "actors": actors, "tokens": tokens, "policyInput": policy_input, "caseAddress": identity,
        "policyBeforeCase": policy_before, "instructions": instructions, "snapshots": snapshots,
    })).unwrap());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn observation(ctx: &TestContext) -> Value {
    let clock = ctx.clock();
    json!({"slot": clock.slot.to_string(), "unixTimestamp": clock.unix_timestamp.to_string()})
}

fn account(ctx: &TestContext, key: Address) -> Value {
    let account = ctx.account(key);
    json!({"key": key.to_string(), "owner": account.owner.to_string(), "executable": account.executable,
        "dataHex": hex(&account.data), "slot": ctx.clock().slot.to_string()})
}

fn case_address(address: CaseAddress) -> Value {
    json!({"policy": address.policy.to_string(), "subject": address.subject.to_string(), "author": address.author.to_string(), "nonce": address.nonce.to_string()})
}

fn policy_rules(rules: &PolicyRules) -> Value {
    json!({
        "auditors": rules.auditors.iter().map(|auditor| json!({"authority": auditor.authority.to_string(), "feeRecipient": auditor.fee_recipient.to_string()})).collect::<Vec<_>>(),
        "threshold": rules.threshold, "authorBond": rules.author_bond.to_string(), "challengeTarget": rules.challenge_target.to_string(),
        "auditFee": rules.audit_fee.to_string(), "fundingDuration": rules.funding_duration.to_string(), "auditTimeout": rules.audit_timeout.to_string(),
    })
}

fn instruction(name: &str, label: &str, input: Value, ix: Instruction) -> Value {
    json!({"name": name, "label": label, "input": input, "programId": ix.program_id.to_string(),
        "accounts": ix.accounts.iter().map(|meta| json!({"pubkey": meta.pubkey.to_string(), "isSigner": meta.is_signer, "isWritable": meta.is_writable})).collect::<Vec<_>>(),
        "dataHex": hex(&ix.data)})
}

fn claim(claim: ClaimState) -> Value {
    match claim {
        ClaimState::Unavailable => json!({"kind": "unavailable"}),
        ClaimState::Unclaimed { amount } => {
            json!({"kind": "unclaimed", "amount": amount.to_string()})
        }
        ClaimState::Claimed => json!({"kind": "claimed"}),
    }
}

fn snapshot(name: &str, ctx: &TestContext) -> Value {
    let view = ctx.view();
    let case = view.case();
    let mut accounts = serde_json::Map::new();
    for (name, key) in [
        ("case", ctx.case),
        ("policy", ctx.policy),
        ("subject", ctx.subject),
        ("vault", ctx.vault),
        ("authorReceipt", ctx.author_receipt()),
        ("clock", CLOCK),
    ] {
        accounts.insert(name.to_string(), account(ctx, key));
    }
    let mut expected = json!({
        "phase": match case.phase() { CasePhase::Funding => "funding", CasePhase::Funded => "funded", CasePhase::Finalized => "finalized" },
        "result": case.result().map(|result| match result { CaseResult::Yes => "yes", CaseResult::No => "no", CaseResult::Unknown => "unknown" }),
        "completion": case.completion_method().map(|method| match method { CompletionMethod::Optimistic => "optimistic", CompletionMethod::Audited => "audited", CompletionMethod::TechnicalFailure => "technicalFailure" }),
        "authorClaim": claim(view.author_claim()), "auditFeesPaid": view.audit_fees_paid().map(|amount| amount.to_string()),
        "vaultBalance": view.vault_balance().to_string(), "unpaid": ctx.unpaid().to_string(),
    });
    for (name, actor) in [("alice", Actor::Alice), ("bob", Actor::Bob)] {
        let key = ctx.receipt(actor);
        if let Some(receipt) = ctx.get_account(&key) {
            accounts.insert(format!("{name}Receipt"), account(ctx, key));
            expected[format!("{name}Claim")] = claim(
                view.contributor_claim(
                    pk(ctx.actor(actor)),
                    observed_account(key, &receipt, view.observation().slot),
                )
                .unwrap(),
            );
        }
    }
    expected["contributionQuotes"] = json!(
        [0_u64, 1, 99, u64::MAX]
            .into_iter()
            .map(|requested| {
                match view.quote_contribution(requested) {
                    Ok(amount) => {
                        json!({"requested": requested.to_string(), "amount": amount.to_string()})
                    }
                    Err(error) => {
                        json!({"requested": requested.to_string(), "error": format!("{error:?}")})
                    }
                }
            })
            .collect::<Vec<_>>()
    );
    let mut quotes = Vec::new();
    for actor in [
        Actor::Auditor1,
        Actor::Auditor2,
        Actor::Auditor3,
        Actor::Outsider,
    ] {
        for (answer, reason) in [
            (AuditAnswer::Yes, AuditReason::Decisive),
            (AuditAnswer::No, AuditReason::Decisive),
            (AuditAnswer::Unknown, AuditReason::MissingEvidence),
        ] {
            let mut quote = json!({"auditor": ctx.actor(actor).to_string(), "answer": answer_name(answer), "reason": reason_name(reason)});
            match view.quote_audit(pk(ctx.actor(actor)), answer, reason) {
                Ok(payouts) => quote["payouts"] = json!(payouts.iter().map(|payout| json!({"auditorIndex": payout.auditor_index, "feeRecipient": payout.fee_recipient.to_string(), "amount": payout.amount.to_string()})).collect::<Vec<_>>()),
                Err(error) => quote["error"] = json!(format!("{error:?}")),
            }
            quotes.push(quote);
        }
    }
    expected["auditQuotes"] = json!(quotes);
    json!({"name": name, "observation": observation(ctx), "caseAddress": case_address(ctx.case_address()), "accounts": accounts, "expected": expected})
}

fn answer_name(answer: AuditAnswer) -> &'static str {
    match answer {
        AuditAnswer::Yes => "yes",
        AuditAnswer::No => "no",
        AuditAnswer::Unknown => "unknown",
    }
}

fn reason_name(reason: AuditReason) -> &'static str {
    match reason {
        AuditReason::Decisive => "decisive",
        AuditReason::MissingEvidence => "missingEvidence",
        AuditReason::StaleEvidence => "staleEvidence",
        AuditReason::ConflictingEvidence => "conflictingEvidence",
        AuditReason::UnsupportedEvidence => "unsupportedEvidence",
        AuditReason::Inconclusive => "inconclusive",
    }
}
