use anyhow::Result;
use secp256k1::{KeyPair as SecpKeyPair, Message, PublicKey, Secp256k1, SecretKey};

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
        use secp256k1::{Secp256k1, SecretKey};
        use blake2::{Blake2b, Digest};
        use generic_array::typenum::U32;

        let secp = Secp256k1::new();

        // Generate a random nonce for the Schnorr signature
        let nonce_secret = SecretKey::new(&mut secp256k1::rand::thread_rng());
        let a_point = secp256k1::PublicKey::from_secret_key(&secp, &nonce_secret);
        let a_bytes = a_point.serialize();

        // Compute challenge e = H(a || message || issuer_pubkey)
        let issuer_pubkey_bytes = self.get_public_key_bytes();
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(&a_bytes);
        hasher.update(message);
        hasher.update(&issuer_pubkey_bytes);
        let e_bytes = hasher.finalize();

        // Take first 32 bytes for the scalar
        let e_bytes_32: [u8; 32] = e_bytes[..32]
            .try_into()
            .map_err(|_| anyhow::anyhow!("Failed to convert challenge to 32 bytes"))?;

        // Convert to Scalar using the same approach as in basis_store
        let e_scalar = secp256k1::Scalar::from_be_bytes(e_bytes_32)
            .map_err(|_| anyhow::anyhow!("Failed to create scalar from challenge"))?;

        // Get the secret key scalar
        let secret_key_scalar = secp256k1::Scalar::from_be_bytes(self.keypair.secret_bytes())
            .map_err(|_| anyhow::anyhow!("Failed to create scalar from secret key"))?;

        // Convert scalars to their big integer representations for modular arithmetic
        let k_big = num_bigint::BigUint::from_bytes_be(&nonce_secret.secret_bytes());
        let s_big = num_bigint::BigUint::from_bytes_be(&self.keypair.secret_bytes());
        let e_big = num_bigint::BigUint::from_bytes_be(&e_scalar.to_be_bytes());

        // Curve order for secp256k1
        let n = num_bigint::BigUint::from_bytes_be(&[
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36,
            0x41, 0x41,
        ]);

        // Compute z = k + e * s (mod n)
        let e_times_s = (&e_big * &s_big) % &n;
        let z_big = (&k_big + &e_times_s) % &n;

        // Ensure z is in the valid range [1, n-1]
        if z_big == num_bigint::BigUint::from(0u32) || z_big >= n {
            return Err(anyhow::anyhow!("Invalid signature"));
        }

        // Convert back to bytes
        let z_vec = z_big.to_bytes_be();
        let mut z_bytes = [0u8; 32];
        if z_vec.len() > 32 {
            z_bytes.copy_from_slice(&z_vec[z_vec.len() - 32..]);
        } else if z_vec.len() < 32 {
            let start_idx = 32 - z_vec.len();
            z_bytes[start_idx..].copy_from_slice(&z_vec);
        } else {
            z_bytes.copy_from_slice(&z_vec);
        }

        // Create the signature (a || z) - 33 bytes for a, 32 bytes for z
        let mut signature = [0u8; 65];
        signature[0..33].copy_from_slice(&a_bytes);
        signature[33..65].copy_from_slice(&z_bytes);

        Ok(signature)
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

        Ok(secp
            .verify_schnorr(&schnorr_sig, &message, &public_key.x_only_public_key().0)
            .is_ok())
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
        let secp = Secp256k1::new();
        let private_key = SecretKey::from_slice(bytes)?;
        let keypair = SecpKeyPair::from_secret_key(&secp, &private_key);

        Ok(Self { keypair })
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
