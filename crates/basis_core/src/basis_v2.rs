//! Exact message and state domain for the Basis reserve ABI generation v2.
//!
//! V2 claims are deliberately reserve-bound. A signature created for one
//! reserve singleton, tracker singleton, asset family, owner, or receiver must
//! not be valid in any other domain.

use crate::impls::{schnorr_sign, validate_public_key, SchnorrVerifier};
use crate::traits::{CryptoError, SignatureVerifier};
use crate::types::{PubKey, Signature};
use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;
use thiserror::Error;

/// ABI generation authenticated by both Basis v2 reserve contracts.
pub const BASIS_V2_ABI_GENERATION: u8 = 2;
/// Network byte compiled into the current Basis v2 contract family.
pub const BASIS_V2_ERGO_MAINNET_DOMAIN: u8 = 0;
/// Asset discriminator compiled into the ERG reserve contract.
pub const BASIS_V2_ERG_ASSET_KIND: u8 = 0;
/// Asset discriminator compiled into the token reserve contract.
pub const BASIS_V2_TOKEN_ASSET_KIND: u8 = 1;

/// `"BASIS" || ABI 2 || Ergo mainnet || ERG`.
pub const BASIS_V2_ERG_DOMAIN_TAG: [u8; 8] = *b"BASIS\x02\x00\x00";
/// `"BASIS" || ABI 2 || Ergo mainnet || token`.
pub const BASIS_V2_TOKEN_DOMAIN_TAG: [u8; 8] = *b"BASIS\x02\x00\x01";

/// Maximum non-negative value representable by ErgoScript `Long`.
pub const BASIS_V2_MAX_LONG: u64 = i64::MAX as u64;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BasisV2Error {
    #[error("invalid compressed public key")]
    InvalidPublicKey,
    #[error("reserve NFT and reserve token ids must differ")]
    DuplicateReserveAssetId,
    #[error("total debt must be in 1..=Long.MaxValue")]
    InvalidTotalDebt,
    #[error("timestamp must be in 1..=Long.MaxValue")]
    InvalidTimestamp,
    #[error("redeemed amount exceeds total debt")]
    RedeemedExceedsDebt,
    #[error("redemption amount must be positive")]
    InvalidRedemptionAmount,
    #[error("redemption amount exceeds the remaining claim")]
    RedemptionExceedsClaim,
    #[error("claim update regresses timestamp or cumulative debt")]
    ClaimRegression,
    #[error("state value must be exactly 24 bytes")]
    InvalidStateLength,
    #[error("state value contains a negative ErgoScript Long")]
    NegativeStateValue,
    #[error("invalid Schnorr signature")]
    InvalidSignature,
}

impl From<CryptoError> for BasisV2Error {
    fn from(_: CryptoError) -> Self {
        Self::InvalidSignature
    }
}

/// Collateral family authenticated by a v2 claim key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReserveAssetV2 {
    Erg,
    Token { token_id: [u8; 32] },
}

/// All inputs that define the unique settlement domain of a v2 claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClaimDomainV2 {
    pub reserve_nft_id: [u8; 32],
    pub tracker_nft_id: [u8; 32],
    pub owner_pubkey: PubKey,
    pub receiver_pubkey: PubKey,
    pub asset: ReserveAssetV2,
}

impl ClaimDomainV2 {
    pub fn erg(
        reserve_nft_id: [u8; 32],
        tracker_nft_id: [u8; 32],
        owner_pubkey: PubKey,
        receiver_pubkey: PubKey,
    ) -> Result<Self, BasisV2Error> {
        Self::new(
            reserve_nft_id,
            tracker_nft_id,
            owner_pubkey,
            receiver_pubkey,
            ReserveAssetV2::Erg,
        )
    }

    pub fn token(
        reserve_nft_id: [u8; 32],
        reserve_token_id: [u8; 32],
        tracker_nft_id: [u8; 32],
        owner_pubkey: PubKey,
        receiver_pubkey: PubKey,
    ) -> Result<Self, BasisV2Error> {
        Self::new(
            reserve_nft_id,
            tracker_nft_id,
            owner_pubkey,
            receiver_pubkey,
            ReserveAssetV2::Token {
                token_id: reserve_token_id,
            },
        )
    }

