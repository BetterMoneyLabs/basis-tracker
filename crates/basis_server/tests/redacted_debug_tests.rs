//! Redacted Debug output (tracker v3, design §3.8: signatures are never logged). The tracker thread debug-logs
//! every command (main.rs) and `create_note` debug-logs its request, so `TrackerCommand` and the note and novation
//! request structs implement Debug by hand: public keys, totals and timestamps are shown, each signature only as
//! its fingerprint `sha256:<8 hex>` (the first 4 bytes of the SHA-256 of its bytes).

use basis_server::{
    redact, CreateNoteRequest, ExchangeAcceptanceCheck, RedeemRequest, SetOffRequest, TrackerCommand, TransferNotesRequest,
};
use basis_store::{DenomKey, IouNote, PriorAck, RedemptionRequest, SetOffEdge, TransferSignatures};
use secp256k1::{Secp256k1, SecretKey};
use serde_json::json;
use sha2::Digest;

const NFT: [u8; 32] = [0x42; 32];
const A: [u8; 32] = [1u8; 32];
const B: [u8; 32] = [2u8; 32];
const C: [u8; 32] = [3u8; 32];
const TS: u64 = 1_762_570_000_999;

fn pk(secret: &[u8; 32]) -> [u8; 33] {
    let secp = Secp256k1::new();
    let sk = SecretKey::from_slice(secret).expect("valid secret key");
    secp256k1::PublicKey::from_secret_key(&secp, &sk).serialize()
}

/// A real Schnorr signature by `signer` over the note message (A -> B, `total`, TS): distinct per `total`.
fn sig(signer: &[u8; 32], total: u64) -> [u8; 65] {
    let msg = basis_store::schnorr::signing_message(&pk(&A), &pk(&B), &DenomKey::Erg, total, TS, &NFT);
    basis_store::schnorr::schnorr_sign(&msg, signer, &pk(signer)).unwrap()
}

/// The fingerprint, computed here independently of `redact`.
fn fp(signature: &[u8]) -> String {
    format!("sha256:{}", hex::encode(&sha2::Sha256::digest(signature)[..4]))
}

/// The longest run of hex digits in `s`.
fn longest_hex_run(s: &str) -> usize {
    s.split(|c: char| !c.is_ascii_hexdigit()).map(str::len).max().unwrap_or(0)
}

