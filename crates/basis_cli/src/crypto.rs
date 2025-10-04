use anyhow::Result;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey, KeyPair as SecpKeyPair};

pub type PubKey = [u8; 33];
pub type Signature = [u8; 65];

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub keypair: SecpKeyPair,
}

impl KeyPair {
    pub fn new() -> Result<Self> {
        let secp = Secp256k1::new();
        let keypair = SecpKeyPair::new(&secp, &mut rand::thread_rng());
        
        Ok(Self { keypair })
    }

    pub fn from_private_key(private_key: SecretKey) -> Result<Self> {
        let secp = Secp256k1::new();
        let keypair = SecpKeyPair::from_secret_key(&secp, &private_key);
        
        Ok(Self { keypair })
    }

    pub fn sign_message(&self, message: &[u8]) -> Result<Signature> {
        let secp = Secp256k1::new();
        let message_hash = blake2b_hash(message);
        let message = Message::from_slice(&message_hash)?;
        
        // Create Schnorr signature (following chaincash-rs approach)
        let signature = secp.sign_schnorr(&message, &self.keypair);
        let sig_bytes = signature.as_ref();
        
        // Convert to 65-byte format (33-byte a + 32-byte z)
        let mut schnorr_sig = [0u8; 65];
        schnorr_sig[..64].copy_from_slice(sig_bytes);
        
        Ok(schnorr_sig)
    }

    pub fn verify_signature(
        message: &[u8],
        signature: &Signature,
        public_key: &PubKey,
    ) -> Result<bool> {
        let secp = Secp256k1::new();
        let message_hash = blake2b_hash(message);
        let message = Message::from_slice(&message_hash)?;
        
        let public_key = PublicKey::from_slice(public_key)?;
        
        // Convert 65-byte signature to 64-byte Schnorr
        let schnorr_sig = secp256k1::schnorr::Signature::from_slice(&signature[..64])?;
        
        Ok(secp.verify_schnorr(&schnorr_sig, &message, &public_key.x_only_public_key().0).is_ok())
    }

    pub fn get_public_key_bytes(&self) -> PubKey {
        let mut pubkey_bytes = [0u8; 33];
        pubkey_bytes.copy_from_slice(&self.keypair.public_key().serialize());
        pubkey_bytes
    }

    pub fn get_private_key_bytes(&self) -> [u8; 32] {
        self.keypair.secret_bytes()
    }

    pub fn from_private_key_bytes(bytes: &[u8; 32]) -> Result<Self> {
        let private_key = SecretKey::from_slice(bytes)?;
        Self::from_private_key(private_key)
    }
}

fn blake2b_hash(data: &[u8]) -> [u8; 32] {
    use blake2::{Blake2b, Digest};
    
    let mut hasher = Blake2b::<blake2::digest::consts::U32>::new();
    hasher.update(data);
    let result = hasher.finalize();
    
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result[..32]);
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() -> Result<()> {
        let keypair = KeyPair::new()?;
        let pubkey_bytes = keypair.get_public_key_bytes();
        
        assert_eq!(pubkey_bytes.len(), 33);
        assert!(pubkey_bytes[0] == 0x02 || pubkey_bytes[0] == 0x03);
        
        Ok(())
    }

    #[test]
    fn test_signature_verification() -> Result<()> {
        let keypair = KeyPair::new()?;
        let message = b"test message";
        
        let signature = keypair.sign_message(message)?;
        assert_eq!(signature.len(), 65);
        
        let pubkey_bytes = keypair.get_public_key_bytes();
        let verified = KeyPair::verify_signature(message, &signature, &pubkey_bytes)?;
        
        assert!(verified);
        
        Ok(())
    }
}