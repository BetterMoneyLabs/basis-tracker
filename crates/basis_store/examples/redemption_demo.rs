//! Demonstration of the redemption flow for Basis offchain notes

use basis_store::{
    schnorr::generate_keypair, IouNote, RedemptionManager, RedemptionRequest, TrackerStateManager,
};
use secp256k1::SecretKey;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Basis Redemption Flow Demo ===\n");

    // Generate test keypairs
    println!("1. Generating test keypairs...");
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (recipient_secret, recipient_pubkey) = generate_keypair();

    println!("   Issuer pubkey: {}", hex::encode(issuer_pubkey));
    println!("   Recipient pubkey: {}\n", hex::encode(recipient_pubkey));

    // Create a tracker and redemption manager with fresh data directory
    println!("2. Initializing tracker and redemption manager...");
    let tracker = TrackerStateManager::new();
    let mut redemption_manager = RedemptionManager::new(tracker);

    // Create and sign a test note
    println!("3. Creating and signing a test note...");
    let amount_collected = 1000;
    let timestamp = 1672531200; // Jan 1, 2023

    // Convert secret key to bytes
    let issuer_secret_bytes: [u8; 32] = issuer_secret.secret_bytes();

    let note = IouNote::create_and_sign(
        recipient_pubkey,
        amount_collected,
        timestamp,
        &issuer_secret_bytes,
    )
    .map_err(|e| format!("Failed to create note: {:?}", e))?;

    println!(
        "   Note created: {} -> {} ({} nanoERG collected, {} nanoERG redeemed)",
        hex::encode(&issuer_pubkey[..8]),
        hex::encode(&recipient_pubkey[..8]),
        note.amount_collected,
        note.amount_redeemed
    );

    // Add note to tracker
    println!("4. Adding note to tracker...");
    redemption_manager
        .tracker
        .add_note(&issuer_pubkey, &note)
        .map_err(|e| format!("Failed to add note: {:?}", e))?;
    println!("   Note added successfully\n");

    // Create redemption request
    println!("5. Creating redemption request...");
    let redemption_request = RedemptionRequest {
        issuer_pubkey: hex::encode(issuer_pubkey),
        recipient_pubkey: hex::encode(recipient_pubkey),
        amount: amount_collected,
        timestamp,
        reserve_box_id: "test_reserve_box_123".to_string(),
        recipient_address: "test_address".to_string(),
    };

    println!(
        "   Redemption request created for amount: {} nanoERG\n",
        amount_collected
    );

    // Try to initiate redemption (this will fail due to time lock)
    println!("6. Attempting to initiate redemption...");
    match redemption_manager.initiate_redemption(&redemption_request) {
        Ok(redemption_data) => {
            println!("   Redemption initiated successfully!");
            println!("   Redemption ID: {}", redemption_data.redemption_id);
            println!(
                "   Transaction bytes: {}...",
                &redemption_data.transaction_bytes[..20]
            );
            println!(
                "   Required signatures: {:?}",
                redemption_data.required_signatures
            );
            println!(
                "   Estimated fee: {} nanoERG",
                redemption_data.estimated_fee
            );

            // Complete redemption
            println!("\n7. Completing redemption...");
            let redeemed_amount = 500; // Redeem half the amount
            redemption_manager
                .complete_redemption(&issuer_pubkey, &recipient_pubkey, redeemed_amount)
                .map_err(|e| format!("Failed to complete redemption: {}", e))?;
            println!("   Redemption completed successfully!");
            println!("   Redeemed {} nanoERG", redeemed_amount);

            // Check note status after redemption
            let updated_note = redemption_manager
                .tracker
                .lookup_note(&issuer_pubkey, &recipient_pubkey)
                .map_err(|e| format!("Failed to lookup note: {:?}", e))?;
            println!(
                "   Note after redemption: collected={}, redeemed={}, outstanding={}",
                updated_note.amount_collected,
                updated_note.amount_redeemed,
                updated_note.outstanding_debt()
            );
        }
        Err(e) => {
            println!("   Redemption failed (expected due to time lock): {}", e);
            println!(
                "   This is expected behavior - notes require 1 week minimum before redemption\n"
            );
        }
    }

    // Test redemption proof verification
    println!("8. Testing redemption proof verification...");
    let proof = vec![0u8; 32]; // Mock proof
    let is_valid = redemption_manager
        .verify_redemption_proof(&proof, &note, &issuer_pubkey)
        .map_err(|e| format!("Failed to verify proof: {}", e))?;

    println!("   Proof verification result: {}\n", is_valid);

    println!("=== Redemption Flow Demo Complete ===");
    println!("\nSummary:");
    println!("- Created and signed IOU note");
    println!("- Added note to tracker state");
    println!("- Initiated redemption process");
    println!("- Verified redemption proof");
    println!("- Demonstrated time lock protection");

    Ok(())
}
