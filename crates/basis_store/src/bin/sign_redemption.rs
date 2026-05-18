use basis_store::schnorr;

fn main() {
    // Alice's secret key and public key
    let alice_secret_hex = "c693d626538e9dd926519c13f3855412d60aaaa9c8818e7725415a45e92f3108";
    let alice_pubkey_hex = "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83";
    let bob_pubkey_hex = "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea";
    
    let alice_secret = hex::decode(alice_secret_hex).expect("Invalid secret hex");
    let alice_pubkey = hex::decode(alice_pubkey_hex).expect("Invalid pubkey hex");
    let bob_pubkey = hex::decode(bob_pubkey_hex).expect("Invalid recipient pubkey hex");
    
    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&alice_secret);
    
    let mut alice_pk = [0u8; 33];
    alice_pk.copy_from_slice(&alice_pubkey);
    
    let mut bob_pk = [0u8; 33];
    bob_pk.copy_from_slice(&bob_pubkey);
    
    // The old reserve has total_debt = 50000000
    let total_debt: u64 = 50000000;
    let timestamp: u64 = 1778743808367; // Current timestamp in millis
    
    let message = schnorr::signing_message(&alice_pk, &bob_pk, total_debt, timestamp);
    println!("Message: {}", hex::encode(&message));
    
    let signature = schnorr::schnorr_sign(&message, &secret_bytes, &alice_pk)
        .expect("Failed to sign");
    
    println!("Signature: {}", hex::encode(&signature));
    
    // Verify
    schnorr::schnorr_verify(&signature, &message, &alice_pk)
        .expect("Verification failed");
    println!("Signature verified!");
}
