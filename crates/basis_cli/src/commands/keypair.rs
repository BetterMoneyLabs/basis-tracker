use anyhow::Result;
use clap::Args;
use crate::crypto::KeyPair;

#[derive(Args)]
#[command(name = "generate-keypair", about = "Generate a new secp256k1 keypair")]
pub struct GenerateKeypairArgs {}

pub async fn handle_generate_keypair_command(_args: GenerateKeypairArgs) -> Result<()> {
    let keypair = KeyPair::new()?;
    let public_key = keypair.get_public_key_bytes();
    let private_key = keypair.get_private_key_bytes();

    println!("Keypair generated successfully!");
    println!("Public Key (hex): {}", hex::encode(public_key));
    println!("Private Key (hex): {}", hex::encode(private_key));

    Ok(())
}