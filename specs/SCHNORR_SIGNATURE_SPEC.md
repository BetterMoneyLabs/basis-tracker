# Schnorr Signature Specification for Basis Tracker

## Overview

This specification defines the Schnorr signature algorithm implementation for the Basis Tracker system. It follows the chaincash-rs approach with secp256k1 elliptic curve cryptography and is designed to be compatible with Ergo blockchain requirements.

## Signature Format

### Public Keys
- **Format**: Compressed secp256k1 public keys
- **Size**: 33 bytes total
- **Structure**: 
  - 1-byte prefix (0x02 or 0x03) indicating compressed format
  - 32-byte x-coordinate of the elliptic curve point
- **Encoding**: Hexadecimal representation (66 characters)

### Signatures
- **Format**: 65-byte Schnorr signatures following chaincash-rs format
- **Size**: 65 bytes total (130 hex characters when encoded)
- **Structure**:
  - 1-byte prefix (0x02 or 0x03) - compressed public key format indicator
  - 33-byte 'a' component (32-byte random point + 1-byte prefix)
  - 32-byte 'z' component (response value)
- **Total**: 1 + 33 + 32 = 65 bytes

### Example Signature Breakdown
For signature `"02f40cf9d43542868b3e97a790872812574a8be92fd02ce229908d578724c28b925fa689420f9be9f5ddb3d22a6b2a317351008ad38fe222f66aae251f04daae03"`:
- `02`: Prefix (compressed public key format)
- `f40cf9d43542868b3e97a790872812574a8be92fd02ce229908d578724c28b92`: 'a' component (33 bytes)
- `5fa689420f9be9f5ddb3d22a6b2a317351008ad38fe222f66aae251f04daae03`: 'z' component (32 bytes)

## Signing Process

### Message Format
The message to be signed follows the Basis protocol specification (always 80 bytes):

**Standard Format** (80 bytes):
```
message = key || longToByteArray(totalDebt) || longToByteArray(timestamp) || trackerNftId
```

Where:
- `key = blake2b256(ownerKeyBytes || receiverKeyBytes)` (32 bytes)
  - `ownerKeyBytes`: 33-byte compressed public key of the reserve owner (issuer/payer)
  - `receiverKeyBytes`: 33-byte compressed public key of the recipient (creditor/payee)
- `totalDebt`: 8-byte big-endian representation of the total cumulative debt amount
- `timestamp`: 8-byte big-endian representation of the payment timestamp (milliseconds since Unix epoch, Java time format)
- `trackerNftId`: 32-byte identifier of the tracker's NFT (the tracker NFT id read from the reserve/tracker box, on-chain from `SELF.R6[Coll[Byte]]`). Appending it binds every signature to one specific tracker/contract, so a signature produced for one tracker cannot be replayed on another (issue #15, finding C3)

**Total message length**: 32 + 8 + 8 + 32 = **80 bytes**

**IMPORTANT**:
- Both the reserve owner (payer) and the tracker sign the **exact same message**
- The timestamp is the time of the latest payment from owner to receiver, in milliseconds since Unix epoch
- Emergency redemption uses the **same message format** - the only difference is that the tracker signature becomes optional after the emergency period (2160 blocks from tracker creation)

### Signing Algorithm
1. **Input Validation**:
   - Verify owner public key is 33 bytes in compressed format
   - Verify receiver public key is 33 bytes in compressed format
   - Verify totalDebt is a valid u64 value
   - Verify timestamp is a valid i64 value (milliseconds since Unix epoch)

2. **Message Construction**:
   - Compute key hash: `key = blake2b256(ownerKey || receiverKey)` (32 bytes)
   - Concatenate: `key || longToByteArray(totalDebt) || longToByteArray(timestamp) || trackerNftId`
   - Total message length: 80 bytes (always)

3. **Nonce Generation**:
   - Generate a cryptographically secure random nonce `k` (scalar value)
   - Ensure `k` is within the secp256k1 field range

4. **Random Point Calculation**:
   - Compute `R = k * G` where `G` is the secp256k1 generator point
   - Convert `R` to compressed format (33 bytes with 0x02/0x03 prefix)
   - This becomes the 'a' component of the signature

