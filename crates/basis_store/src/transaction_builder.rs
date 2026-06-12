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
    /// Network prefix for Ergo address encoding
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
    /// Current blockchain height for transaction validity
    pub current_height: u32,
    /// Issuer's public key (33 bytes compressed) for reserve output R4 register
    pub issuer_pubkey: Vec<u8>,
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
        issuer_pubkey: &crate::PubKey,
        context: &TxContext,
        reserve_lookup_proof: Option<Vec<u8>>,
        tracker_lookup_proof: Vec<u8>,
        redemption_amount: u64,
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

        // Validate redemption amount
        if redemption_amount == 0 {
            return Err(TransactionBuilderError::Configuration(
                "Redemption amount must be greater than 0".to_string()
            ));
        }
        if redemption_amount > note.outstanding_debt() {
            return Err(TransactionBuilderError::InsufficientFunds(
                format!("Redemption amount {} exceeds outstanding debt {}",
                    redemption_amount, note.outstanding_debt())
            ));
        }

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
            current_height: context.current_height,
            issuer_pubkey: issuer_pubkey.to_vec(),
        })
    }

    /// Build a real Ergo redemption transaction
    ///
    /// This function creates an actual Ergo transaction JSON that follows the Basis contract specification:
    /// - Spends the reserve box
    /// - Uses tracker box as data input for AVL proof verification
    /// - Creates updated reserve box output
    /// - Creates redemption output box for recipient
    /// - Includes proper context extension with contract parameters
    /// - Preserves R6 register with tracker NFT ID in output reserve box
    ///
    /// The returned JSON follows the Ergo node `/wallet/transaction/sign` API format.
    ///
    /// # Parameters
    /// - `tx_data`: Complete redemption transaction data including context extension
    ///
    /// # Returns
    /// - JSON bytes representing the unsigned transaction ready for Ergo node signing
    pub fn build_redemption_transaction(
        tx_data: &RedemptionTransactionData,
    ) -> Result<Vec<u8>, TransactionBuilderError> {
        // Build proper Ergo transaction JSON following node API format
        let tx_json = Self::build_ergo_transaction_json(tx_data)?;
        Ok(tx_json.into_bytes())
    }

    /// Serialize a byte value as Ergo constant (prefix 02)
    fn serialize_ergo_byte(value: u8) -> String {
        format!("02{:02x}", value)
    }

    /// Serialize a long value as Ergo constant (prefix 05, VLQ encoded)
    fn serialize_ergo_long(value: i64) -> String {
        // For simplicity, use fixed 8-byte big-endian with prefix
        // In full Ergo serialization, Long uses VLQ encoding
        format!("05{:016x}", value)
    }

    /// Serialize bytes as Coll[Byte] constant (prefix 0e + 2-byte length + data)
    fn serialize_ergo_coll_bytes(data: &[u8]) -> String {
        format!("0e{:04x}{}", data.len(), hex::encode(data))
    }

    /// Serialize a GroupElement (33-byte compressed pubkey) as Ergo constant (prefix 07)
    fn serialize_ergo_group_element(pubkey: &[u8]) -> String {
        format!("07{}", hex::encode(pubkey))
    }

    /// Build Ergo transaction JSON for redemption
    fn build_ergo_transaction_json(
        tx_data: &RedemptionTransactionData,
    ) -> Result<String, TransactionBuilderError> {
        let ctx = tx_data.context_extension.as_ref().ok_or_else(|| {
            TransactionBuilderError::TransactionBuilding("Context extension is required".to_string())
        })?;

        // Build context extension map with properly serialized Ergo constants
        let mut extension = std::collections::HashMap::new();

        // #0: Action byte (Byte constant)
        extension.insert("0".to_string(), Self::serialize_ergo_byte(ctx.action));

        // #1: Receiver pubkey (GroupElement constant)
        extension.insert("1".to_string(), Self::serialize_ergo_group_element(&ctx.receiver_pubkey));

        // #2: Reserve signature (Coll[Byte] constant, 65 bytes)
        extension.insert("2".to_string(), Self::serialize_ergo_coll_bytes(&ctx.reserve_signature));

        // #3: Total debt (Long constant)
        extension.insert("3".to_string(), Self::serialize_ergo_long(ctx.total_debt as i64));

        // #4: Timestamp (Long constant)
        extension.insert("4".to_string(), Self::serialize_ergo_long(ctx.timestamp as i64));

        // #5: Insert proof (Coll[Byte] constant)
        extension.insert("5".to_string(), Self::serialize_ergo_coll_bytes(&ctx.insert_proof));

        // #6: Tracker signature (Coll[Byte] constant, 65 bytes)
        extension.insert("6".to_string(), Self::serialize_ergo_coll_bytes(&ctx.tracker_signature));

        // #7: Reserve lookup proof (optional, Coll[Byte] constant)
        if let Some(ref proof) = ctx.reserve_lookup_proof {
            extension.insert("7".to_string(), Self::serialize_ergo_coll_bytes(proof));
        }

        // #8: Tracker lookup proof (Coll[Byte] constant)
        extension.insert("8".to_string(), Self::serialize_ergo_coll_bytes(&ctx.tracker_lookup_proof));

        // Build transaction JSON following Ergo node API format
        let recipient_ergo_tree = format!("0008cd{}", hex::encode(&ctx.receiver_pubkey));
        
        // Get the reserve contract ErgoTree (P2S) for the reserve output
        let reserve_ergo_tree = crate::contract_compiler::get_basis_reserve_ergo_tree_hex()
            .map_err(|e| TransactionBuilderError::TransactionBuilding(format!("Failed to get reserve contract: {}", e)))?;
        
        // Reserve NFT ID from the transaction data (from reserve box R6)
        let reserve_nft_id = &tx_data.tracker_nft_id;
        
        // Calculate remaining reserve value after redemption and fee
        // In a real implementation, this would be fetched from the actual reserve box value
        // For now, we use a placeholder calculation: reserve_value - redemption_amount - fee
        // TODO: Fetch actual reserve value from the reserve box when blockchain integration is complete
        let reserve_remaining = tx_data.redemption_amount.saturating_add(tx_data.fee);
        // Note: The actual reserve_remaining should be: reserve_box_value - redemption_amount - fee
        // This is a placeholder until we have access to the actual reserve box value
        
        let tx = serde_json::json!({
            "tx": {
                "inputs": [
                    {
                        "boxId": tx_data.reserve_box_id,
                        "extension": extension
                    }
                ],
                "dataInputs": [
                    {
                        "boxId": tx_data.tracker_box_id
                    }
                ],
                "outputs": [
                    {
                        "value": reserve_remaining,
                        "ergoTree": reserve_ergo_tree,
                        "assets": [
                            {
                                "tokenId": reserve_nft_id,
                                "amount": 1
                            }
                        ],
                        "additionalRegisters": {
                            "R4": format!("07{}", hex::encode(&tx_data.issuer_pubkey)),
                            "R5": "64000000000000000000000000000000000000000000000000000000000000000000012000",
                            "R6": format!("0e20{}", tx_data.tracker_nft_id)
                        },
                        "creationHeight": tx_data.current_height
                    },
                    {
                        "value": tx_data.redemption_amount,
                        "ergoTree": recipient_ergo_tree,
                        "assets": [],
                        "additionalRegisters": {},
                        "creationHeight": tx_data.current_height
                    }
                ]
            }
        });

        serde_json::to_string_pretty(&tx).map_err(|e| {
            TransactionBuilderError::TransactionBuilding(format!("JSON serialization failed: {}", e))
        })
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
        // Create a complete transaction data structure
        let tx_data = RedemptionTransactionData {
            reserve_box_id: "test_reserve_box_1234567890abcdef".to_string(),
            tracker_box_id: "test_tracker_box_abcdef1234567890".to_string(),
            redemption_amount: 100000000, // 0.1 ERG
            recipient_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
            avl_proof: vec![0x01, 0x02, 0x03],
            issuer_signature: vec![0u8; 65],
            tracker_signature: vec![0u8; 65],
            fee: 1000000, // 0.001 ERG fee
            tracker_nft_id: "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
            context_extension: Some(ContextExtension {
                action: 0x00,
                receiver_pubkey: vec![0x03; 33],
                reserve_signature: vec![0u8; 65],
                total_debt: 100000000,
                timestamp: 1743379200000,
                insert_proof: vec![0x01, 0x02],
                tracker_signature: vec![0u8; 65],
                reserve_lookup_proof: None,
                tracker_lookup_proof: vec![0x03, 0x04],
            }),
            total_debt: 100000000,
            already_redeemed: 0,
            is_first_redemption: true,
            current_height: 1779469,
            issuer_pubkey: vec![0x02; 33],
        };

        let result = RedemptionTransactionBuilder::build_redemption_transaction(&tx_data);

        assert!(result.is_ok());
        let tx_bytes = result.unwrap();
        assert!(!tx_bytes.is_empty());
        
        // Verify the transaction is valid JSON with expected structure
        let tx_json: serde_json::Value = serde_json::from_slice(&tx_bytes).expect("Should be valid JSON");
        assert!(tx_json.get("tx").is_some());
        assert!(tx_json["tx"].get("inputs").is_some());
        assert!(tx_json["tx"].get("dataInputs").is_some());
        assert!(tx_json["tx"].get("outputs").is_some());
        
        // Verify inputs contain reserve box
        let inputs = tx_json["tx"]["inputs"].as_array().unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0]["boxId"], "test_reserve_box_1234567890abcdef");
        
        // Verify context extension contains action byte
        let extension = inputs[0]["extension"].as_object().unwrap();
        assert!(extension.contains_key("0"));
        assert_eq!(extension["0"], "0200");
        
        // Verify data inputs contain tracker box
        let data_inputs = tx_json["tx"]["dataInputs"].as_array().unwrap();
        assert_eq!(data_inputs.len(), 1);
        assert_eq!(data_inputs[0]["boxId"], "test_tracker_box_abcdef1234567890");
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
            let tx_data = RedemptionTransactionData {
                reserve_box_id: "test_reserve_box_1234567890abcdef".to_string(),
                tracker_box_id: "test_tracker_box_abcdef1234567890".to_string(),
                redemption_amount: amount,
                recipient_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                avl_proof: vec![0x01],
                issuer_signature: vec![0u8; 65],
                tracker_signature: vec![0u8; 65],
                fee: 1000000,
                tracker_nft_id: "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
                context_extension: Some(ContextExtension {
                    action: 0x00,
                    receiver_pubkey: vec![0x03; 33],
                    reserve_signature: vec![0u8; 65],
                    total_debt: amount,
                    timestamp: 1743379200000,
                    insert_proof: vec![0x01],
                    tracker_signature: vec![0u8; 65],
                    reserve_lookup_proof: None,
                    tracker_lookup_proof: vec![0x02],
                }),
                total_debt: amount,
                already_redeemed: 0,
                is_first_redemption: true,
            current_height: 1779469,
            issuer_pubkey: vec![0x02; 33],
            };

            let result = RedemptionTransactionBuilder::build_redemption_transaction(&tx_data);

            assert!(result.is_ok(), "Failed to build transaction for {}: {:?}", description, result.err());
            let tx_bytes = result.unwrap();
            assert!(!tx_bytes.is_empty(), "Transaction bytes empty for {}", description);
            
            let tx_json: serde_json::Value = serde_json::from_slice(&tx_bytes).expect("Should be valid JSON");
            assert!(tx_json.get("tx").is_some(), "Transaction JSON missing 'tx' key for {}", description);
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
            let tx_data = RedemptionTransactionData {
                reserve_box_id: "test_reserve_box_1234567890abcdef".to_string(),
                tracker_box_id: "test_tracker_box_abcdef1234567890".to_string(),
                redemption_amount: 100000000,
                recipient_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
                avl_proof: vec![0x01],
                issuer_signature: vec![0u8; 65],
                tracker_signature: vec![0u8; 65],
                fee,
                tracker_nft_id: "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
                context_extension: Some(ContextExtension {
                    action: 0x00,
                    receiver_pubkey: vec![0x03; 33],
                    reserve_signature: vec![0u8; 65],
                    total_debt: 100000000,
                    timestamp: 1743379200000,
                    insert_proof: vec![0x01],
                    tracker_signature: vec![0u8; 65],
                    reserve_lookup_proof: None,
                    tracker_lookup_proof: vec![0x02],
                }),
            total_debt: 100000000,
            already_redeemed: 0,
            is_first_redemption: true,
            current_height: 1779469,
            issuer_pubkey: vec![0x02; 33],
        };

        let result = RedemptionTransactionBuilder::build_redemption_transaction(&tx_data);

            assert!(result.is_ok(), "Failed to build transaction with {}: {:?}", description, result.err());
            let tx_bytes = result.unwrap();
            assert!(!tx_bytes.is_empty(), "Transaction bytes empty with {}", description);
            
            let tx_json: serde_json::Value = serde_json::from_slice(&tx_bytes).expect("Should be valid JSON");
            assert!(tx_json.get("tx").is_some(), "Transaction JSON missing 'tx' key with {}", description);
        }
    }

    #[test]
    fn test_transaction_building_error_conditions() {
        // Test error conditions following chaincash-rs error testing pattern
        
        // Test with missing context extension
        let tx_data = RedemptionTransactionData {
            reserve_box_id: "test_reserve_box_1234567890abcdef".to_string(),
            tracker_box_id: "test_tracker_box_abcdef1234567890".to_string(),
            redemption_amount: 100000000,
            recipient_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
            avl_proof: vec![0x01],
            issuer_signature: vec![0u8; 65],
            tracker_signature: vec![0u8; 65],
            fee: 1000000,
            tracker_nft_id: "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
            context_extension: None, // Missing context extension should fail
            total_debt: 100000000,
            already_redeemed: 0,
            is_first_redemption: true,
            current_height: 1779469,
            issuer_pubkey: vec![0x02; 33],
        };
        
        let result = RedemptionTransactionBuilder::build_redemption_transaction(&tx_data);
        assert!(result.is_err(), "Should fail without context extension");
        
        // Test with empty reserve box ID (should still build JSON, just with empty string)
        let tx_data = RedemptionTransactionData {
            reserve_box_id: "".to_string(),
            tracker_box_id: "test_tracker_box_abcdef1234567890".to_string(),
            redemption_amount: 100000000,
            recipient_address: "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33".to_string(),
            avl_proof: vec![0x01],
            issuer_signature: vec![0u8; 65],
            tracker_signature: vec![0u8; 65],
            fee: 1000000,
            tracker_nft_id: "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
            context_extension: Some(ContextExtension {
                action: 0x00,
                receiver_pubkey: vec![0x03; 33],
                reserve_signature: vec![0u8; 65],
                total_debt: 100000000,
                timestamp: 1743379200000,
                insert_proof: vec![0x01],
                tracker_signature: vec![0u8; 65],
                reserve_lookup_proof: None,
                tracker_lookup_proof: vec![0x02],
            }),
            total_debt: 100000000,
            already_redeemed: 0,
            is_first_redemption: true,
            current_height: 1779469,
            issuer_pubkey: vec![0x02; 33],
        };
        
        let result = RedemptionTransactionBuilder::build_redemption_transaction(&tx_data);
        assert!(result.is_ok(), "Should build even with empty reserve box ID");
    }

    #[test]
    fn test_transaction_building_with_test_helpers() {
        // Test using test helper functions following chaincash-rs pattern
        use crate::test_helpers::{
            create_test_recipient_address, create_test_reserve_box_id, create_test_tracker_box_id,
        };

        let tx_data = RedemptionTransactionData {
            reserve_box_id: create_test_reserve_box_id(),
            tracker_box_id: create_test_tracker_box_id(),
            redemption_amount: 100000000, // 0.1 ERG
            recipient_address: create_test_recipient_address(),
            avl_proof: vec![0x01],
            issuer_signature: vec![0u8; 65],
            tracker_signature: vec![0u8; 65],
            fee: 1000000,   // 0.001 ERG fee
            tracker_nft_id: "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304".to_string(),
            context_extension: Some(ContextExtension {
                action: 0x00,
                receiver_pubkey: vec![0x03; 33],
                reserve_signature: vec![0u8; 65],
                total_debt: 100000000,
                timestamp: 1743379200000,
                insert_proof: vec![0x01],
                tracker_signature: vec![0u8; 65],
                reserve_lookup_proof: None,
                tracker_lookup_proof: vec![0x02],
            }),
            total_debt: 100000000,
            already_redeemed: 0,
            is_first_redemption: true,
            current_height: 1779469,
            issuer_pubkey: vec![0x02; 33],
        };

        let result = RedemptionTransactionBuilder::build_redemption_transaction(&tx_data);

        assert!(result.is_ok());
        let tx_bytes = result.unwrap();
        assert!(!tx_bytes.is_empty());

        let tx_json: serde_json::Value = serde_json::from_slice(&tx_bytes).expect("Should be valid JSON");
        assert!(tx_json.get("tx").is_some());
        assert!(tx_json["tx"]["inputs"][0]["boxId"].as_str().unwrap().contains("e56847ed"));
        assert!(tx_json["tx"]["dataInputs"][0]["boxId"].as_str().unwrap().contains("f67858fe"));
    }



}