    fn new(
        reserve_nft_id: [u8; 32],
        tracker_nft_id: [u8; 32],
        owner_pubkey: PubKey,
        receiver_pubkey: PubKey,
        asset: ReserveAssetV2,
    ) -> Result<Self, BasisV2Error> {
        validate_public_key(&owner_pubkey).map_err(|_| BasisV2Error::InvalidPublicKey)?;
        validate_public_key(&receiver_pubkey).map_err(|_| BasisV2Error::InvalidPublicKey)?;
        if let ReserveAssetV2::Token { token_id } = asset {
            if token_id == reserve_nft_id {
                return Err(BasisV2Error::DuplicateReserveAssetId);
            }
        }
        Ok(Self {
            reserve_nft_id,
            tracker_nft_id,
            owner_pubkey,
            receiver_pubkey,
            asset,
        })
    }

    /// Exact `blake2b256(...)` key consumed by both v2 ErgoScripts.
    pub fn claim_key(&self) -> [u8; 32] {
        let mut hasher = Blake2b::<U32>::new();
        match self.asset {
            ReserveAssetV2::Erg => {
                hasher.update(BASIS_V2_ERG_DOMAIN_TAG);
                hasher.update(self.reserve_nft_id);
            }
            ReserveAssetV2::Token { token_id } => {
                hasher.update(BASIS_V2_TOKEN_DOMAIN_TAG);
                hasher.update(self.reserve_nft_id);
                hasher.update(token_id);
            }
        }
        hasher.update(self.tracker_nft_id);
        hasher.update(self.owner_pubkey);
        hasher.update(self.receiver_pubkey);
        hasher.finalize().into()
    }

    /// Exact 48-byte message consumed by the debtor and tracker signature checks.
    pub fn signing_message(
        &self,
        total_debt: u64,
        timestamp: u64,
    ) -> Result<[u8; 48], BasisV2Error> {
        validate_claim_values(total_debt, timestamp)?;
        let mut message = [0u8; 48];
        message[..32].copy_from_slice(&self.claim_key());
        message[32..40].copy_from_slice(&total_debt.to_be_bytes());
        message[40..48].copy_from_slice(&timestamp.to_be_bytes());
        Ok(message)
    }
}

/// A debtor-signed cumulative claim in one exact v2 reserve domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimV2 {
    pub domain: ClaimDomainV2,
    pub total_debt: u64,
    pub timestamp: u64,
    pub signature: Signature,
}

impl ClaimV2 {
    pub fn sign(
        domain: ClaimDomainV2,
        total_debt: u64,
        timestamp: u64,
        owner_secret: &[u8; 32],
    ) -> Result<Self, BasisV2Error> {
        let message = domain.signing_message(total_debt, timestamp)?;
        let signature = schnorr_sign(&message, owner_secret, &domain.owner_pubkey)?;
        Ok(Self {
            domain,
            total_debt,
            timestamp,
            signature,
        })
    }

    pub fn signing_message(&self) -> Result<[u8; 48], BasisV2Error> {
        self.domain.signing_message(self.total_debt, self.timestamp)
    }

    pub fn verify(&self) -> Result<(), BasisV2Error> {
        let message = self.signing_message()?;
        SchnorrVerifier
            .verify_signature(&self.signature, &message, &self.domain.owner_pubkey)
            .map_err(BasisV2Error::from)
    }
}

/// Fixed 24-byte value committed by reserve R5 in ABI v2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedeemedStateV2 {
    pub timestamp: u64,
    pub total_debt: u64,
    pub redeemed: u64,
}

impl RedeemedStateV2 {
    pub const ENCODED_LEN: usize = 24;

    pub fn new(timestamp: u64, total_debt: u64, redeemed: u64) -> Result<Self, BasisV2Error> {
        validate_claim_values(total_debt, timestamp)?;
        if redeemed > BASIS_V2_MAX_LONG || redeemed > total_debt {
            return Err(BasisV2Error::RedeemedExceedsDebt);
        }
        Ok(Self {
            timestamp,
            total_debt,
            redeemed,
        })
    }

