use basis_store::schnorr_tests;

fn main() {
    let vectors = schnorr_tests::get_comprehensive_test_vectors();
    
    println!("Schnorr Signature Test Vectors (Hex Format)");
    println!("===========================================\n");
    
    for vector in vectors {
        println!("Test Vector: {}", vector.id);
        println!("Description: {}", vector.description);
        println!("Issuer PubKey: {}", hex::encode(vector.issuer_pubkey));
        println!("Recipient PubKey: {}", hex::encode(vector.recipient_pubkey));
        println!("Amount: {}", vector.amount);
        println!("Timestamp: {}", vector.timestamp);
        println!("Signature: {}", hex::encode(vector.signature));
        println!("Signing Message: {}", hex::encode(&vector.signing_message));
        println!("Challenge Hash: {}", hex::encode(vector.challenge_hash));
        println!("Should Verify: {}", vector.should_verify);
        println!("---\n");
    }
}