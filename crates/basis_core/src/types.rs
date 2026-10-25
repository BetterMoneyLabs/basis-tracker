//! Core types for Basis Tracker system

use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;

/// Public key type (Secp256k1 compressed)
pub type PubKey = [u8; 33];

/// Signature type (Secp256k1 Schnorr) - 65 bytes (33 for 'a' component, 32 for 'z' component)
pub type Signature = [u8; 65];

/// Generate the signing message used when creating off-chain IOU notes.
///
/// message = blake2b256(ownerKeyBytes || receiverKeyBytes) || longToByteArray(totalDebt) || longToByteArray(timestamp) || trackerNftId
///
/// Where:
/// - key = blake2b256(ownerKeyBytes || receiverKeyBytes) (32 bytes)
/// - totalDebt: 8-byte big-endian total cumulative debt amount
/// - timestamp: 8-byte big-endian payment timestamp (milliseconds since Unix epoch)
/// - trackerNftId: 32-byte tracker NFT ID binding the signature to one tracker
///   (and thus one reserve contract deployment and one chain — issue #15 C3)
///
/// # Returns
/// * 80 bytes: key (32) || totalDebt (8 BE) || timestamp (8 BE) || trackerNftId (32)
pub fn signing_message(
    owner_key: &PubKey,
    receiver_key: &PubKey,
    total_debt: u64,
    timestamp: u64,
    tracker_nft_id: &[u8; 32],
) -> Vec<u8> {
    // Compute key = blake2b256(ownerKey || receiverKey)
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(owner_key);
    hasher.update(receiver_key);
    let key_hash = hasher.finalize();

    // Build message: key || totalDebt || timestamp || trackerNftId (always 80 bytes)
    let mut message = Vec::with_capacity(80);
    message.extend_from_slice(&key_hash);
    message.extend_from_slice(&total_debt.to_be_bytes());
    message.extend_from_slice(&timestamp.to_be_bytes());
    message.extend_from_slice(tracker_nft_id);

    message
}
