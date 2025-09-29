//! Simple demonstration of the redemption flow for Basis offchain notes

use basis_store::{
    schnorr::generate_keypair, IouNote, RedemptionManager, RedemptionRequest, TrackerStateManager,
};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Simple Basis Redemption Flow Demo ===\n");

    // Generate test keypairs
    println!("1. Generating test keypairs...");
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_recipient_secret, recipient_pubkey) = generate_keypair();

    println!("   Issuer pubkey: {}", hex::encode(issuer_pubkey));
    println!("   Recipient pubkey: {}\n", hex::encode(recipient_pubkey));

    // Create a tracker and redemption manager
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

    // Test note methods
    println!("5. Testing note methods...");
    println!("   Outstanding debt: {} nanoERG", note.outstanding_debt());
    println!("   Fully redeemed: {}\n", note.is_fully_redeemed());

    // Test partial redemption
    println!("6. Testing partial redemption...");
    let redeemed_amount = 500; // Redeem half the amount
    redemption_manager
        .complete_redemption(&issuer_pubkey, &recipient_pubkey, redeemed_amount)
        .map_err(|e| format!("Failed to complete redemption: {}", e))?;
    println!("   Redeemed {} nanoERG successfully", redeemed_amount);

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

    // Test full redemption
    println!("\n7. Testing full redemption...");
    let remaining_amount = updated_note.outstanding_debt();
    redemption_manager
        .complete_redemption(&issuer_pubkey, &recipient_pubkey, remaining_amount)
        .map_err(|e| format!("Failed to complete redemption: {}", e))?;
    println!("   Redeemed remaining {} nanoERG", remaining_amount);

    // Check final note status
    let final_note = redemption_manager
        .tracker
        .lookup_note(&issuer_pubkey, &recipient_pubkey)
        .map_err(|e| format!("Failed to lookup note: {:?}", e))?;
    println!(
        "   Final note status: collected={}, redeemed={}, outstanding={}",
        final_note.amount_collected,
        final_note.amount_redeemed,
        final_note.outstanding_debt()
    );
    println!("   Fully redeemed: {}", final_note.is_fully_redeemed());

    println!("\n=== Simple Redemption Flow Demo Complete ===");
    println!("\nSummary:");
    println!("- Created and signed IOU note with collected/redeemed tracking");
    println!("- Added note to tracker state");
    println!("- Tested partial redemption (500 nanoERG)");
    println!("- Tested full redemption (remaining 500 nanoERG)");
    println!("- Verified note tracking cumulative debt relationship");

    Ok(())
}
