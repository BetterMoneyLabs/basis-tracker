//! Core types for Basis Tracker system

use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;

/// Public key type (Secp256k1 compressed)
pub type PubKey = [u8; 33];

/// Signature type (Secp256k1 Schnorr) - 65 bytes (33 for 'a' component, 32 for 'z' component)
pub type Signature = [u8; 65];

/// Generate the signing message following the Basis protocol specification.
///
/// message = blake2b256(ownerKeyBytes || receiverKeyBytes) || longToByteArray(totalDebt) || longToByteArray(timestamp)
///
/// Where:
/// - key = blake2b256(ownerKeyBytes || receiverKeyBytes) (32 bytes)
/// - totalDebt: 8-byte big-endian total cumulative debt amount
/// - timestamp: 8-byte big-endian payment timestamp (milliseconds since Unix epoch)
///
/// Both the reserve owner (payer) and the tracker sign the **exact same message**.
/// The timestamp enables replay attack prevention during redemption.
///
/// # Arguments
/// * `owner_key` - Reserve owner's public key (33 bytes)
/// * `receiver_key` - Recipient's public key (33 bytes)
/// * `total_debt` - Total cumulative debt amount
/// * `timestamp` - Payment timestamp in milliseconds since Unix epoch
///
/// # Returns
/// * 48 bytes: key (32) || totalDebt (8 BE) || timestamp (8 BE)
pub fn signing_message(
    owner_key: &PubKey,
    receiver_key: &PubKey,
    total_debt: u64,
    timestamp: u64,
) -> Vec<u8> {
    // Compute key = blake2b256(ownerKey || receiverKey)
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(owner_key);
    hasher.update(receiver_key);
    let key_hash = hasher.finalize();

    // Build message: key || totalDebt || timestamp (always 48 bytes)
    let mut message = Vec::with_capacity(48);
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&total_debt.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());

    message
}