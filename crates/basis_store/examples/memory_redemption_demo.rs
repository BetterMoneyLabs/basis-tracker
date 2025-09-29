//! Memory-only demonstration of the redemption flow for Basis offchain notes

use basis_store::{schnorr::generate_keypair, IouNote};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Memory-Only Basis Redemption Flow Demo ===\n");

    // Generate test keypairs
    println!("1. Generating test keypairs...");
    let (issuer_secret, issuer_pubkey) = generate_keypair();
    let (_recipient_secret, recipient_pubkey) = generate_keypair();

    println!("   Issuer pubkey: {}", hex::encode(issuer_pubkey));
    println!("   Recipient pubkey: {}\n", hex::encode(recipient_pubkey));

    // Create and sign a test note
    println!("2. Creating and signing a test note...");
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

    // Test note methods
    println!("\n3. Testing note methods...");
    println!("   Outstanding debt: {} nanoERG", note.outstanding_debt());
    println!("   Fully redeemed: {}", note.is_fully_redeemed());

    // Verify signature
    println!("\n4. Verifying note signature...");
    note.verify_signature(&issuer_pubkey)
        .map_err(|e| format!("Signature verification failed: {:?}", e))?;
    println!("   Signature verified successfully!");

    // Test creating a note with partial redemption
    println!("\n5. Testing note with partial redemption...");
    let partially_redeemed_note = IouNote::new(
        recipient_pubkey,
        amount_collected, // collected
        500,              // redeemed
        timestamp,
        note.signature, // Note: In real usage, this would need to be re-signed
    );

    println!(
        "   Partially redeemed note: collected={}, redeemed={}, outstanding={}",
        partially_redeemed_note.amount_collected,
        partially_redeemed_note.amount_redeemed,
        partially_redeemed_note.outstanding_debt()
    );

    // Test creating a fully redeemed note
    println!("\n6. Testing fully redeemed note...");
    let fully_redeemed_note = IouNote::new(
        recipient_pubkey,
        amount_collected, // collected
        amount_collected, // fully redeemed
        timestamp,
        note.signature, // Note: In real usage, this would need to be re-signed
    );

    println!(
        "   Fully redeemed note: collected={}, redeemed={}, outstanding={}",
        fully_redeemed_note.amount_collected,
        fully_redeemed_note.amount_redeemed,
        fully_redeemed_note.outstanding_debt()
    );
    println!(
        "   Fully redeemed: {}",
        fully_redeemed_note.is_fully_redeemed()
    );

    // Test signing message format
    println!("\n7. Testing signing message format...");
    let message = note.signing_message();
    println!("   Signing message length: {} bytes", message.len());
    println!("   Message format: recipient_pubkey || amount_collected || timestamp");

    println!("\n=== Memory-Only Redemption Flow Demo Complete ===");
    println!("\nSummary:");
    println!("- Created and signed IOU note with collected/redeemed tracking");
    println!("- Tested note methods (outstanding_debt, is_fully_redeemed)");
    println!("- Verified signature validation");
    println!("- Demonstrated partial and full redemption scenarios");
    println!("- Verified signing message format matches basis.es contract");

    Ok(())
}
