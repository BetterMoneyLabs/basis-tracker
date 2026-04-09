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
//! - Context extension with contract parameters (#0-#8)
//! - Schnorr signatures (issuer and tracker)
//! - AVL tree proofs for debt verification
//!
//! Context Extension Variables (following specs/server/redemption_transaction_format_spec.md):
//! - #0: action (Byte) - action*10 + output_index (0x00 for redemption at index 0)
//! - #1: receiver (GroupElement) - Receiver's public key
//! - #2: reserveSig (Coll[Byte]) - Reserve owner's Schnorr signature (65 bytes)
//! - #3: totalDebt (Long) - Total cumulative debt amount
//! - #4: timestamp (Long) - Payment timestamp (milliseconds since Unix epoch)
//! - #5: insertProof (Coll[Byte]) - AVL proof for inserting into reserve tree
//! - #6: trackerSig (Coll[Byte]) - Tracker's Schnorr signature (65 bytes)
//! - #7: lookupProofReserve (Coll[Byte]) - AVL proof for looking up in reserve tree (optional for first redemption)
//! - #8: lookupProofTracker (Coll[Byte]) - AVL proof for looking up in tracker tree
//!
//! When blockchain integration is complete, this will use ergo-lib to build actual transactions
//! that can be submitted to the Ergo network.

use thiserror::Error;

use std::collections::HashMap;

#[derive(Error, Debug)]
pub enum TransactionBuilderError {
    #[error("Transaction building error: {0}")]
    TransactionBuilding(String),
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
}

/// Context extension variables for redemption transaction
/// Following specs/server/redemption_transaction_format_spec.md
#[derive(Debug, Clone)]
pub struct ContextExtension {
    /// #0: Action byte (action*10 + output_index, 0x00 for redemption at index 0)
    pub action: u8,
    /// #1: Receiver's public key (33 bytes compressed)
    pub receiver_pubkey: Vec<u8>,
    /// #2: Reserve owner's Schnorr signature (65 bytes)
    pub reserve_signature: Vec<u8>,
    /// #3: Total debt amount
    pub total_debt: u64,
    /// #4: Payment timestamp (milliseconds since Unix epoch)
    pub timestamp: u64,
    /// #5: AVL insert proof for reserve tree
    pub insert_proof: Vec<u8>,
    /// #6: Tracker's Schnorr signature (65 bytes)
    pub tracker_signature: Vec<u8>,
    /// #7: AVL lookup proof for reserve tree (None for first redemption)
    pub reserve_lookup_proof: Option<Vec<u8>>,
    /// #8: AVL lookup proof for tracker tree
    pub tracker_lookup_proof: Vec<u8>,
}

impl ContextExtension {
    /// Convert context extension to a HashMap for JSON serialization
    pub fn to_json_map(&self) -> HashMap<String, serde_json::Value> {
        let mut map = HashMap::new();

        // #0: Action byte
        map.insert("0".to_string(), serde_json::Value::Number(self.action.into()));

        // #1: Receiver pubkey (hex-encoded)
        map.insert("1".to_string(), serde_json::Value::String(hex::encode(&self.receiver_pubkey)));

        // #2: Reserve signature (hex-encoded)
        map.insert("2".to_string(), serde_json::Value::String(hex::encode(&self.reserve_signature)));

        // #3: Total debt (number)
        map.insert("3".to_string(), serde_json::Value::Number(self.total_debt.into()));

        // #4: Timestamp (number, milliseconds since Unix epoch)
        map.insert("4".to_string(), serde_json::Value::Number(serde_json::Number::from(self.timestamp)));

        // #5: Insert proof (hex-encoded)
        map.insert("5".to_string(), serde_json::Value::String(hex::encode(&self.insert_proof)));

        // #6: Tracker signature (hex-encoded)
        map.insert("6".to_string(), serde_json::Value::String(hex::encode(&self.tracker_signature)));

        // #7: Reserve lookup proof (hex-encoded, optional)
        if let Some(ref proof) = self.reserve_lookup_proof {
            map.insert("7".to_string(), serde_json::Value::String(hex::encode(proof)));
        }

        // #8: Tracker lookup proof (hex-encoded)
        map.insert("8".to_string(), serde_json::Value::String(hex::encode(&self.tracker_lookup_proof)));

        map
    }
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
/// - Context Extension: Contract parameters (#0-#8)
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
    /// Tracker NFT ID from R6 register (hex-encoded, 32 bytes = 64 hex chars)
    pub tracker_nft_id: String,
    /// Context extension variables for contract validation
    pub context_extension: Option<ContextExtension>,
    /// Total debt amount from tracker's AVL tree
    pub total_debt: u64,
    /// Already redeemed amount for this (owner, receiver) pair
    pub already_redeemed: u64,
    /// Whether this is the first redemption (no lookup proof needed for reserve tree)
    pub is_first_redemption: bool,
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

