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
The message to be signed follows the format: `recipient_pubkey || amount_be_bytes || timestamp_be_bytes`

Where:
- `recipient_pubkey`: 33-byte compressed public key of the recipient (hex-encoded)
- `amount_be_bytes`: 8-byte big-endian representation of the amount
- `timestamp_be_bytes`: 8-byte big-endian representation of the Unix timestamp

### Signing Algorithm
1. **Input Validation**:
   - Verify recipient public key is 33 bytes in compressed format
   - Verify amount and timestamp are valid u64 values

2. **Message Construction**:
   - Concatenate recipient public key bytes (33 bytes)
   - Concatenate amount as 8-byte big-endian (8 bytes)
   - Concatenate timestamp as 8-byte big-endian (8 bytes)
   - Total message length: 49 bytes

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
- Recipient pubkey: `02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b` (33 bytes)
- Amount: 1000000000 (0x000000003B9ACA00)
- Timestamp: 1672531200 (0x63B1A800)

Message bytes: `02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b000000003B9ACA000000000063B1A800`

### Expected Signature Format
- Length: 65 bytes (130 hex characters)
- Structure: [1-byte prefix][33-byte A component][32-byte z component]
- Valid prefix: 0x02 or 0x03

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