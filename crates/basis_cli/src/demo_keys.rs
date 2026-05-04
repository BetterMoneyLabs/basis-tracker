//! Demo participant keys for simple demo.
//!
//! These keys are loaded from `secrets/participants.csv` for demonstration
//! purposes to enable a simple Alice → Bob → Redeem flow matching the Scala demo.
//!
//! # Security Warning
//!
//! These are **TEST KEYS ONLY** - never use in production!
//! The private keys are loaded from a local file and should never be committed.

use secp256k1::{SecretKey, PublicKey};
use crate::crypto::KeyPair;
use std::fs;
use std::path::Path;

/// Demo participant with known keys
pub struct DemoParticipant {
    pub name: &'static str,
    pub keypair: KeyPair,
    pub address: String,
    /// Optional pubkey override (for participants where we only know the pubkey, not the secret)
    pubkey_override: Option<PublicKey>,
}

impl DemoParticipant {
    /// Get public key as compressed secp256k1 bytes (33 bytes)
    pub fn public_key(&self) -> PublicKey {
        if let Some(pubkey) = self.pubkey_override {
            pubkey
        } else {
            self.keypair.keypair.public_key()
        }
    }

    /// Get public key as hex string (66 chars)
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key().serialize())
    }

    /// Get secret key bytes
    pub fn secret_key_bytes(&self) -> [u8; 32] {
        self.keypair.keypair.secret_bytes()
    }
}

/// Load participant secret from CSV file
fn load_secret_from_csv(name: &str) -> Option<String> {
    // Try multiple possible paths for the secrets file
    let possible_paths = [
        Path::new("secrets/participants.csv"),
        Path::new("../../secrets/participants.csv"), // From crates/basis_cli/src/
        Path::new("../secrets/participants.csv"),    // From crates/basis_cli/
        Path::new("../../../secrets/participants.csv"), // From deeper test paths
    ];
    
    let contents = possible_paths
        .iter()
        .filter(|p| p.exists())
        .find_map(|p| fs::read_to_string(p).ok())?;
    
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 3 && parts[0].trim() == name {
            return Some(parts[2].trim().to_string());
        }
    }
    
    None
}

/// Create a demo participant from a secret hex string
fn participant_from_secret(name: &'static str, secret_hex: &str, address: &str) -> DemoParticipant {
    let secret = SecretKey::from_slice(&hex::decode(secret_hex).unwrap()).unwrap();
    let keypair = KeyPair::from_private_key(secret).unwrap();
    
    DemoParticipant {
        name,
        keypair,
        address: address.to_string(),
        pubkey_override: None,
    }
}

/// Alice - reserve owner and IOU issuer
/// 
/// Public key: 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83
pub fn alice() -> DemoParticipant {
    let secret_hex = load_secret_from_csv("alice")
        .expect("Alice secret not found in secrets/participants.csv");
    participant_from_secret("alice", &secret_hex, "9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ")
}

/// Bob - payee and IOU recipient
/// 
/// Public key: 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea
/// 
/// Note: Bob's pubkey is derived from his Ergo address (like Scala does),
/// not from a secret key. We use the known pubkey directly since we only
/// need it for message construction (Bob never signs in the demo flow).
pub fn bob() -> DemoParticipant {
    // Bob's pubkey from Scala demo note.json
    let bob_pubkey_hex = "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea";
    let bob_pubkey_bytes = hex::decode(bob_pubkey_hex).unwrap();
    let bob_pubkey = secp256k1::PublicKey::from_slice(&bob_pubkey_bytes).unwrap();
    
    // Create participant with a dummy keypair (Bob doesn't need to sign)
    // but override the public_key method to return the correct pubkey
    let secp = secp256k1::Secp256k1::new();
    let dummy_secret = secp256k1::SecretKey::new(&mut rand::thread_rng());
    let keypair = KeyPair::from_private_key(dummy_secret).unwrap();
    
    DemoParticipant {
        name: "bob",
        keypair,
        address: "9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73".to_string(),
        pubkey_override: Some(bob_pubkey),
    }
}

/// Tracker - off-chain debt witness
pub fn tracker() -> DemoParticipant {
    let secret_hex = load_secret_from_csv("tracker")
        .expect("Tracker secret not found in secrets/participants.csv");
    participant_from_secret("tracker", &secret_hex, "9f7ZXamnfaDZL7EWLKLuBZgWMuHCusQYK6yow2d7p2eES9oRRRe")
}

/// Print demo participant information
pub fn print_demo_keys() {
    println!("=== Demo Participant Keys ===\n");
    
    let participants = [alice(), bob(), tracker()];
    
    for p in &participants {
        println!("{}:", p.name.to_uppercase());
        println!("  Address:     {}", p.address);
        println!("  Public Key:  {}", p.public_key_hex());
        println!("  Secret Key:  {}", hex::encode(p.secret_key_bytes()));
        println!();
    }
    
    println!("⚠️  WARNING: These are TEST KEYS - never use in production!");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_demo_keys_have_valid_keys() {
        let alice = alice();
        let bob = bob();
        let tracker = tracker();
        
        // Verify public keys are 33 bytes (compressed format)
        assert_eq!(alice.public_key().serialize().len(), 33);
        assert_eq!(bob.public_key().serialize().len(), 33);
        assert_eq!(tracker.public_key().serialize().len(), 33);
        
        // Verify public keys start with 0x02 or 0x03 (compressed format)
        assert!(alice.public_key().serialize()[0] == 0x02 || alice.public_key().serialize()[0] == 0x03);
        assert!(bob.public_key().serialize()[0] == 0x02 || bob.public_key().serialize()[0] == 0x03);
        assert!(tracker.public_key().serialize()[0] == 0x02 || tracker.public_key().serialize()[0] == 0x03);
    }
    
    #[test]
    fn test_demo_keypair_can_sign() {
        let alice = alice();
        let message = b"test message";
        
        let signature = alice.keypair.sign_message(message).unwrap();
        assert_eq!(signature.len(), 65);
        
        let pubkey = alice.public_key();
        let verified = KeyPair::verify_signature(message, &signature, &pubkey.serialize()).unwrap();
        assert!(verified);
    }
    
    #[test]
    fn test_demo_keys_are_distinct() {
        let alice = alice();
        let bob = bob();
        let tracker = tracker();
        
        // All participants should have different public keys
        assert_ne!(alice.public_key().serialize(), bob.public_key().serialize());
        assert_ne!(alice.public_key().serialize(), tracker.public_key().serialize());
        assert_ne!(bob.public_key().serialize(), tracker.public_key().serialize());
    }
    
    #[test]
    fn test_demo_keys_match_scala_demo() {
        // Verify loaded secrets derive to expected Scala demo pubkeys
        let alice = alice();
        let bob = bob();
        let tracker = tracker();
        
        assert_eq!(
            alice.public_key_hex(),
            "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
            "Alice pubkey must match Scala demo"
        );
        
        // Tracker pubkey from tracker_box_setup.json R4 register
        // R4: 07024e564477ff457c601c01ad1cc31903f8b27b7d5e515bd03138891d8152d787b2
        // 07 = GroupElement, 02 = compressed, rest = 32-byte x-coordinate
        assert_eq!(
            tracker.public_key_hex(),
            "024e564477ff457c601c01ad1cc31903f8b27b7d5e515bd03138891d8152d787b2",
            "Tracker pubkey must match Scala demo tracker_box_setup.json R4"
        );
        
        assert_eq!(
            bob.public_key_hex(),
            "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
            "Bob pubkey must match Scala demo"
        );
    }
}