5. **Challenge Computation**:
   - Compute `e = H(R || message || public_key)` using Blake2b256
   - Reduce `e` modulo the secp256k1 order `n` to get scalar
   - **ErgoScript Compatibility Constraint**: The 32-byte big-endian representation of `e` MUST have its most-significant byte < 0x80 (i.e. the highest bit must be 0). ErgoScript's `byteArrayToBigInt` treats 32-byte values as signed two's-complement integers, so if the top byte is >= 0x80, `e` would be interpreted as a negative number and signature verification would fail on-chain. If this constraint is not met, the signer MUST generate a new nonce and retry.

6. **Response Calculation**:
   - Compute `z = k + e * s (mod n)` where `s` is the private key
   - This becomes the 'z' component of the signature
   - **ErgoScript Compatibility Constraint**: The 32-byte big-endian representation of `z` MUST have its most-significant byte < 0x80 (equivalently `z.bitLength <= 255`). If this constraint is not met, the signer MUST generate a new nonce and retry.

7. **Signature Assembly**:
   - Combine prefix (from compressed R), 'a' component (R), and 'z' component
   - Total signature: 1 + 33 + 32 = 65 bytes

### Reference Implementation (Pseudocode)
```
function schnorr_sign(message_bytes, private_key_scalar, public_key_bytes):
    // Generate random nonce
    k = random_scalar()
    
    // Calculate random point R = k*G
    R_point = multiply_generator(k)
    R_compressed = compress_point(R_point)  // 33 bytes with 0x02/0x03 prefix
    
    // Calculate challenge e = H(R || message || public_key)
    challenge_input = R_compressed || message_bytes || public_key_bytes
    e_full = blake2b256(challenge_input)
    e = reduce_mod_n(e_full)  // Reduce to field range

    // ErgoScript compatibility: e must be a positive signed 256-bit integer.
    // Reject nonces where the top byte of e is >= 0x80 and retry.
    if e_bytes[0] >= 0x80:
        continue  // retry with new nonce

    // Calculate response z = k + e*s (mod n)
    z = (k + e * private_key_scalar) % curve_order_n

    // ErgoScript compatibility: z must also be a positive signed 256-bit integer
    // (bitLength <= 255). Reject and retry if the top byte of z is >= 0x80.
    if z_bytes[0] >= 0x80:
        continue  // retry with new nonce
    
    // Assemble signature: [prefix_byte || R_compressed_without_prefix || z_bytes]
    signature = [R_compressed[0]] || R_compressed[1:] || int_to_bytes(z, 32)
    
    return signature  // 65 bytes total
```

## Verification Process

### Verification Algorithm
1. **Signature Parsing**:
   - Extract prefix byte (0x02 or 0x03)
   - Extract 'a' component (33 bytes - compressed point A)
   - Extract 'z' component (32 bytes - response z)

2. **Input Validation**:
   - Verify signature is exactly 65 bytes
   - Verify prefix is 0x02 or 0x03
   - Verify 'a' component represents a valid point on secp256k1 curve
   - Verify 'z' component is within field range

3. **Challenge Recomputation**:
   - Compute `e = H(A || message || public_key)` using Blake2b256
   - Reduce `e` modulo the secp256k1 order `n`

4. **Verification Equation**:
   - Verify that `g^z = A * x^e` where:
     - `g` is the secp256k1 generator point
     - `z` is the response from signature
     - `A` is the random point from signature
     - `x` is the public key point
     - `e` is the challenge

5. **Alternative Verification**:
   - Compute `R_check = z*G - e*X` where `X` is the public key point
   - Verify that `compress_point(R_check)` equals the 'a' component from signature