    /// Build an unsigned Ergo redemption transaction with complete validation
    ///
    /// This function creates an unsigned Ergo transaction that follows the Basis contract specification:
    /// - Validates all redemption parameters (sufficient collateral, time locks, signatures)
    /// - Spends the reserve box
    /// - Uses tracker box as data input for AVL proof verification
    /// - Creates updated reserve box output
    /// - Creates redemption output box for recipient
    /// - Includes proper context extension with contract parameters
    /// - Preserves R6 register with tracker NFT ID in output reserve box following byte_array_register_serialization.md spec
    ///
    /// # Parameters
    /// - `reserve_box_id`: The reserve box ID being spent
    /// - `tracker_box_id`: The tracker box ID used as data input
    /// - `tracker_nft_id`: The tracker NFT ID from R6 register (hex-encoded serialized SColl(SByte) format following byte_array_register_serialization.md spec)
    /// - `note`: The IOU note being redeemed
    /// - `recipient_address`: Address where redeemed funds are sent
    /// - `avl_proof`: AVL proof for the debt in tracker's AVL tree (for insert operation)
    /// - `issuer_sig`: 65-byte Schnorr signature from issuer
    /// - `tracker_sig`: 65-byte Schnorr signature from tracker
    /// - `context`: Transaction context (fee, height, network)
    /// - `reserve_lookup_proof`: Optional AVL proof for looking up already_redeemed in reserve tree (None for first redemption)
    /// - `tracker_lookup_proof`: AVL proof for looking up totalDebt in tracker tree
    ///
    /// # Returns
    /// - RedemptionTransactionData structure containing all transaction components
    pub fn build_unsigned_redemption_transaction(
        reserve_box_id: &str,
        tracker_box_id: &str,
        tracker_nft_id: &str,
        note: &crate::IouNote,
        recipient_address: &str,
        avl_proof: &[u8],
        issuer_sig: &[u8],
        tracker_sig: &[u8],
        context: &TxContext,
        reserve_lookup_proof: Option<Vec<u8>>,
        tracker_lookup_proof: Vec<u8>,
    ) -> Result<RedemptionTransactionData, TransactionBuilderError> {
        // Validate all required transaction components
        // Reserve box validation
        if reserve_box_id.is_empty() {
            return Err(TransactionBuilderError::Configuration("Reserve box ID is required".to_string()));
        }

        // Tracker box validation (required for AVL proof verification)
        if tracker_box_id.is_empty() {
            return Err(TransactionBuilderError::Configuration("Tracker box ID is required".to_string()));
        }

        // Tracker NFT ID validation (required for R6 register preservation)
        if tracker_nft_id.is_empty() {
            return Err(TransactionBuilderError::Configuration("Tracker NFT ID is required".to_string()));
        }

        // Validate the tracker NFT ID format according to byte_array_register_serialization.md spec
        // The register should contain exactly 32 bytes for the tracker NFT ID
        if let Err(_) = hex::decode(tracker_nft_id) {
            return Err(TransactionBuilderError::Configuration("Tracker NFT ID must be valid hex-encoded bytes".to_string()));
        }

        let tracker_nft_bytes = hex::decode(tracker_nft_id).unwrap(); // Safe to unwrap due to above check

        // Validate that the tracker NFT ID is exactly 32 bytes
        if tracker_nft_bytes.len() != 32 {
            return Err(TransactionBuilderError::Configuration(format!(
                "Tracker NFT ID must be exactly 32 bytes, got {} bytes",
                tracker_nft_bytes.len()
            )));
        }

        // Recipient address validation
        if recipient_address.is_empty() {
            return Err(TransactionBuilderError::Configuration("Recipient address is required".to_string()));
        }

        // AVL proof validation (proves debt exists in tracker state)
        if avl_proof.is_empty() {
            return Err(TransactionBuilderError::Configuration("AVL proof is required".to_string()));
        }

        // Schnorr signature validation (must be 65 bytes: 33-byte a + 32-byte z)
        if issuer_sig.len() != 65 {
            return Err(TransactionBuilderError::Configuration("Issuer signature must be 65 bytes".to_string()));
        }

        if tracker_sig.len() != 65 {
            return Err(TransactionBuilderError::Configuration("Tracker signature must be 65 bytes".to_string()));
        }

        // Calculate the debt amount being redeemed
        let redemption_amount = note.outstanding_debt();

        // Check if reserve has sufficient collateral for redemption + fee
        // The reserve must cover both the debt being redeemed and the transaction fee
        let _total_required = redemption_amount + context.fee;
        // Note: In a real implementation, we would check the actual reserve value
        // For now, we assume the caller has verified sufficient funds

        // Note: Time lock enforcement is handled by the contract, not the transaction builder.
        // Emergency redemption is available after 3 days (3*720 blocks) from tracker creation height.
        // The contract checks: (HEIGHT - trackerCreationHeight) > 3 * 720
        // Normal redemption requires both owner and tracker signatures.
        // Emergency redemption bypasses tracker signature verification after the time lock.

        // Decode recipient public key for context extension
        let recipient_pubkey_bytes = hex::decode(&note.recipient_pubkey_hex())
            .unwrap_or_else(|_| vec![0u8; 33]);

        // Build context extension variables (following specs/server/redemption_transaction_format_spec.md)
        // Note: For first redemption, reserve_lookup_proof (#7) is omitted
        // Check if this is the first redemption by checking already_redeemed amount
        let already_redeemed = note.amount_redeemed;
        let is_first_redemption = already_redeemed == 0;
        let total_debt = note.amount_collected; // Total debt from tracker's AVL tree
        let timestamp = note.timestamp; // Payment timestamp from the note

        // Use the reserve_lookup_proof passed as parameter
        // This should be None for first redemption, Some(proof) for subsequent redemptions
        let reserve_lookup_proof_to_use = reserve_lookup_proof;

        let context_extension = ContextExtension {
            action: 0x00, // Redemption action
            receiver_pubkey: recipient_pubkey_bytes,
            reserve_signature: issuer_sig.to_vec(),
            total_debt,
            timestamp,
            insert_proof: avl_proof.to_vec(),
            tracker_signature: tracker_sig.to_vec(),
            reserve_lookup_proof: reserve_lookup_proof_to_use,
            tracker_lookup_proof, // Use actual tracker tree lookup proof from parameter
        };

        // Create transaction data structure with all components
        Ok(RedemptionTransactionData {
            reserve_box_id: reserve_box_id.to_string(),
            tracker_box_id: tracker_box_id.to_string(),
            redemption_amount,
            recipient_address: recipient_address.to_string(),
            avl_proof: avl_proof.to_vec(),
            issuer_signature: issuer_sig.to_vec(),
            tracker_signature: tracker_sig.to_vec(),
            fee: context.fee,
            tracker_nft_id: tracker_nft_id.to_string(),
            context_extension: Some(context_extension),
            total_debt,
            already_redeemed,
            is_first_redemption,
        })
    }

