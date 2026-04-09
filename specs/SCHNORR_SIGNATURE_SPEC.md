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