### Reference Implementation (Pseudocode)
```
function schnorr_verify(signature, message_bytes, public_key_bytes):
    if len(signature) != 65:
        return false
    
    prefix = signature[0]
    a_component = signature[1:34]  // 33 bytes
    z_component = signature[34:66] // 32 bytes
    
    // Validate prefix
    if prefix != 0x02 and prefix != 0x03:
        return false
    
    // Parse z as scalar
    z = bytes_to_scalar(z_component)
    
    // Parse A (the 'a' component) as a point
    A_bytes = [prefix] + a_component[1:]  // Reconstruct with prefix
    A_point = decompress_point(A_bytes)
    if A_point is invalid:
        return false
    
    // Parse public key
    X_point = decompress_point(public_key_bytes)
    if X_point is invalid:
        return false
    
    // Recompute challenge
    challenge_input = A_bytes || message_bytes || public_key_bytes
    e_full = blake2b256(challenge_input)
    e = reduce_mod_n(e_full)
    
    // Verify g^z = A * x^e by checking if z*G = A + e*X
    left_side = multiply_generator(z)
    right_side = A_point + multiply_point(X_point, e)
    
    return left_side == right_side
```

## Cryptographic Primitives

### Hash Function
- **Algorithm**: Blake2b-256
- **Output**: 32-byte hash
- **Usage**: Challenge computation in Schnorr signature scheme
- **Security**: Collision resistance, preimage resistance

### Elliptic Curve
- **Curve**: secp256k1
- **Field**: Prime field with p = 2^256 - 2^32 - 977
- **Generator**: Standard secp256k1 generator point G
- **Order**: Curve order n ≈ 2^256 - 4.3×10^67

### Field Operations
- **Modular Arithmetic**: Operations modulo the secp256k1 curve order n
- **Scalar Multiplication**: Efficient point multiplication k*P
- **Point Addition**: Elliptic curve point addition

## Security Considerations

### Nonce Security
- Nonces must be cryptographically secure random values
- Never reuse nonces for different messages
- Consider deterministic nonce generation (RFC 6979) to prevent nonce reuse attacks

### Side-Channel Resistance
- Implement constant-time operations where possible
- Protect against timing attacks during scalar multiplication
- Secure handling of private key material

### Validation Requirements
- Always validate public keys are on the correct curve
- Verify signature components are within proper ranges
- Reject signatures with invalid point encodings

## API Integration

### Ergo Node API Endpoint
- **Path**: `/utils/schnorrSign`
- **Method**: POST
- **Content-Type**: application/json
- **Authentication**: API key in header

### Request Format
```json
{
  "address": "String",
  "message": "String"
}
```

### Request Fields
- `address`: String - The Ergo address (P2PK) for which to generate the signature
- `message`: String - Hex-encoded message to be signed (arbitrary bytes)

### Response Format (Success)
```json
{
  "signedMessage": "String",
  "signature": "String",
  "publicKey": "String"
}
```

### Response Fields
- `signedMessage`: String - The original hex-encoded message that was signed
- `signature`: String - 65-byte Schnorr signature in hex format (130 characters)
- `publicKey`: String - The public key corresponding to the private key used for signing (33 bytes in hex, 66 characters)

### Error Response
```json
{
  "error": {
    "code": "String",
    "message": "String"
  }
}
```

## Test Vectors

### Example Message Construction
Given:
- Owner (payer) pubkey: `0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83` (33 bytes)
- Receiver (payee) pubkey: `03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea` (33 bytes)
- Total debt: 50000000 nanoERG (0.05 ERG)
- Timestamp: 1743379200000 ms (Sat Mar 29 2025)
- Tracker NFT id: `4242424242424242424242424242424242424242424242424242424242424242` (32 bytes, example value — `0x42` repeated 32 times)

Step 1 - Compute key:
```
key = blake2b256(ownerKeyBytes || receiverKeyBytes)
    = blake2b256(0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 || 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea)
    = 6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4
```

Step 2 - Assemble message (80 bytes):
```
message = key || longToByteArray(totalDebt) || longToByteArray(timestamp) || trackerNftId
        = 6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4 || 0000000002faf080 || 00000194f8c88000 || 4242424242424242424242424242424242424242424242424242424242424242
```

Message breakdown:
- Key (32 bytes): `6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4`
- Total debt (8 bytes BE): `0000000002faf080` = 50,000,000 nanoERG
- Timestamp (8 bytes BE): `00000194f8c88000` = 1,743,379,200,000 ms
- Tracker NFT id (32 bytes): `4242424242424242424242424242424242424242424242424242424242424242` — binds the signature to this tracker; the contract reads the same id from `SELF.R6[Coll[Byte]]`

