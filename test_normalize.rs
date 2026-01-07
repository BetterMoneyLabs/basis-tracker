
fn main() {
    let key_with_prefix = "0703bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273";
    let key_without_prefix = "03bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273";
    
    let normalized = basis_store::normalize_public_key(key_with_prefix);
    println!("Original: {}", key_with_prefix);
    println!("Normalized: {}", normalized);
    println!("Expected: {}", key_without_prefix);
    println!("Match: {}", normalized == key_without_prefix);
}

