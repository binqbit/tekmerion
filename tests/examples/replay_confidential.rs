//! Replay a recorded terminal workflow payload through the real local Forwarder.
#[cfg(feature = "local-fixtures")]
fn main() {
    use sha2::{Digest, Sha256};
    use tekmerion_chainlink_sdk::confidential::{
        ConfidentialState,
        payload::{AuditReport, ReportBody},
    };
    use tekmerion_contract_tests::support::confidential::ConfidentialContext;
    let path = std::env::args().nth(1).expect("terminal report JSON path");
    let file = std::fs::read(path).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&file).unwrap();
    let encoded = value["report"].as_str().unwrap();
    assert!(encoded.len() <= 204 && encoded.len().is_multiple_of(2));
    let bytes: Vec<u8> = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    let p = AuditReport::decode(&bytes).unwrap();
    let document_hex = value["document"].as_str().expect("final audit report");
    assert!(document_hex.len().is_multiple_of(2));
    let document: Vec<u8> = document_hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    let report_hash: [u8; 32] = Sha256::digest(&document).into();
    let committed_hash = match &p.body {
        ReportBody::Verdict { report_hash, .. } | ReportBody::AttemptFailed { report_hash, .. } => {
            *report_hash
        }
        ReportBody::InputAccepted => panic!("Expected terminal report"),
    };
    assert_eq!(report_hash, committed_hash);
    let text = std::str::from_utf8(&document).unwrap();
    assert!(text.starts_with("# Audit report\n"));
    assert!(text.contains("## Assessment"));
    assert!(text.contains("## Sources"));
    assert_eq!(p.prefix.attempt_index, 0);
    let mut c = ConfidentialContext::funded(3);
    c.claim(0, p.prefix.execution_id[0]);
    c.base.bank.ok(&c.report_instructions(&p), &[]);
    assert_eq!(c.request().report_hash, report_hash);
    if p.prefix.stage == 1 {
        c.base.bank.ok(
            &[
                tekmerion_chainlink_sdk::confidential::compute_budget(),
                c.apply(),
            ],
            &[],
        );
        assert_eq!(c.request().state, ConfidentialState::Applied);
        assert_eq!(c.request().report_hash, report_hash);
    }
    println!(
        "Verified recorded V3 payload using pinned Forwarder SBF; stage={}",
        p.prefix.stage
    );
}
#[cfg(not(feature = "local-fixtures"))]
fn main() {
    eprintln!("Use --features local-fixtures");
    std::process::exit(2);
}
