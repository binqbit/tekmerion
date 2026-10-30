//! Export deterministic local SBF evidence; no live provider or TEE is implied.
#[cfg(feature = "local-fixtures")]
fn main() {
    use anchor_lang::AccountDeserialize;
    use serde_json::json;
    use tekmerion_chainlink_sdk::confidential::{self as sdk, payload::ReportBody};
    use tekmerion_contract_tests::support::{
        confidential::{ConfidentialContext, agent_config, documents},
        pk,
    };
    let mut c = ConfidentialContext::funded(3);
    let snapshot = |c: &ConfidentialContext| {
        [
            c.base.profile,
            pk(c.base.bank.policy),
            pk(c.base.bank.case),
            c.base.request,
        ]
        .into_iter()
        .map(|key| {
            let a = c.base.bank.account(key);
            json!({"key":key.to_string(),"owner":a.owner.to_string(),"data":hex(&a.data)})
        })
        .collect::<Vec<_>>()
    };
    let prepared = snapshot(&c);
    let claim = c.report_value(0, 1, ReportBody::InputAccepted);
    c.base.bank.ok(&c.report_instructions(&claim), &[]);
    let claimed = snapshot(&c);
    let verdict = c.verdict(0, 1, 0, 0);
    c.base.bank.ok(&c.report_instructions(&verdict), &[]);
    let ready = snapshot(&c);
    c.base.bank.ok(
        &[
            tekmerion_chainlink_sdk::confidential::compute_budget(),
            c.apply(),
        ],
        &[],
    );
    let applied = snapshot(&c);
    let out = json!({"schemaVersion":3,"kind":"synthetic-local-sbf","verificationLevel":"official-pinned-forwarder-sbf-with-synthetic-don-keys",
        "missingArtifacts":["live-confidential-tenant","live-tee-attestation","live-ai-provider"],
        "network":if cfg!(feature="devnet"){"devnet"}else{"mainnet"},"case":c.base.bank.case.to_string(),"profile":c.base.profile.to_string(),"policy":c.base.bank.policy.to_string(),
        "request":c.base.request.to_string(),"agentConfig":agent_config(),
        "agentConfigHash":hex(&c.config.agent_config_hash),"requestDigest":hex(&c.request().digest),
        "claim":hex(&claim.encode().unwrap()),"verdict":hex(&verdict.encode().unwrap()),
        "documents":documents(),
        "snapshots":{"prepared":prepared,"claimed":claimed,"ready":ready,"applied":applied}});
    assert_eq!(
        sdk::ConfidentialRequest::try_deserialize(
            &mut c.base.bank.account(c.base.request).data.as_slice()
        )
        .unwrap()
        .state,
        sdk::ConfidentialState::Applied
    );
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
#[cfg(not(feature = "local-fixtures"))]
fn main() {
    eprintln!("Use --features local-fixtures with disposable local SBF programs");
    std::process::exit(2);
}

#[cfg(feature = "local-fixtures")]
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").unwrap();
    }
    result
}