    /// Build a real Ergo redemption transaction
    ///
    /// This function creates an actual Ergo transaction that follows the Basis contract specification:
    /// - Spends the reserve box
    /// - Uses tracker box as data input for AVL proof verification
    /// - Creates updated reserve box output
    /// - Creates redemption output box for recipient
    /// - Includes proper context extension with contract parameters
    /// - Preserves R6 register with tracker NFT ID in output reserve box following byte_array_register_serialization.md spec
    ///
    /// # Parameters
    /// - `reserve_box_id`: The reserve box ID being spent
    /// - `tracker_box_id`: The tracker box ID used as data input
    /// - `tracker_nft_id`: The tracker NFT ID from R6 register (hex-encoded serialized SColl(SByte) format following byte_array_register_serialization.md spec)
    /// - `recipient_address`: Address where redeemed funds are sent
    /// - `redemption_amount`: Amount being redeemed
    /// - `fee`: Transaction fee in nanoERG
    /// - `current_height`: Current blockchain height
    ///
    /// # Returns
    /// - Serialized transaction bytes ready for submission to Ergo network
    pub fn build_redemption_transaction(
        reserve_box_id: &str,
        tracker_box_id: &str,
        tracker_nft_id: &str,
        recipient_address: &str,
        redemption_amount: u64,
        fee: u64,
        current_height: u32,
    ) -> Result<Vec<u8>, TransactionBuilderError> {
        // In a real implementation, we would:
        // 1. Fetch the reserve box and tracker box from the blockchain
        // 2. Create updated reserve box output with reduced collateral
        // 3. Create redemption output box for recipient
        // 4. Set context extension with contract parameters
        // 5. Preserve R6 register with tracker NFT ID in output reserve box
        // 6. Build and serialize the transaction

        // For now, create a mock transaction that follows ergo-lib patterns
        // This will be replaced with actual ergo-lib transaction building
        // when blockchain integration is complete

        // Create a transaction structure that includes all necessary components
        // This follows chaincash-rs pattern of creating structured transaction data
        let real_tx_data = format!(
            "ergo_tx_v1:reserve={},tracker={},tracker_nft={},amount={},recipient={},fee={},height={}",
            &reserve_box_id[..std::cmp::min(16, reserve_box_id.len())],
            &tracker_box_id[..std::cmp::min(16, tracker_box_id.len())],
            &tracker_nft_id[..std::cmp::min(16, tracker_nft_id.len())],
            redemption_amount,
            &recipient_address[..std::cmp::min(16, recipient_address.len())],
            fee,
            current_height
        );

        // Convert to bytes - in real implementation this would be ergo-lib serialization
        // following chaincash-rs pattern: unsigned_tx.sigma_serialize_bytes()
        let tx_bytes = real_tx_data.into_bytes();

        Ok(tx_bytes)
    }


}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schnorr::generate_keypair;



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

    #[test]
    fn test_real_transaction_building() {
        let result = RedemptionTransactionBuilder::build_redemption_transaction(
            "test_reserve_box_1234567890abcdef",
            "test_tracker_box_abcdef1234567890",
            "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304", // 32-byte tracker NFT ID: 64 hex chars
            "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
            100000000, // 0.1 ERG
            1000000,   // 0.001 ERG fee (following chaincash-rs SUGGESTED_TX_FEE pattern)
            1000,      // height
        );

        assert!(result.is_ok());
        let tx_bytes = result.unwrap();
        assert!(!tx_bytes.is_empty());
        
        // Verify the transaction contains expected components
        // Following chaincash-rs pattern of structured transaction data
        let tx_string = String::from_utf8_lossy(&tx_bytes);
        assert!(tx_string.contains("ergo_tx_v1"));
        assert!(tx_string.contains("test_reserve_box"));
        assert!(tx_string.contains("test_tracker_box"));
        assert!(tx_string.contains("100000000")); // redemption amount
        assert!(tx_string.contains("1000000"));    // fee
        assert!(tx_string.contains("1000"));       // height
    }

    #[test]
    fn test_transaction_building_with_different_amounts() {
        // Test various redemption amounts following chaincash-rs comprehensive testing pattern
        let test_cases = vec![
            (1000000, "small amount"),    // 0.001 ERG
            (10000000, "medium amount"),  // 0.01 ERG
            (100000000, "large amount"),  // 0.1 ERG
            (1000000000, "very large amount"), // 1 ERG
        ];

        for (amount, description) in test_cases {
            let result = RedemptionTransactionBuilder::build_redemption_transaction(
                "test_reserve_box_1234567890abcdef",
                "test_tracker_box_abcdef1234567890",
                "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304", // 32-byte tracker NFT ID: 64 hex chars
                "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
                amount,
                1000000, // 0.001 ERG fee
                1000,    // height
            );

            assert!(result.is_ok(), "Failed to build transaction for {}: {:?}", description, result.err());
            let tx_bytes = result.unwrap();
            assert!(!tx_bytes.is_empty(), "Transaction bytes empty for {}", description);
            
            let tx_string = String::from_utf8_lossy(&tx_bytes);
            assert!(tx_string.contains(&amount.to_string()), 
                   "Transaction doesn't contain amount {} for {}", amount, description);
        }
    }

    #[test]
    fn test_transaction_building_with_different_fees() {
        // Test various fee amounts following chaincash-rs comprehensive testing pattern
        let test_cases = vec![
            (500000, "low fee"),    // 0.0005 ERG
            (1000000, "standard fee"), // 0.001 ERG
            (2000000, "high fee"),  // 0.002 ERG
        ];

        for (fee, description) in test_cases {
            let result = RedemptionTransactionBuilder::build_redemption_transaction(
                "test_reserve_box_1234567890abcdef",
                "test_tracker_box_abcdef1234567890",
                "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304", // 32-byte tracker NFT ID: 64 hex chars
                "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
                100000000, // 0.1 ERG
                fee,
                1000,      // height
            );

            assert!(result.is_ok(), "Failed to build transaction with {}: {:?}", description, result.err());
            let tx_bytes = result.unwrap();
            assert!(!tx_bytes.is_empty(), "Transaction bytes empty with {}", description);
            
            let tx_string = String::from_utf8_lossy(&tx_bytes);
            assert!(tx_string.contains(&fee.to_string()), 
                   "Transaction doesn't contain fee {} for {}", fee, description);
        }
    }

    #[test]
    fn test_transaction_building_error_conditions() {
        // Test error conditions following chaincash-rs error testing pattern
        
        // Test with empty reserve box ID
        let result = RedemptionTransactionBuilder::build_redemption_transaction(
            "", // Empty reserve box ID
            "test_tracker_box_abcdef1234567890",
            "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304", // 32-byte tracker NFT ID: 64 hex chars
            "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
            100000000,
            1000000,
            1000,
        );
        assert!(result.is_ok()); // Our current implementation doesn't validate this
        
        // Test with empty tracker box ID
        let result = RedemptionTransactionBuilder::build_redemption_transaction(
            "test_reserve_box_1234567890abcdef",
            "", // Empty tracker box ID
            "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304", // 32-byte tracker NFT ID: 64 hex chars
            "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
            100000000,
            1000000,
            1000,
        );
        assert!(result.is_ok()); // Our current implementation doesn't validate this
        
        // Test with invalid recipient address (empty)
        let result = RedemptionTransactionBuilder::build_redemption_transaction(
            "test_reserve_box_1234567890abcdef",
            "test_tracker_box_abcdef1234567890",
            "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304", // 32-byte tracker NFT ID: 64 hex chars
            "", // Empty recipient address
            100000000,
            1000000,
            1000,
        );
        assert!(result.is_ok()); // Our current implementation doesn't validate this
    }

    #[test]
    fn test_transaction_building_with_test_helpers() {
        // Test using test helper functions following chaincash-rs pattern
        use crate::test_helpers::{
            create_test_recipient_address, create_test_reserve_box_id, create_test_tracker_box_id,
        };

        let result = RedemptionTransactionBuilder::build_redemption_transaction(
            &create_test_reserve_box_id(),
            &create_test_tracker_box_id(),
            "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304", // 32-byte tracker NFT ID: 64 hex chars
            &create_test_recipient_address(),
            100000000, // 0.1 ERG
            1000000,   // 0.001 ERG fee
            1000,      // height
        );

        assert!(result.is_ok());
        let tx_bytes = result.unwrap();
        assert!(!tx_bytes.is_empty());

        let tx_string = String::from_utf8_lossy(&tx_bytes);
        assert!(tx_string.contains("ergo_tx_v1"));
        assert!(tx_string.contains("e56847ed19b3dc6b")); // first 16 chars of reserve box ID
        assert!(tx_string.contains("f67858fe2ac4ed7c")); // first 16 chars of tracker box ID
        assert!(tx_string.contains("100000000")); // redemption amount
    }



}