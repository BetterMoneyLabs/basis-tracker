use crate::api::TrackerClient;
use anyhow::Result;
use clap::Subcommand;
use serde_json::json;
use std::fs;
use basis_store;
use std::thread;
use std::time::Duration;

#[derive(Subcommand)]
pub enum TestCommands {
    /// Test redemption transaction by polling notes and generating unsigned transaction
    TestRedemption {
        /// Output file for the transaction JSON (optional, defaults to redemption_transaction_{timestamp}.json)
        #[arg(long)]
        output_file: Option<String>,
        
        /// Amount to redeem in nanoERG (optional, defaults to 50% of available debt)
        #[arg(long)]
        amount: Option<u64>,
        
        /// Polling interval in seconds (optional, defaults to 30 seconds)
        #[arg(long, default_value_t = 30)]
        poll_interval: u64,
    },
}

pub async fn handle_test_command(
    cmd: TestCommands,
    client: &TrackerClient,
) -> Result<()> {
    match cmd {
        TestCommands::TestRedemption {
            output_file,
            amount,
            poll_interval,
        } => {
            test_redemption_transaction(client, output_file, amount, poll_interval).await
        }
    }
}

async fn test_redemption_transaction(
    client: &TrackerClient,
    output_file: Option<String>,
    amount: Option<u64>,
    poll_interval: u64,
) -> Result<()> {
    println!("🚀 Starting redemption transaction test...");
    println!("📡 Connecting to server: {}", "configured server URL");
    
    // Verify server health
    match client.health_check().await {
        Ok(healthy) => {
            if healthy {
                println!("✅ Server connection verified");
            } else {
                return Err(anyhow::anyhow!("❌ Server health check failed"));
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("❌ Server health check failed: {}", e));
        }
    }
    
    println!("🔄 Starting note polling loop (checking every {} seconds)...", poll_interval);
    
    loop {
        println!("🔍 Polling for notes...");
        
        // Get all notes from the server
        let notes = match client.get_all_notes().await {
            Ok(notes) => notes,
            Err(e) => {
                eprintln!("⚠️  Failed to retrieve notes: {}", e);
                println!("⏳ Retrying in {} seconds...", poll_interval);
                thread::sleep(Duration::from_secs(poll_interval));
                continue;
            }
        };

        println!("📊 Retrieved {} notes", notes.len());

        // Find a note with sufficient collateral
        if let Some((note, reserve_info)) = find_note_with_sufficient_collateral(client, &notes, amount).await {
            println!("✅ Found suitable note with sufficient collateral!");
            
            // Determine redemption amount
            let redemption_amount = amount.unwrap_or_else(|| {
                let available_debt = note.outstanding_debt();
                std::cmp::min(available_debt, reserve_info.base_info.collateral_amount / 2) // Use up to 50% of available debt
            });
            
            if redemption_amount == 0 {
                println!("⚠️  Redemption amount is 0, skipping this note");
                println!("⏳ Continuing to poll for notes...");
                thread::sleep(Duration::from_secs(poll_interval));
                continue;
            }
            
            println!("💰 Redemption amount: {} nanoERG", redemption_amount);
            
            // Prepare redemption data
            println!("🔧 Preparing redemption data...");
            let redemption_data = match client.prepare_redemption(&note.issuer_pubkey, &note.recipient_pubkey, redemption_amount).await {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("⚠️  Failed to prepare redemption: {}", e);
                    println!("⏳ Continuing to poll for notes...");
                    thread::sleep(Duration::from_secs(poll_interval));
                    continue;
                }
            };
            
            println!("✅ Redemption data prepared successfully");
            println!("   - AVL proof: {} bytes", redemption_data.avl_proof.len());
            println!("   - Tracker signature: {} bytes", redemption_data.tracker_signature.len());
            println!("   - Tracker state digest: {}", redemption_data.tracker_state_digest);
            println!("   - Block height: {}", redemption_data.block_height);
            
            // Generate unsigned transaction JSON
            println!("📝 Generating unsigned transaction...");
            let transaction_json = generate_unsigned_transaction(
                &note.issuer_pubkey,
                &note.recipient_pubkey,
                redemption_amount,
                &redemption_data,
                &reserve_info
            );
            
            // Determine output file name
            let filename = match output_file.as_ref() {
                Some(name) => name.clone(),
                None => format!("redemption_transaction_{}.json", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()),
            };
            
            // Write transaction to file
            println!("💾 Writing transaction to file: {}", filename);
            fs::write(&filename, serde_json::to_string_pretty(&transaction_json)?)?;
            
            println!("🎉 Redemption transaction test completed successfully!");
            println!("📋 Transaction details:");
            println!("   - Issuer: {}", note.issuer_pubkey);
            println!("   - Recipient: {}", note.recipient_pubkey);
            println!("   - Redemption amount: {} nanoERG", redemption_amount);
            println!("   - Transaction saved to: {}", filename);
            println!("   - Source Ergo node: 159.89.116.15:11088");
            
            return Ok(());
        } else {
            println!("⚠️  No suitable notes found with sufficient collateral");
            println!("⏳ Continuing to poll for notes...");
            thread::sleep(Duration::from_secs(poll_interval));
        }
    }
}

