//! Disposable, explicitly synthetic DON fixtures with genuine Forwarder SBF.
use super::{
    Actor,
    chainlink::{ForwarderContext, forwarder, serialize, sha},
    pk,
};
use anchor_lang::AccountDeserialize;
use solana_instruction::Instruction;
use tekmerion_chainlink_sdk::confidential::{
    self as sdk, ConfidentialConfig, ConfidentialProfile, ConfidentialRequest,
    payload::{AuditReport, ReportBody, ReportPrefix},
};

pub struct ConfidentialContext {
    pub base: ForwarderContext,
    pub config: ConfidentialConfig,
}

pub fn agent_config() -> serde_json::Value {
    serde_json::from_str(include_str!("../fixtures/confidential/agent-config.json")).unwrap()
}

pub fn agent_config_hash() -> [u8; 32] {
    // serde_json's sorted map keys match the workflow canonical JSON encoder.
    sha(serde_json::to_string(&agent_config()).unwrap().as_bytes())
}

impl ConfidentialContext {
    pub fn configured(limit: u16) -> Self {
        let mut base = ForwarderContext::configured();
        let mut rules = base.bank.policy_state().rules;
        rules.audit_timeout = 600;
        base.bank
            .ok(&[base.bank.update_policy_ix(rules)], &[Actor::Admin]);
        let config = ConfidentialConfig {
            forwarder_program: forwarder::FORWARDER,
            forwarder_state: base.state,
            workflow_id: [0x11; 32],
            agent_config_hash: agent_config_hash(),
            max_attempts: limit,
        };
        Self { base, config }
    }
    pub fn enrolled(limit: u16) -> Self {
        let mut ctx = Self::configured(limit);
        let policy = pk(ctx.base.bank.policy);
        let admin = pk(ctx.base.bank.actor(Actor::Admin));
        let payer = pk(ctx.base.bank.actor(Actor::Payer));
        ctx.base.bank.ok(
            &[sdk::register_profile(
                payer,
                admin,
                policy,
                ctx.config.clone(),
            )],
            &[Actor::Admin],
        );
        ctx
    }
    pub fn funded(limit: u16) -> Self {
        let mut ctx = Self::enrolled(limit);
        let documents = documents();
        let create = tekmerion_sdk::instructions::create_case(
            ctx.base.bank.case_address(),
            tekmerion_sdk::instructions::CreateCase {
                payer: pk(ctx.base.bank.actor(Actor::Payer)),
                client_authority: pk(ctx.base.bank.authority),
                author: pk(ctx.base.bank.actor(Actor::Author)),
                source: pk(ctx.base.bank.token(Actor::Author)),
                funding: tekmerion::CaseFunding::DirectAudit {
                    expected_fee: ctx.base.bank.policy_state().rules.audit_fee,
                },
                agreement_hash: sha(documents["agreement"].as_str().unwrap().as_bytes()),
                result_hash: sha(documents["result"].as_str().unwrap().as_bytes()),
            },
        );
        ctx.base
            .bank
            .ok(&[create], &[Actor::ClientAuthority, Actor::Author]);
        ctx.base.bank.ok(
            &[sdk::prepare_request(
                pk(ctx.base.bank.actor(Actor::Payer)),
                ctx.base.bank.case_address(),
            )],
            &[],
        );
        ctx
    }
    pub fn profile(&self) -> ConfidentialProfile {
        ConfidentialProfile::try_deserialize(
            &mut self.base.bank.account(self.base.profile).data.as_slice(),
        )
        .unwrap()
    }
    pub fn request(&self) -> ConfidentialRequest {
        ConfidentialRequest::try_deserialize(
            &mut self.base.bank.account(self.base.request).data.as_slice(),
        )
        .unwrap()
    }
    pub fn metadata(&self, stage: u8) -> Vec<u8> {
        [
            self.config.workflow_id.as_slice(),
            &[0x33; 10],
            &[0x22; 20],
            sdk::payload::report_id(stage).unwrap().as_slice(),
        ]
        .concat()
    }
    pub fn report_value(&self, index: u16, execution: u8, body: ReportBody) -> AuditReport {
        let request = self.request();
        let stage = match body {
            ReportBody::InputAccepted => 0,
            ReportBody::Verdict { .. } => 1,
            ReportBody::AttemptFailed { .. } => 2,
        };
        AuditReport {
            prefix: ReportPrefix {
                version: 3,
                stage,
                request_digest: request.digest,
                attempt_index: index,
                execution_id: [execution; 32],
            },
            body,
        }
    }
    pub fn report(&self, payload: &AuditReport) -> Instruction {
        self.raw_report(
            payload,
            &payload.encode().unwrap(),
            payload.prefix.execution_id[0],
        )
    }
    pub fn report_instructions(&self, payload: &AuditReport) -> Vec<Instruction> {
        vec![sdk::compute_budget(), self.report(payload)]
    }
    pub fn raw_report(&self, payload: &AuditReport, bytes: &[u8], execution: u8) -> Instruction {
        let config = &self.config;
        let metadata = self.metadata(payload.prefix.stage);
        let receiver = sdk::receiver_accounts(self.base.bank.case_address(), &self.config);
        let mut raw = vec![1];
        raw.extend([execution; 32]);
        raw.extend((self.base.bank.clock().unix_timestamp as u32).to_be_bytes());
        raw.extend(forwarder::DON.to_be_bytes());
        raw.extend(forwarder::CONFIG_VERSION.to_be_bytes());
        raw.extend(metadata);
        let keys: Vec<_> = receiver.iter().flat_map(|m| m.pubkey.to_bytes()).collect();
        raw.extend(sha(&keys));
        raw.extend(serialize(&bytes.to_vec()));
        forwarder::signed_report_with_accounts(
            config,
            receiver,
            pk(self.base.bank.actor(Actor::Payer)),
            self.base.oracles,
            raw,
            &[0, 1],
        )
    }
    pub fn claim(&mut self, index: u16, execution: u8) {
        let p = self.report_value(index, execution, ReportBody::InputAccepted);
        self.base.bank.ok(&self.report_instructions(&p), &[]);
    }
    pub fn verdict(&self, index: u16, execution: u8, answer: u8, reason: u8) -> AuditReport {
        self.report_value(
            index,
            execution,
            ReportBody::Verdict {
                report_hash: [0x66; 32],
                answer,
                reason,
            },
        )
    }
    pub fn apply(&self) -> Instruction {
        sdk::apply_verdict(
            self.base.bank.case_address(),
            pk(self.base.bank.token(Actor::FeeRecipient)),
        )
    }
}

pub fn documents() -> serde_json::Value {
    serde_json::from_str(include_str!("../fixtures/confidential/documents.json")).unwrap()
}
