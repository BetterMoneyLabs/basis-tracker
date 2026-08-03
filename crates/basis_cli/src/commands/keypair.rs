use crate::crypto::KeyPair;
use anyhow::Result;
use clap::Args;
use serde::Serialize;

#[derive(Args)]
#[command(name = "generate-keypair", about = "Generate a new secp256k1 keypair")]
pub struct GenerateKeypairArgs {}

/// A freshly generated secp256k1 keypair (hex-encoded).
#[derive(Debug, Serialize)]
pub struct KeypairResult {
    pub public_key_hex: String,
    pub private_key_hex: String,
}

/// Generate a new secp256k1 keypair.
pub fn generate_keypair() -> Result<KeypairResult> {
    let keypair = KeyPair::new()?;
    Ok(KeypairResult {
        public_key_hex: hex::encode(keypair.get_public_key_bytes()),
        private_key_hex: hex::encode(keypair.get_private_key_bytes()),
    })
}

pub async fn handle_generate_keypair_command(_args: GenerateKeypairArgs, json: bool) -> Result<()> {
    let result = generate_keypair()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Keypair generated successfully!");
        println!("Public Key (hex): {}", result.public_key_hex);
        println!("Private Key (hex): {}", result.private_key_hex);
    }

    Ok(())
}