### Expected Signature Format
- Length: 65 bytes (130 hex characters)
- Structure: [33-byte a component][32-byte z component]
  - a component: compressed random point R = k*G (starts with 0x02 or 0x03)
  - z component: response scalar (unsigned, 32 bytes)

## Compliance Requirements

### Chaincash-rs Compatibility
- Follow the same signature format as chaincash-rs library
- Maintain compatibility with existing Basis Tracker implementations
- Use the same message construction format

### Ergo Blockchain Compatibility
- Signatures must be verifiable by Ergo's cryptographic primitives
- Public keys must be in compressed format expected by Ergo
- Follow Ergo's Schnorr signature verification procedures

## Implementation Guidelines

### Recommended Libraries
- **secp256k1**: For elliptic curve operations
- **blake2**: For hash function implementation
- **libsodium**: For additional cryptographic primitives (optional)

### Performance Considerations
- Optimize scalar multiplication using precomputed tables
- Consider batch verification for multiple signatures
- Efficient point compression/decompression routines

### Error Handling
- Proper validation of all inputs
- Clear error messages for invalid signatures
- Secure handling of cryptographic failures

## Cross-Validation Test Vectors

The following test vectors were generated using the Scala reference implementation
(`scala/src/main/scala/basis/offchain/SigUtils.scala`) with the `z.bitLength <= 255`
constraint. They are hardcoded in the Rust test suite (`crates/basis_store/src/schnorr_test_vectors.rs`)
and verify cross-compatibility between Scala and Rust implementations.

