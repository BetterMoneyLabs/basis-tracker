//! Test tool for the tracker e2e driver (e2e/driver.py): writes a store that a pure-credit tracker must refuse to
//! start on (design §3.7).
//!
//!   seed_onchain_confirmation <data_dir> [committed|pending]
//!
//! Opens <data_dir> the way basis_server opens its `[server] data_dir` (`TrackerStateManager::new`), adds one ERG
//! note between two throwaway test keys and records an on-chain total for that edge in the stored confirmation
//! records: `committed` (the default) as if a tracker box had committed the tree on-chain
//! (`reconcile_with_confirmed_digest`), `pending` as if a tracker box update covering the edge had been submitted
//! (`mark_notes_pending`). It then checks that the store fails the pure-credit startup check for exactly that
//! record, and prints one line. Exit status 2 on a usage error, 1 on any other failure. Never run it on a data
//! folder a tracker process is using.

use basis_store::{DenomKey, IouNote, TrackerStateManager};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, kind) = match args.as_slice() {
        [dir] => (dir.as_str(), "committed"),
        [dir, kind] if kind == "committed" || kind == "pending" => (dir.as_str(), kind.as_str()),
        _ => {
            eprintln!("usage: seed_onchain_confirmation <data_dir> [committed|pending]");
            std::process::exit(2);
        }
    };
    match seed(dir, kind) {
        Ok(line) => println!("{}", line),
        Err(e) => {
            eprintln!("seed_onchain_confirmation: {}", e);
            std::process::exit(1);
        }
    }
}

fn seed(dir: &str, kind: &str) -> Result<String, String> {
    // Throwaway test keys and a synthetic tracker NFT id: nothing here is a real key.
    let (issuer_secret, recipient_secret, nft) = ([0x11u8; 32], [0x22u8; 32], [0x42u8; 32]);
    let secp = Secp256k1::new();
    let public = |secret: &[u8; 32]| -> Result<[u8; 33], String> {
        let sk = SecretKey::from_slice(secret).map_err(|e| format!("bad test key: {}", e))?;
        Ok(PublicKey::from_secret_key(&secp, &sk).serialize())
    };
    let (issuer, recipient) = (public(&issuer_secret)?, public(&recipient_secret)?);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("clock: {}", e))?
        .as_millis() as u64;
    // A minute ago, at an ordinary millisecond (500).
    let ts = now - now % 1000 - 60_000 + 500;

    let mut store = TrackerStateManager::new(dir);
    let note = IouNote::create_and_sign(recipient, 7, ts, &issuer_secret, &nft, DenomKey::Erg)
        .map_err(|e| format!("signing the note: {:?}", e))?;
    store.add_note(&issuer, &note, None).map_err(|e| format!("add_note: {:?}", e))?;
    let digest = store.get_state().avl_root_digest;
    let written = match kind {
        "committed" => store.reconcile_with_confirmed_digest(&digest, "seed-onchain-confirmation", 1),
        _ => store.mark_notes_pending(digest, "seed-onchain-confirmation", 1),
    }
    .map_err(|e| format!("recording the {} total: {:?}", kind, e))?;
    if written != 1 {
        return Err(format!("expected to record 1 {} total, recorded {}", kind, written));
    }
    match store.check_pure_credit() {
        Err(refusal) if refusal.onchain_confirmations == 1 && refusal.undecodable_note_records == 0 => Ok(format!(
            "seeded {}: one ERG note (total 7, timestamp {}) with a {} on-chain total; a pure-credit tracker must \
             refuse this store",
            dir, ts, kind
        )),
        Err(refusal) => Err(format!("unexpected pure-credit check result: {}", refusal)),
        Ok(()) => Err("the seeded store passes the pure-credit check".to_string()),
    }
}
