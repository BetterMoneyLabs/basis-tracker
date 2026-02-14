//! Transaction building for Basis redemption
//!
//! This module provides the foundation for building redemption transactions that interact with
//! the Basis reserve contract on the Ergo blockchain. The transaction builder prepares all
//! necessary components for redemption including:
//!
//! - Reserve box spending (input)
//! - Tracker box as data input (for AVL proof verification)
//! - Updated reserve box (output)
//! - Redemption output box (funds sent to recipient)
//! - Context extension with contract parameters
//! - Schnorr signatures (issuer and tracker)
//! - AVL tree proofs for debt verification
//!
//! When blockchain integration is complete, this will use ergo-lib to build actual transactions
//! that can be submitted to the Ergo network.

use thiserror::Error;


/// Public key type (Secp256k1)
pub type PubKey = [u8; 33];

#[derive(Error, Debug)]
pub enum TransactionBuilderError {
    #[error("Transaction building error: {0}")]
    TransactionBuilding(String),
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
}

/// Context for transaction building containing blockchain and fee parameters
///
/// This structure holds all the contextual information needed to build a valid
/// redemption transaction that can be accepted by the Ergo network.
#[derive(Debug, Clone)]
pub struct TxContext {
    /// Current blockchain height (required for transaction validity)
    pub current_height: u32,
    /// Transaction fee in nanoERG (0.001 ERG = 1,000,000 nanoERG)
    pub fee: u64,
    /// Change address for any leftover funds after redemption
    pub change_address: String,
    /// Network prefix (0 for mainnet, 16 for testnet)
    pub network_prefix: u8,
}

impl Default for TxContext {
    fn default() -> Self {
        Self {
            current_height: 0,
            fee: 1000000, // 0.001 ERG
            change_address: "".to_string(),
            network_prefix: 0, // mainnet
        }
    }
}

/// Complete redemption transaction data structure
///
/// This structure contains all the components needed to build a redemption transaction
/// that follows the Basis contract specification. The transaction structure is:
///
/// - Inputs: [Reserve box] (spent)
/// - Data Inputs: [Tracker box] (for AVL proof verification)
/// - Outputs: [Updated reserve box, Redemption output box, Change box (optional)]
/// - Context Extension: Contract parameters (action, signatures, proofs, amounts)
#[derive(Debug, Clone)]
pub struct RedemptionTransactionData {
    /// Reserve box ID being spent (contains collateral backing the debt)
    pub reserve_box_id: String,
    /// Tracker box ID used as data input (contains AVL tree commitment)
    pub tracker_box_id: String,
    /// Amount being redeemed from the reserve (debt amount)
    pub redemption_amount: u64,
    /// Recipient address where redeemed funds are sent
    pub recipient_address: String,
    /// AVL proof bytes proving the debt exists in the tracker's state
    pub avl_proof: Vec<u8>,
    /// Issuer's 65-byte Schnorr signature authorizing the redemption
    pub issuer_signature: Vec<u8>,
    /// Tracker's 65-byte Schnorr signature validating the debt
    pub tracker_signature: Vec<u8>,
    /// Transaction fee in nanoERG
    pub fee: u64,
    /// Tracker NFT ID from R6 register (hex-encoded serialized SColl(SByte) format following byte_array_register_serialization.md spec)
    pub tracker_nft_id: String,
}

/// Builder for redemption transactions following the Basis contract specification
///
/// This builder assembles all components needed for a redemption transaction:
/// - Validates redemption parameters (sufficient funds, time locks)
/// - Prepares transaction structure with proper inputs/outputs
/// - Ensures Schnorr signature compatibility (65-byte format)
/// - Estimates transaction size for fee calculation
pub struct RedemptionTransactionBuilder;

impl RedemptionTransactionBuilder {

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_context() {
        let context = TxContext {
            current_height: 1000,
            fee: 2000000, // 0.002 ERG
            change_address: "test_change_address".to_string(),
            network_prefix: 16, // testnet
        };

        assert_eq!(context.current_height, 1000);
        assert_eq!(context.fee, 2000000);
        assert_eq!(context.network_prefix, 16);

        let default_context = TxContext::default();
        assert_eq!(default_context.fee, 1000000);
        assert_eq!(default_context.network_prefix, 0);
    }
}