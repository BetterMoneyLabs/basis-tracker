//! Core types for Basis Tracker system

/// Public key type (Secp256k1 compressed)
pub type PubKey = [u8; 33];

/// Signature type (Secp256k1 Schnorr) - 65 bytes (33 for 'a' component, 32 for 'z' component)
pub type Signature = [u8; 65];

/// Generate the signing message in the same format as chaincash-rs
pub fn signing_message(recipient_pubkey: &PubKey, amount: u64, timestamp: u64) -> Vec<u8> {
    let mut message = Vec::new();
    message.extend_from_slice(recipient_pubkey);
    message.extend_from_slice(&amount.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());
    message
}