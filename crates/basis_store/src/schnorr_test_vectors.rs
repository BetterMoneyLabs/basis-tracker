//! Cross-validation test vectors for Schnorr signatures
//!
//! These test vectors were generated using the Scala-compatible signing algorithm
//! with bitLength <= 255 constraint, matching the reference implementation in
//! scala/scala-utils/SigUtils.scala

// No imports needed

/// A single Schnorr signature test vector
#[derive(Debug, Clone)]
pub struct SchnorrTestVector {
    pub id: &'static str,
    pub description: &'static str,
    pub issuer_pubkey_hex: &'static str,
    pub recipient_pubkey_hex: &'static str,
    pub amount: u64,
    pub timestamp: u64,
    pub message_hex: &'static str,
    pub signature_hex: &'static str,
    pub should_verify: bool,
}

/// Cross-validation test vectors for Schnorr signature verification
///
/// Generated with deterministic keypairs and Scala-compatible bitLength constraint.
/// All valid signatures (should_verify=true) have z.bitLength <= 255.
pub const SCHNORR_TEST_VECTORS: &[SchnorrTestVector] = &[
    SchnorrTestVector {
        id: "TV001",
        description: "Standard valid signature",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800",
        signature_hex: "0389ec7df5ff00fcdf83f41ad41ef1813cfd64a87b6c7f219bcd1ecfae9b82a1041af95c9171d4ad63e29513701cdeb5cc9f45798276947c8a8b361dae0f94ab93",
        should_verify: true,
    },
    SchnorrTestVector {
        id: "TV002",
        description: "All-zero signature should fail",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800",
        signature_hex: "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV003",
        description: "Valid tracker signature",
        issuer_pubkey_hex: "037c3f0429768437a942f1818ef1616c609b7a6d8a8dd245e179c8c0838e7d169d",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 500000000u64,
        timestamp: 1743379201000u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000001dcd650000000195e97f7be8",
        signature_hex: "024900b6f2a6c83c9158420e7e15bc211e761f5157fe84f2a25499340e731c420624c6b3f14a59b811d50ab0492e53784b541a53688452898924142a313cb64a37",
        should_verify: true,
    },
    SchnorrTestVector {
        id: "TV004",
        description: "Wrong signer signature should fail",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800",
        signature_hex: "03896bab104009190272b8f99808d3d04654f3a882c04aa4119fdffe352e7d496e31f2cc1a52fb60cd3ea7eb5919929584b83f4e9fd7122ea28c9a5ff20090e782",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV005",
        description: "Corrupted signature should fail",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800",
        signature_hex: "0224f5a465dc99fe66177dbb503363bcd12a679b260783adc2305dfa996feb5e9564afadb695cf16d8ff1500f557bc0fff7cfb28e418bac449748a09a5ffb7dce3",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV006",
        description: "Wrong amount should fail",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0100000195e97f7800",
        signature_hex: "028fd39a0481ab31003d979a8276655c020530038ee18046a441296c4f4b8bbebf38fdbd14ac7fedbfef993d02ef3941dd9fb1f3f287e7bf56a93bf0dd6af67456",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV007",
        description: "Wrong timestamp should fail",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7801",
        signature_hex: "03a3d6e4435fb29955452a59d568395b4d46423adbdd46de707c21468dcad159aa6a6a09dff6065b03a54069037c3e37a71186bcc8df20728424d214373c708c12",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV008",
        description: "Wrong recipient should fail",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 1000000000u64,
        timestamp: 1743379200000u64,
        message_hex: "55df4d11e0afb42e8137dab457fd76f46a00b6abb753c85cdef64493263c9900000000003b9aca0000000195e97f7800",
        signature_hex: "023c0b5e1235b762dc62f27938ada133422ecd4e94ebdfc875cf8af05c30f67a7b751806f0a0d4d92a65be6e5c84de819a45a31720453f8fdb348e2c9ed857226c",
        should_verify: false,
    },
    SchnorrTestVector {
        id: "TV009",
        description: "Maximum u64 values",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 18446744073709551615u64,
        timestamp: 18446744073709551615u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220bffffffffffffffffffffffffffffffff",
        signature_hex: "03ac2d20f2aceedc94fd621ce5fa0f42926da94d6b673296e24c4a63c7f5178c6f7645dd84cd50f6c5bed74a8aeaacceba442a5008ca0eeb17c8008ae7d3c58dec",
        should_verify: true,
    },
    SchnorrTestVector {
        id: "TV010",
        description: "Zero amount and timestamp",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 0u64,
        timestamp: 0u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b00000000000000000000000000000000",
        signature_hex: "022d591f919b441f3a3fa671560ef3e7dffa9cf2fb51ed02a7e64e9da203be38905096f698e4c8e49bf4bf03d1f38e4c4554e22df4d334167c0cc59d6747a2501e",
        should_verify: true,
    },
    SchnorrTestVector {
        id: "TV011",
        description: "Emergency redemption valid reserve signature",
        issuer_pubkey_hex: "0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0",
        recipient_pubkey_hex: "02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82",
        amount: 500000000u64,
        timestamp: 1743379202000u64,
        message_hex: "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000001dcd650000000195e97f7fd0",
        signature_hex: "03517ac544f2d87d1ae0731b9c992d7359bfb09b41d18337b9c24dd59b6919b3f26d73531d00d7ba3ae8cf36168a9b9f652eed6cb6a5c7f68c8e9d8fd36641e5a5",
        should_verify: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schnorr;

    #[test]
    fn test_cross_validation_vectors() {
        for vector in SCHNORR_TEST_VECTORS {
            let issuer_pubkey = hex::decode(vector.issuer_pubkey_hex)
                .expect("Invalid issuer pubkey hex")
                .try_into()
                .expect("Issuer pubkey must be 33 bytes");
            let message = hex::decode(vector.message_hex)
                .expect("Invalid message hex");
            let signature = hex::decode(vector.signature_hex)
                .expect("Invalid signature hex")
                .try_into()
                .expect("Signature must be 65 bytes");

            let result = schnorr::schnorr_verify(&signature, &message, &issuer_pubkey
            );
            let verified = result.is_ok();

            assert_eq!(
                verified, vector.should_verify,
                "Test vector {} failed: {} (expected {}, got {})",
                vector.id, vector.description, vector.should_verify, verified
            );
        }
    }
}