/// Whether `s` holds a list of at least 8 integers in a row ("2, 155, 30, ..."), as the derived Debug of a byte
/// array prints one (compact or pretty).
fn has_byte_list(s: &str) -> bool {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut run = 0;
    for part in flat.split(", ") {
        let part = part.trim_matches(|c| c == '[' || c == ']' || c == ' ' || c == ',');
        if !part.is_empty() && part.parse::<u8>().is_ok() {
            run += 1;
            if run >= 8 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// `out` reveals none of `sigs`: no 130-hex signature (in fact no hex run longer than a 66-hex public key), no
/// 16-hex-character piece of any signature in either case, no decimal byte list; and every signature's
/// fingerprint is shown.
fn assert_redacted(out: &str, sigs: &[[u8; 65]]) {
    assert!(longest_hex_run(out) <= 66, "a hex run longer than a public key: {out}");
    assert!(!has_byte_list(out), "a byte list: {out}");
    for s in sigs {
        let lower = hex::encode(s);
        for start in 0..=(lower.len() - 16) {
            let piece = &lower[start..start + 16];
            assert!(
                !out.contains(piece) && !out.contains(&piece.to_uppercase()),
                "a piece of a signature ({piece}): {out}"
            );
        }
        assert!(out.contains(&fp(s)), "the fingerprint {} is missing: {out}", fp(s));
    }
}

/// Both renderings of `value`, compact and pretty.
fn both(value: &impl std::fmt::Debug) -> [String; 2] {
    [format!("{:?}", value), format!("{:#?}", value)]
}

fn transfer_signatures() -> TransferSignatures {
    TransferSignatures {
        issuer_from: sig(&A, 1),
        issuer_to: sig(&A, 2),
        from_recipient_prior: sig(&B, 3),
        from_recipient: sig(&B, 4),
        from_recipient_to: sig(&B, 5),
        to_recipient_prior: sig(&C, 6),
        to_recipient: sig(&C, 7),
    }
}

fn all(s: &TransferSignatures) -> [[u8; 65]; 7] {
    [s.issuer_from, s.issuer_to, s.from_recipient_prior, s.from_recipient, s.from_recipient_to, s.to_recipient_prior, s.to_recipient]
}

#[test]
fn the_detectors_see_a_signature_in_the_derived_debug() {
    // Not vacuous: a derived Debug of signatures prints each as a byte list, and the hex of a signature is a
    // 130-character run. (Until tracker v3.1 the witness was the store's own TransferSignatures, whose derived Debug
    // did exactly this; the store types are redacted now, so a struct with the same derive stands in for it.)
    #[derive(Debug)]
    #[allow(dead_code)]
    struct DerivedDebug {
        issuer_from: [u8; 65],
        issuer_to: [u8; 65],
    }
    let s = transfer_signatures();
    let derived = DerivedDebug { issuer_from: s.issuer_from, issuer_to: s.issuer_to };
    assert!(has_byte_list(&format!("{:?}", derived)) && has_byte_list(&format!("{:#?}", derived)));
    assert_eq!(longest_hex_run(&hex::encode(s.issuer_from)), 130);
}

#[test]
fn the_store_types_with_signatures_are_redacted_on_their_own() {
    // Tracker v3.1: IouNote, PriorAck, TransferSignatures and SetOffEdge implement Debug by hand as well, so a direct
    // {:?} of any of them (or of a type that contains one) never prints a signature, not only the TrackerCommand
    // rendering. Their output is the same as the server's redact wrappers'.
    let token = DenomKey::Token([0xa5; 32]);
    let note = IouNote::create_and_sign(pk(&B), 100, TS - 2, &A, &NFT, token).unwrap();
    for out in both(&note) {
        assert_redacted(&out, &[note.signature]);
        for shown in [hex::encode(pk(&B)), "amount_collected: 100".to_string(), format!("token:{}", hex::encode([0xa5u8; 32]))] {
            assert!(out.contains(&shown), "{shown} is missing: {out}");
        }
    }
    assert_eq!(format!("{:?}", note), format!("{:?}", redact::Note(&note)));

    let prior = PriorAck { signature: sig(&A, 27), prior_total: Some(60) };
    for out in both(&prior) {
        assert_redacted(&out, &[prior.signature]);
    }
    assert_eq!(format!("{:?}", prior), format!("PriorAck {{ signature: {}, prior_total: Some(60) }}", fp(&prior.signature)));
    assert_eq!(format!("{:?}", prior), format!("{:?}", redact::Prior(&prior)));

    let s = transfer_signatures();
    for out in both(&s) {
        assert_redacted(&out, &all(&s));
    }
    assert_eq!(format!("{:?}", s), format!("{:?}", redact::TransferSigs(&s)));

    let edge = SetOffEdge {
        issuer_pubkey: pk(&A),
        recipient_pubkey: pk(&B),
        decrease: 30,
        timestamp: TS - 1,
        issuer_signature: sig(&A, 28),
        recipient_signature: sig(&B, 29),
    };
    let edge_sigs = [edge.issuer_signature, edge.recipient_signature];
    for out in both(&edge) {
        assert_redacted(&out, &edge_sigs);
        assert!(out.contains("decrease: 30") && out.contains("timestamp: 1762570000998"), "{out}");
    }
    assert_eq!(format!("{:?}", edge), format!("{:?}", redact::Edge(&edge)));

    // Types that contain them inherit the redaction: a list of set-off edges, an optional prior, and a staged
    // exchange record (its to-edge note).
    for out in both(&vec![edge.clone(), edge.clone()]) {
        assert_redacted(&out, &edge_sigs);
    }
    for out in both(&Some(prior)) {
        assert_redacted(&out, &[prior.signature]);
    }
    let record = basis_store::PendingExchangeRecord {
        issuer_pubkey: pk(&A),
        recipient_pubkey: pk(&B),
        from_denomination: DenomKey::Erg,
        from_old_total: 100,
        from_new_total: 90,
        to_denomination: token,
        to_note: note.clone(),
        to_base_collected: None,
        reserve_box_id: "bb".repeat(32),
        created_at: TS,
        status: basis_store::PendingExchangeStatus::Staged,
    };
    for out in both(&record) {
        // The record's own derived Debug prints its token denomination as a list of 32 bytes (no secret), so the
        // generic byte-list detector does not apply; no run of 8 of the signature's bytes may appear.
        assert!(longest_hex_run(&out) <= 66, "{out}");
        let flat = out.split_whitespace().collect::<Vec<_>>().join(" ");
        let bytes_shown = note.signature.windows(8).any(|w| {
            flat.contains(&w.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", "))
        });
        assert!(!bytes_shown, "the signature's bytes: {out}");
        let lower = hex::encode(note.signature);
        assert!((0..=lower.len() - 16).all(|i| !out.contains(&lower[i..i + 16])), "a piece of the signature: {out}");
        assert!(out.contains(&fp(&note.signature)), "{out}");
    }
}

#[test]
fn the_redemption_request_is_redacted() {
    // initiate_redemption debug-logs its RedeemRequest; the issuer's signature appears only as its fingerprint.
    let signature = sig(&A, 30);
    let body = json!({
        "issuer_pubkey": hex::encode(pk(&A)),
        "recipient_pubkey": hex::encode(pk(&B)),
        "amount": 100,
        "timestamp": TS,
        "reserve_box_id": "bb".repeat(32),
        "recipient_address": "test-recipient-address-for-redaction",
        "issuer_signature": hex::encode(signature),
        "emergency": true,
        "denomination": "erg",
    });
    let request: RedeemRequest = serde_json::from_value(body.clone()).unwrap();
    for out in both(&request) {
        assert_redacted(&out, &[signature]);
        for shown in [
            hex::encode(pk(&A)),
            hex::encode(pk(&B)),
            "amount: 100".to_string(),
            "timestamp: 1762570000999".to_string(),
            "bb".repeat(32),
            "test-recipient-address-for-redaction".to_string(),
            "emergency: true".to_string(),
            "\"erg\"".to_string(),
        ] {
            assert!(out.contains(&shown), "{shown} is missing: {out}");
        }
    }
    // A signature pasted into a free-text or key field is shown only by its length.
    let mut pasted = body;
    pasted["recipient_address"] = json!(hex::encode(signature));
    pasted["recipient_pubkey"] = json!(hex::encode(signature));
    pasted["issuer_signature"] = json!("zz");
    let out = format!("{:?}", serde_json::from_value::<RedeemRequest>(pasted).unwrap());
    assert!(longest_hex_run(&out) <= 66, "{out}");
    for shown in [
        "recipient_address: <130 chars>",
        "recipient_pubkey: <not 33 bytes of hex, 130 chars>",
        "issuer_signature: <not a 65-byte hex signature, 2 chars>",
    ] {
        assert!(out.contains(shown), "{shown} is missing: {out}");
    }
}

#[test]
fn a_transfer_command_shows_fingerprints_never_signatures() {
    let signatures = transfer_signatures();
    let (response_tx, _rx) = tokio::sync::oneshot::channel();
    let command = TrackerCommand::TransferNotes {
        issuer_pubkey: pk(&A),
        from_recipient_pubkey: pk(&B),
        to_recipient_pubkey: pk(&C),
        denomination: DenomKey::Erg,
        amount: 20,
        timestamp: TS,
        signatures,
        from_prior_total: Some(100),
        to_prior_total: Some(50),
        acceptance_check: Some(ExchangeAcceptanceCheck(Box::new(
            |_view: basis_store::ExchangeAcceptanceView| -> Result<(), String> { Ok(()) },
        ))),
        response_tx,
    };
    for out in both(&command) {
        assert_redacted(&out, &all(&signatures));
        // What the log needs is there: the parties, the denomination, the amount, the timestamp.
        for party in [&A, &B, &C] {
            assert!(out.contains(&hex::encode(pk(party))), "{out}");
        }
        for shown in ["TransferNotes", "\"erg\"", "amount: 20", "timestamp: 1762570000999", "ExchangeAcceptanceCheck(..)"] {
            assert!(out.contains(shown), "{shown} is missing: {out}");
        }
    }
    // The compact form, exactly (the response channel is elided).
    let expected = format!(
        "TransferNotes {{ issuer_pubkey: {}, from_recipient_pubkey: {}, to_recipient_pubkey: {}, denomination: \"erg\", amount: 20, timestamp: 1762570000999, signatures: TransferSignatures {{ issuer_from: {}, issuer_to: {}, from_recipient_prior: {}, from_recipient: {}, from_recipient_to: {}, to_recipient_prior: {}, to_recipient: {} }}, from_prior_total: Some(100), to_prior_total: Some(50), acceptance_check: Some(ExchangeAcceptanceCheck(..)), .. }}",
        hex::encode(pk(&A)),
        hex::encode(pk(&B)),
        hex::encode(pk(&C)),
        fp(&signatures.issuer_from),
        fp(&signatures.issuer_to),
        fp(&signatures.from_recipient_prior),
        fp(&signatures.from_recipient),
        fp(&signatures.from_recipient_to),
        fp(&signatures.to_recipient_prior),
        fp(&signatures.to_recipient),
    );
    assert_eq!(format!("{:?}", command), expected);
}

#[test]
fn every_command_with_a_signature_is_redacted() {
    // A bound note with its issuer prior acknowledgment.
    let note = IouNote::create_and_sign(pk(&B), 100, TS - 2, &A, &NFT, DenomKey::Erg).unwrap();
    let prior = PriorAck { signature: sig(&A, 8), prior_total: Some(60) };
    let (response_tx, _rx) = tokio::sync::oneshot::channel();
    let add = TrackerCommand::AddNote { issuer_pubkey: pk(&A), note: note.clone(), prior: Some(prior), response_tx };
    for out in both(&add) {
        assert_redacted(&out, &[note.signature, prior.signature]);
    }
    let out = format!("{:?}", add);
    assert!(out.contains("amount_collected: 100") && out.contains("prior_total: Some(60)"), "{out}");

    // A set-off with two edges, two signatures each.
    let edges = vec![
        SetOffEdge { issuer_pubkey: pk(&A), recipient_pubkey: pk(&B), decrease: 30, timestamp: TS - 1, issuer_signature: sig(&A, 9), recipient_signature: sig(&B, 10) },
        SetOffEdge { issuer_pubkey: pk(&B), recipient_pubkey: pk(&A), decrease: 30, timestamp: TS - 1001, issuer_signature: sig(&B, 11), recipient_signature: sig(&A, 12) },
    ];
    let edge_sigs: Vec<[u8; 65]> = edges.iter().flat_map(|e| [e.issuer_signature, e.recipient_signature]).collect();
    let (response_tx, _rx) = tokio::sync::oneshot::channel();
    let setoff = TrackerCommand::SetOff { denomination: DenomKey::Token([0xa5; 32]), edges, response_tx };
    for out in both(&setoff) {
        assert_redacted(&out, &edge_sigs);
        assert!(out.contains("decrease: 30") && out.contains(&format!("token:{}", hex::encode([0xa5u8; 32]))), "{out}");
    }

    // An exchange, four signatures.
    let exchange_sigs = [sig(&A, 13), sig(&B, 14), sig(&A, 15), sig(&B, 16)];
    let (response_tx, _rx) = tokio::sync::oneshot::channel();
    let exchange = TrackerCommand::ExchangeNotes {
        issuer_pubkey: pk(&A),
        recipient_pubkey: pk(&B),
        from_denomination: DenomKey::Erg,
        from_amount: 30,
        to_denomination: DenomKey::Token([0xa5; 32]),
        to_amount: 40,
        timestamp: TS,
        issuer_from_signature: exchange_sigs[0],
        recipient_from_signature: exchange_sigs[1],
        issuer_to_signature: exchange_sigs[2],
        recipient_to_signature: exchange_sigs[3],
        from_reserve_box_id: "aa".repeat(32),
        acceptance_check: None,
        response_tx,
    };
    for out in both(&exchange) {
        assert_redacted(&out, &exchange_sigs);
    }

    // A redemption request carries the note's signature and the tracker's, hex-encoded.
    let redemption_sigs = [sig(&A, 17), sig(&C, 18)];
    let request: RedemptionRequest = serde_json::from_value(json!({
        "issuer_pubkey": hex::encode(pk(&A)),
        "recipient_pubkey": hex::encode(pk(&B)),
        "amount": 100,
        "timestamp": TS,
        "reserve_box_id": "bb".repeat(32),
        "tracker_box_id": "cc".repeat(32),
        "tracker_nft_id": hex::encode(NFT),
        "current_height": 1000,
        "recipient_address": "9f",
        "change_address": "9f",
        "issuer_signature": hex::encode(redemption_sigs[0]),
        "tracker_signature": hex::encode(redemption_sigs[1]),
        "reserve_box_value": 1000000,
    }))
    .unwrap();
    let (response_tx, _rx) = tokio::sync::oneshot::channel();
    let redeem = TrackerCommand::InitiateRedemption { request, response_tx };
    for out in both(&redeem) {
        assert_redacted(&out, &redemption_sigs);
    }
}

#[test]
fn note_and_novation_requests_are_redacted() {
    let (signature, prior) = (sig(&A, 19), sig(&A, 20));
    let create: CreateNoteRequest = serde_json::from_value(json!({
        "recipient_pubkey": hex::encode(pk(&B)),
        "amount": 100,
        "timestamp": TS - 2,
        "signature": hex::encode(signature),
        "issuer_pubkey": hex::encode(pk(&A)),
        "tracker_nft_id": hex::encode(NFT),
        "denomination": "erg",
        "prior_signature": hex::encode(prior),
        "prior_total": 60,
    }))
    .unwrap();
    for out in both(&create) {
        assert_redacted(&out, &[signature, prior]);
        assert!(out.contains(&hex::encode(pk(&A))) && out.contains("amount: 100") && out.contains("1762570000997"), "{out}");
    }

    let s = transfer_signatures();
    let transfer: TransferNotesRequest = serde_json::from_value(json!({
        "issuer_pubkey": hex::encode(pk(&A)),
        "from_recipient_pubkey": hex::encode(pk(&B)),
        "to_recipient_pubkey": hex::encode(pk(&C)),
        "denomination": "erg",
        "amount": 20,
        "timestamp": TS,
        "issuer_from_signature": hex::encode(s.issuer_from),
        "issuer_to_signature": hex::encode(s.issuer_to),
        "from_recipient_prior_signature": hex::encode(s.from_recipient_prior),
        "from_recipient_signature": hex::encode(s.from_recipient),
        "from_recipient_to_signature": hex::encode(s.from_recipient_to),
        "to_recipient_prior_signature": hex::encode(s.to_recipient_prior),
        "to_recipient_signature": hex::encode(s.to_recipient),
    }))
    .unwrap();
    for out in both(&transfer) {
        assert_redacted(&out, &all(&s));
    }

    let edge_sigs = [sig(&A, 21), sig(&B, 22), sig(&B, 23), sig(&A, 24)];
    let setoff: SetOffRequest = serde_json::from_value(json!({
        "denomination": "erg",
        "timestamp": TS - 1,
        "edges": [
            {"issuer_pubkey": hex::encode(pk(&A)), "recipient_pubkey": hex::encode(pk(&B)), "decrease": 30,
             "issuer_signature": hex::encode(edge_sigs[0]), "recipient_signature": hex::encode(edge_sigs[1])},
            {"issuer_pubkey": hex::encode(pk(&B)), "recipient_pubkey": hex::encode(pk(&A)), "decrease": 30,
             "timestamp": TS - 1001,
             "issuer_signature": hex::encode(edge_sigs[2]), "recipient_signature": hex::encode(edge_sigs[3])},
        ],
    }))
    .unwrap();
    for out in both(&setoff) {
        assert_redacted(&out, &edge_sigs);
    }
    let out = format!("{:?}", setoff);
    assert!(out.contains("timestamp: Some(1762569999998)") && out.contains("timestamp: None"), "{out}");
}

#[test]
fn malformed_request_values_are_shown_only_by_length() {
    // A signature that is not 65 bytes of hex, and a signature pasted into a key and a denomination field.
    let s = sig(&A, 25);
    let sig_hex = hex::encode(s);
    let create: CreateNoteRequest = serde_json::from_value(json!({
        "recipient_pubkey": sig_hex,
        "amount": 1,
        "timestamp": TS - 2,
        "signature": format!("{}x", sig_hex),
        "issuer_pubkey": hex::encode(pk(&A)),
        "tracker_nft_id": "zz",
        "denomination": sig_hex,
    }))
    .unwrap();
    let out = format!("{:?}", create);
    assert!(longest_hex_run(&out) <= 66, "{out}");
    assert!(!out.contains(&sig_hex[..16]) && !out.contains(&sig_hex[64..80]), "{out}");
    for shown in [
        "recipient_pubkey: <not 33 bytes of hex, 130 chars>",
        "signature: <not a 65-byte hex signature, 131 chars>",
        "tracker_nft_id: <not 32 bytes of hex, 2 chars>",
        "denomination: <not a denomination, 130 chars>",
        "prior_signature: None",
    ] {
        assert!(out.contains(shown), "{shown} is missing: {out}");
    }
}

#[test]
fn a_fingerprint_is_the_first_four_bytes_of_the_sha256() {
    let s = sig(&B, 26);
    let digest = sha2::Sha256::digest(s);
    assert_eq!(redact::fingerprint(&s), hex::encode(&digest[..4]));
    assert_eq!(redact::fingerprint(&s).len(), 8);
    // The same fingerprint from the bytes and from the hex (either case) a request carries, so a request's log line
    // and its command's log line can be matched.
    assert_eq!(format!("{:?}", redact::Sig(&s)), fp(&s));
    assert_eq!(format!("{:?}", redact::SigHex(&hex::encode(s))), fp(&s));
    assert_eq!(format!("{:?}", redact::SigHex(&hex::encode_upper(s))), fp(&s));
}