async fn find_note_with_sufficient_collateral(
    client: &TrackerClient,
    notes: &[crate::api::SerializableIouNoteWithAge],
    requested_amount: Option<u64>,
) -> Option<(crate::api::SerializableIouNoteWithAge, basis_store::ExtendedReserveInfo)> {
    for note in notes {
        // Get the issuer's reserve information
        let reserves = match client.get_reserves_by_issuer(&note.issuer_pubkey).await {
            Ok(reserves) => reserves,
            Err(_) => continue, // Skip if we can't get reserve info
        };

        if let Some(reserve_info) = reserves.first() {
            let outstanding_debt = note.amount_collected.saturating_sub(note.amount_redeemed);
            let available_collateral = reserve_info.base_info.collateral_amount;

            // Determine the redemption amount to check
            let check_amount = requested_amount.unwrap_or(outstanding_debt);

            // Check if the note has sufficient collateral
            if outstanding_debt > 0 && available_collateral >= check_amount {
                println!("🎯 Found suitable note:");
                println!("   - Issuer: {}", note.issuer_pubkey);
                println!("   - Recipient: {}", note.recipient_pubkey);
                println!("   - Outstanding debt: {} nanoERG", outstanding_debt);
                println!("   - Available collateral: {} nanoERG", available_collateral);

                return Some((note.clone(), reserve_info.clone()));
            }
        }
    }

    None
}

fn generate_unsigned_transaction(
    issuer_pubkey: &str,
    recipient_pubkey: &str,
    amount: u64,
    redemption_data: &crate::api::RedemptionPreparationResponse,
    reserve_info: &basis_store::ExtendedReserveInfo,
) -> serde_json::Value {
    // Convert public keys to proper P2PK addresses
    let recipient_address = pubkey_to_address(recipient_pubkey)
        .unwrap_or_else(|_| format!("invalid_recipient_{}", &recipient_pubkey[..16]));
    
    // Calculate remaining collateral after redemption
    let remaining_collateral = reserve_info.base_info.collateral_amount - amount;
    let transaction_fee = 1_000_000; // 0.001 ERG
    
    // Create the transaction structure following the Ergo node's /wallet/transaction/send format
    json!({
        "requests": [
            {
                "address": recipient_address,
                "value": amount,
                "assets": [],
                "registers": {}
            },
            {
                "address": &reserve_info.base_info.contract_address,
                "value": remaining_collateral - transaction_fee,
                "assets": [
                    {
                        "tokenId": &reserve_info.base_info.tracker_nft_id,
                        "amount": 1
                    }
                ],
                "registers": {
                    "R4": format!("07{}", issuer_pubkey),
                    "R5": format!("64{}", redemption_data.tracker_state_digest),
                    "R6": format!("0e20{}", &reserve_info.base_info.tracker_nft_id)
                }
            }
        ],
        "fee": transaction_fee,
        "inputsRaw": [
            &reserve_info.box_id
        ],
        "dataInputsRaw": [
            &redemption_data.tracker_box_id
        ],
        "metadata": {
            "source": "159.89.116.15:11088",
            "issuer_pubkey": issuer_pubkey,
            "recipient_pubkey": recipient_pubkey,
            "redemption_amount": amount,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            "block_height": redemption_data.block_height
        }
    })
}

// Helper function to convert public key to a P2PK address using ergo-lib
fn pubkey_to_address(pubkey_hex: &str) -> Result<String> {
    use ergo_lib::ergotree_ir::address::{Address, NetworkPrefix};
    use ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint;
    use ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog;
    use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

    let pubkey_bytes = hex::decode(pubkey_hex)
        .map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;

    if pubkey_bytes.len() != 33 {
        return Err(anyhow::anyhow!("Public key must be 33 bytes"));
    }

    let ec_point = EcPoint::sigma_parse_bytes(&pubkey_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid public key format: {}", e))?;

    let prove_dlog = ProveDlog::new(ec_point);
    let address = Address::P2Pk(prove_dlog);

    let encoder = ergo_lib::ergotree_ir::address::AddressEncoder::new(NetworkPrefix::Mainnet);
    Ok(encoder.address_to_str(&address))
}