    pub fn encode(self) -> [u8; Self::ENCODED_LEN] {
        let mut encoded = [0u8; Self::ENCODED_LEN];
        encoded[..8].copy_from_slice(&self.timestamp.to_be_bytes());
        encoded[8..16].copy_from_slice(&self.total_debt.to_be_bytes());
        encoded[16..24].copy_from_slice(&self.redeemed.to_be_bytes());
        encoded
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, BasisV2Error> {
        if encoded.len() != Self::ENCODED_LEN {
            return Err(BasisV2Error::InvalidStateLength);
        }
        let timestamp = decode_non_negative_long(&encoded[..8])?;
        let total_debt = decode_non_negative_long(&encoded[8..16])?;
        let redeemed = decode_non_negative_long(&encoded[16..24])?;
        Self::new(timestamp, total_debt, redeemed)
    }

    /// Apply the same cumulative-claim and redemption rules as the v2 scripts.
    pub fn advance(
        self,
        claim_timestamp: u64,
        claim_total_debt: u64,
        amount: u64,
    ) -> Result<Self, BasisV2Error> {
        validate_claim_values(claim_total_debt, claim_timestamp)?;
        let same_claim = claim_timestamp == self.timestamp && claim_total_debt == self.total_debt;
        let monotone_successor =
            claim_timestamp > self.timestamp && claim_total_debt >= self.total_debt;
        if !same_claim && !monotone_successor {
            return Err(BasisV2Error::ClaimRegression);
        }
        if amount == 0 {
            return Err(BasisV2Error::InvalidRedemptionAmount);
        }
        let available = claim_total_debt
            .checked_sub(self.redeemed)
            .ok_or(BasisV2Error::RedemptionExceedsClaim)?;
        if amount > available {
            return Err(BasisV2Error::RedemptionExceedsClaim);
        }
        let redeemed = self
            .redeemed
            .checked_add(amount)
            .ok_or(BasisV2Error::RedemptionExceedsClaim)?;
        Self::new(claim_timestamp, claim_total_debt, redeemed)
    }
}

fn validate_claim_values(total_debt: u64, timestamp: u64) -> Result<(), BasisV2Error> {
    if total_debt == 0 || total_debt > BASIS_V2_MAX_LONG {
        return Err(BasisV2Error::InvalidTotalDebt);
    }
    if timestamp == 0 || timestamp > BASIS_V2_MAX_LONG {
        return Err(BasisV2Error::InvalidTimestamp);
    }
    Ok(())
}

fn decode_non_negative_long(bytes: &[u8]) -> Result<u64, BasisV2Error> {
    let raw: [u8; 8] = bytes
        .try_into()
        .map_err(|_| BasisV2Error::InvalidStateLength)?;
    let value = i64::from_be_bytes(raw);
    if value < 0 {
        return Err(BasisV2Error::NegativeStateValue);
    }
    Ok(value as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{PublicKey, Secp256k1, SecretKey};

    fn key(secret: u8) -> ([u8; 32], PubKey) {
        let mut bytes = [0u8; 32];
        bytes[31] = secret;
        let secret_key = SecretKey::from_slice(&bytes).unwrap();
        let public_key = PublicKey::from_secret_key(&Secp256k1::new(), &secret_key).serialize();
        (bytes, public_key)
    }

    fn erg_domain() -> ClaimDomainV2 {
        ClaimDomainV2::erg([1u8; 32], [2u8; 32], key(1).1, key(2).1).unwrap()
    }

    #[test]
    fn domain_tags_match_the_contract_bytes() {
        assert_eq!(hex::encode(BASIS_V2_ERG_DOMAIN_TAG), "4241534953020000");
        assert_eq!(hex::encode(BASIS_V2_TOKEN_DOMAIN_TAG), "4241534953020001");
        let base = erg_domain();
        assert_eq!(
            hex::encode(base.claim_key()),
            "656c938601a973fe7dd8b5984b70430bf2c885b69a0d91268b0f5c4383a02d73"
        );
        let token = ClaimDomainV2::token(
            base.reserve_nft_id,
            [5u8; 32],
            base.tracker_nft_id,
            base.owner_pubkey,
            base.receiver_pubkey,
        )
        .unwrap();
        assert_eq!(
            hex::encode(token.claim_key()),
            "db33c7176e8d11041a971258458e6be92b59b56865e06ebfbe8e44e9e007f4a1"
        );
    }

    #[test]
    fn every_domain_coordinate_changes_the_claim_key() {
        let base = erg_domain();
        let base_key = base.claim_key();
        let mutations = [
            ClaimDomainV2::erg(
                [3u8; 32],
                base.tracker_nft_id,
                base.owner_pubkey,
                base.receiver_pubkey,
            )
            .unwrap(),
            ClaimDomainV2::erg(
                base.reserve_nft_id,
                [4u8; 32],
                base.owner_pubkey,
                base.receiver_pubkey,
            )
            .unwrap(),
            ClaimDomainV2::erg(
                base.reserve_nft_id,
                base.tracker_nft_id,
                key(3).1,
                base.receiver_pubkey,
            )
            .unwrap(),
            ClaimDomainV2::erg(
                base.reserve_nft_id,
                base.tracker_nft_id,
                base.owner_pubkey,
                key(4).1,
            )
            .unwrap(),
            ClaimDomainV2::token(
                base.reserve_nft_id,
                [5u8; 32],
                base.tracker_nft_id,
                base.owner_pubkey,
                base.receiver_pubkey,
            )
            .unwrap(),
        ];
        for mutation in mutations {
            assert_ne!(mutation.claim_key(), base_key);
        }
    }

    #[test]
    fn signed_claim_is_bound_to_its_exact_domain() {
        let (owner_secret, owner) = key(1);
        let domain = ClaimDomainV2::erg([1u8; 32], [2u8; 32], owner, key(2).1).unwrap();
        let claim = ClaimV2::sign(domain, 100, 10, &owner_secret).unwrap();
        claim.verify().unwrap();

        let mut wrong = claim.clone();
        wrong.domain.reserve_nft_id = [9u8; 32];
        assert_eq!(wrong.verify(), Err(BasisV2Error::InvalidSignature));
    }

    #[test]
    fn contract_long_boundaries_fail_closed() {
        let domain = erg_domain();
        assert_eq!(
            domain.signing_message(0, 1),
            Err(BasisV2Error::InvalidTotalDebt)
        );
        assert_eq!(
            domain.signing_message(BASIS_V2_MAX_LONG + 1, 1),
            Err(BasisV2Error::InvalidTotalDebt)
        );
        assert_eq!(
            domain.signing_message(1, BASIS_V2_MAX_LONG + 1),
            Err(BasisV2Error::InvalidTimestamp)
        );
    }

    #[test]
    fn redeemed_state_roundtrips_and_advances_monotonically() {
        let state = RedeemedStateV2::new(10, 100, 20).unwrap();
        assert_eq!(RedeemedStateV2::decode(&state.encode()).unwrap(), state);
        assert_eq!(state.advance(10, 100, 30).unwrap().redeemed, 50);
        assert_eq!(state.advance(11, 120, 30).unwrap().redeemed, 50);
        assert_eq!(state.advance(9, 100, 1), Err(BasisV2Error::ClaimRegression));
        assert_eq!(state.advance(11, 99, 1), Err(BasisV2Error::ClaimRegression));
        assert_eq!(
            state.advance(10, 100, 81),
            Err(BasisV2Error::RedemptionExceedsClaim)
        );
    }

    #[test]
    fn redeemed_state_rejects_wrong_shape_and_negative_longs() {
        assert_eq!(
            RedeemedStateV2::decode(&[0u8; 23]),
            Err(BasisV2Error::InvalidStateLength)
        );
        let mut negative = RedeemedStateV2::new(1, 1, 0).unwrap().encode();
        negative[0] = 0x80;
        assert_eq!(
            RedeemedStateV2::decode(&negative),
            Err(BasisV2Error::NegativeStateValue)
        );
    }
}