They were regenerated for the C3 fix (issue #15): every message is now 80 bytes,
`key(32) || totalDebt(8 BE) || timestamp(8 BE) || trackerNftId(32)`, with the tracker
NFT id fixed to `0x42` repeated 32 times for test purposes. Keypairs are derived from
the committed `scala/secrets/participants.csv` fixture (alice = issuer,
bob = recipient, tracker = issuer of TV003).

### TV001 - Standard valid signature
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `027742dac59ea73f7f8710648f17b9fb8ccf570460a727c02b28a6e2f310ebf4711397afdac9e6cc799beedc3309ef2b4989598768f65ea78b95151c844e86408f`
- **Expected verify**: `true`

### TV002 - All-zero signature should fail
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000`
- **Expected verify**: `false`

### TV003 - Signature with a contract-incompatible challenge
- **Issuer pubkey**: `02b651a75c3cec621fa493dfa45fbb42ff77943acdceeb9543163d76e3d81cb645`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `500000000`
- **Timestamp**: `1743379201000`
- **Message**: `52eb0beb0bd0e86206e0a817785aadabbef08730fd5654ea65ebcbe1ef1b3b9e000000001dcd650000000195e97f7be84242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `02d052ebc2a622f61681dc9ad52a8cd062acc4c6acf9ba6cfdd507ae886b8054701319d7378338904e89be9604c16825a06fd831afa598248e3f3902be9d270c39`
- **Expected verify**: `false`

> **This vector was previously labelled "Valid tracker signature" with `Expected verify: true`. That
> was wrong, and the mistake was itself a bug report.**
>
> Its response `z` is fine (top byte `0x13`, `bitLength` well under 255), but the Fiat-Shamir
> challenge is
> `dabdc82a6add16a23e6124ccea24b8e77013909cdc06821245f7a4e5c8a0d8da` — the first byte is `0xda`.
> The contract reads both values with ErgoScript's `byteArrayToBigInt`, which is a **signed**
> big-endian conversion, so `0xda..` becomes a negative number and `pubKey.exp(negative)` throws.
>
> Two implementations disagreed with that:
>
> * `SigUtils.sign` only retried on `z.bitLength > 255` and never checked the challenge, so this
>   reference signer emitted such signatures routinely.
> * The Rust verifier checked only `z` too, so the tracker accepted and pinned a note whose
>   redemption could never execute — the creditor's funds were stuck with no way to recover.
>
> Both now enforce the challenge rule (`SigUtils.sign` retries; `SchnorrVerifier` rejects). The
> vector is kept with `should_verify: false` so the gap cannot silently reopen. See
> `specs/pr14_triage.md` (S5).

### TV004 - Wrong signer signature should fail
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `0240d01f20717b244800f147d2c9b559cd836c19aea344071c1639be2a22870e772bdbbaf89bb715fcc8c4ec7d8df464fb196c83a9616ef0ccd392f5ae8022d072`
- **Expected verify**: `false`

### TV005 - Corrupted signature should fail
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `036eb16f1611ae82edfd2ca30251e6fbbb3eae03173eb16e270e53ce7f755342a16d86d89db441bb8905f86110f3e615ffaf3ceb9d7b1992c0d14d0e3cbf7f3931`
- **Expected verify**: `false`

### TV006 - Wrong amount should fail
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `028c3d344aefd0caa6a16ffdab0bd154b457585cd998e5ac80b86286a796f33f1e53488afbc8e3160caab535bac018b81bd62acea9b64c2a426d08727d42193720`
- **Expected verify**: `false`

### TV007 - Wrong timestamp should fail
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `03baecb66840002c131c54134b0abde112a0405f3d6d1d384b5815857ee68b9e624ba6d7c176c8631633c7e609af34f9e389f99c6e3fd64e16aa743956d66e1e11`
- **Expected verify**: `false`

### TV008 - Wrong recipient should fail
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000003b9aca0000000195e97f78004242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `0282fcc89cb99e3491bb43d1797ff2567fde0620ea38d78f937ae122ae0acf2f897169bcb397037aeb94eda8ee6e5642781f8fe1d85f33bc6a8ed5a440760fd029`
- **Expected verify**: `false`

> TV006–TV008 intentionally reuse TV001's message: each signature was produced over a different
> amount, timestamp, or recipient key, so it must fail when verified against TV001's message.

### TV009 - Maximum u64 values (contract-incompatible challenge)
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `18446744073709551615`
- **Timestamp**: `18446744073709551615`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851fffffffffffffffffffffffffffffffff4242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `03917db7e61b54d50409c0447ebd27caa124d2702aa0ae5651381cd662162307c8055e54abdd0ba40be01c695edf95b9026c94f286a3353f63d1e2348e5d455c2f`
- **Expected verify**: `false`

> Like TV003, this vector carries a Fiat-Shamir challenge whose first byte (`0xc0`) has the
> top bit set: `z` satisfies `bitLength <= 255`, but the contract reads the challenge with
> signed `byteArrayToBigInt`, so `pubKey.exp(e)` would throw on-chain. It is kept with
> `should_verify: false` as regression coverage for that gap (see TV003's SECURITY note and
> `specs/pr14_triage.md` S5).

### TV010 - Zero amount and timestamp
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `0`
- **Timestamp**: `0`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000000000000000000000000000004242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `0235f9989903d1e2ee8c274ba5494119b6deef7cd0d0e15dc07190011b524c891d5deec9c943adce1891dc5977cda361da0a81e81bbf489b3a7f6b8d2c15116ad4`
- **Expected verify**: `true`

### TV011 - Emergency redemption valid reserve signature
- **Issuer pubkey**: `02fc84de78d191f6b47b6ba97bf1eb5ba3b541c8ca247461f41dafdd6db11bc063`
- **Recipient pubkey**: `023bc6f9ff8a2af3992d23c145de5a868bae6b80a3dd3b24a1de4c6486472e9dbd`
- **Amount**: `500000000`
- **Timestamp**: `1743379202000`
- **Message**: `76a4d85b3cac6917e0f0565ebcdc2fbf7656662ca97ef40b9ed1db70c211851f000000001dcd650000000195e97f7fd04242424242424242424242424242424242424242424242424242424242424242`
- **Signature**: `02001d67f568ace3b32c68bd9cfdd3795d5df0f5057b4d837935578697675c8e334668c7b923ae13df8dd9725ac66c63a4ca6f05cc2e7d1c3d23abdcb750dea923`
- **Expected verify**: `true`