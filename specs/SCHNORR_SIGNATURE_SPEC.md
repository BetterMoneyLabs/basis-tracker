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
The message to be signed follows the Basis protocol specification (always 48 bytes):

**Standard Format** (48 bytes):
```
message = key || longToByteArray(totalDebt) || longToByteArray(timestamp)
```

Where:
- `key = blake2b256(ownerKeyBytes || receiverKeyBytes)` (32 bytes)
  - `ownerKeyBytes`: 33-byte compressed public key of the reserve owner (issuer/payer)
  - `receiverKeyBytes`: 33-byte compressed public key of the recipient (creditor/payee)
- `totalDebt`: 8-byte big-endian representation of the total cumulative debt amount
- `timestamp`: 8-byte big-endian representation of the payment timestamp (milliseconds since Unix epoch, Java time format)

**Total message length**: 32 + 8 + 8 = **48 bytes**

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
   - Concatenate: `key || longToByteArray(totalDebt) || longToByteArray(timestamp)`
   - Total message length: 48 bytes (always)

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

6. **Response Calculation**:
   - Compute `z = k + e * s (mod n)` where `s` is the private key
   - This becomes the 'z' component of the signature

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
    
    // Calculate response z = k + e*s (mod n)
    z = (k + e * private_key_scalar) % curve_order_n
    
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

Step 1 - Compute key:
```
key = blake2b256(ownerKeyBytes || receiverKeyBytes)
    = blake2b256(0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 || 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea)
    = 6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4
```

Step 2 - Assemble message (48 bytes):
```
message = key || longToByteArray(totalDebt) || longToByteArray(timestamp)
        = 6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4 || 0000000002faf080 || 00000194f8c88000
```

Message breakdown:
- Key (32 bytes): `6995ccf33c8a09705612e6ee3808bb4cedb48cb7b7c019ecdc68b74e7ed912a4`
- Total debt (8 bytes BE): `0000000002faf080` = 50,000,000 nanoERG
- Timestamp (8 bytes BE): `00000194f8c88000` = 1,743,379,200,000 ms

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
(`scala/scala-utils/SigUtils.scala`) with the `z.bitLength <= 255` constraint.
They are hardcoded in the Rust test suite and verify cross-compatibility between
Scala and Rust implementations.

### TV001 - Standard valid signature
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800`
- **Signature**: `0389ec7df5ff00fcdf83f41ad41ef1813cfd64a87b6c7f219bcd1ecfae9b82a1041af95c9171d4ad63e29513701cdeb5cc9f45798276947c8a8b361dae0f94ab93`
- **Expected verify**: `true`

### TV002 - All-zero signature should fail
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800`
- **Signature**: `0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000`
- **Expected verify**: `false`

### TV003 - Valid tracker signature
- **Issuer pubkey**: `037c3f0429768437a942f1818ef1616c609b7a6d8a8dd245e179c8c0838e7d169d`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `500000000`
- **Timestamp**: `1743379201000`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000001dcd650000000195e97f7be8`
- **Signature**: `024900b6f2a6c83c9158420e7e15bc211e761f5157fe84f2a25499340e731c420624c6b3f14a59b811d50ab0492e53784b541a53688452898924142a313cb64a37`
- **Expected verify**: `true`

### TV004 - Wrong signer signature should fail
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800`
- **Signature**: `03896bab104009190272b8f99808d3d04654f3a882c04aa4119fdffe352e7d496e31f2cc1a52fb60cd3ea7eb5919929584b83f4e9fd7122ea28c9a5ff20090e782`
- **Expected verify**: `false`

### TV005 - Corrupted signature should fail
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800`
- **Signature**: `0224f5a465dc99fe66177dbb503363bcd12a679b260783adc2305dfa996feb5e9564afadb695cf16d8ff1500f557bc0fff7cfb28e418bac449748a09a5ffb7dce3`
- **Expected verify**: `false`

### TV006 - Wrong amount should fail
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0100000195e97f7800`
- **Signature**: `028fd39a0481ab31003d979a8276655c020530038ee18046a441296c4f4b8bbebf38fdbd14ac7fedbfef993d02ef3941dd9fb1f3f287e7bf56a93bf0dd6af67456`
- **Expected verify**: `false`

### TV007 - Wrong timestamp should fail
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7801`
- **Signature**: `03a3d6e4435fb29955452a59d568395b4d46423adbdd46de707c21468dcad159aa6a6a09dff6065b03a54069037c3e37a71186bcc8df20728424d214373c708c12`
- **Expected verify**: `false`

### TV008 - Wrong recipient should fail
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `1000000000`
- **Timestamp**: `1743379200000`
- **Message**: `55df4d11e0afb42e8137dab457fd76f46a00b6abb753c85cdef64493263c9900000000003b9aca0000000195e97f7800`
- **Signature**: `023c0b5e1235b762dc62f27938ada133422ecd4e94ebdfc875cf8af05c30f67a7b751806f0a0d4d92a65be6e5c84de819a45a31720453f8fdb348e2c9ed857226c`
- **Expected verify**: `false`

### TV009 - Maximum u64 values
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `18446744073709551615`
- **Timestamp**: `18446744073709551615`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220bffffffffffffffffffffffffffffffff`
- **Signature**: `03ac2d20f2aceedc94fd621ce5fa0f42926da94d6b673296e24c4a63c7f5178c6f7645dd84cd50f6c5bed74a8aeaacceba442a5008ca0eeb17c8008ae7d3c58dec`
- **Expected verify**: `true`

### TV010 - Zero amount and timestamp
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `0`
- **Timestamp**: `0`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b00000000000000000000000000000000`
- **Signature**: `022d591f919b441f3a3fa671560ef3e7dffa9cf2fb51ed02a7e64e9da203be38905096f698e4c8e49bf4bf03d1f38e4c4554e22df4d334167c0cc59d6747a2501e`
- **Expected verify**: `true`

### TV011 - Emergency redemption valid reserve signature
- **Issuer pubkey**: `0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0`
- **Recipient pubkey**: `02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82`
- **Amount**: `500000000`
- **Timestamp**: `1743379202000`
- **Message**: `07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000001dcd650000000195e97f7fd0`
- **Signature**: `03517ac544f2d87d1ae0731b9c992d7359bfb09b41d18337b9c24dd59b6919b3f26d73531d00d7ba3ae8cf36168a9b9f652eed6cb6a5c7f68c8e9d8fd36641e5a5`
- **Expected verify**: `true`