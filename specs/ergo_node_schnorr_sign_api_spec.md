# Ergo Node API Extension: Schnorr Signature Generation

## Overview

This specification defines a new API endpoint for the Ergo node that allows external applications to request Schnorr signatures for arbitrary messages. The node will sign messages using the private key associated with a given Ergo address if the node has access to that private key.

## New API Endpoint

### Endpoint: `/utils/schnorrSign`
- **Method**: `POST`
- **Path**: `/utils/schnorrSign`
- **Description**: Signs an arbitrary message using the private key associated with a given Ergo address

## Request Format

### Request Body (JSON)
```json
{
  "address": "String",
  "message": "String"
}
```

### Request Fields
- `address`: String - The Ergo address for which to generate the signature (currently only P2PK addresses are supported)
- `message`: String - Hex-encoded message to be signed (arbitrary bytes)

### Example Request
```json
{
  "address": "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
  "message": "02415748f8eef16c5ea6896cec3a8defccc8a0dace245248be66ffd6ff2159da32000000000003d09000000000694fa26d"
}
```

## Response Format

### Success Response (200 OK)
```json
{
  "signedMessage": "String",
  "signature": "String",
  "publicKey": "String"
}
```

### Success Response Fields
- `signedMessage`: String - The original hex-encoded message that was signed
- `signature`: String - 65-byte Schnorr signature in hex format (130 characters)
- `publicKey`: String - The public key corresponding to the private key used for signing (33 bytes in hex, 66 characters)

### Example Success Response
```json
{
  "signedMessage": "02415748f8eef16c5ea6896cec3a8defccc8a0dace245248be66ffd6ff2159da32000000000003d09000000000694fa26d",
  "signature": "02f40cf9d43542868b3e97a790872812574a8be92fd02ce229908d578724c28b925fa689420f9be9f5ddb3d22a6b2a317351008ad38fe222f66aae251f04daae03",
  "publicKey": "02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b"
}
```

### Error Response (400 Bad Request / 500 Internal Server Error)
```json
{
  "error": {
    "code": "String",
    "message": "String"
  }
}
```

### Error Response Fields
- `code`: String - Error code (e.g., "WalletError", "InvalidAddress", "InvalidMessage", "MissingSecretKey", "InvalidAddressType")
- `message`: String - Human-readable error message

### Example Error Response
```json
{
  "error": {
    "code": "MissingSecretKey",
    "message": "Node does not have the secret key for the specified address"
  }
}
```

## Error Conditions

### 400 Bad Request
- Invalid hex encoding in the message field
- Invalid Ergo address format
- Missing required fields
- Invalid address type (only P2PK addresses are currently supported)

### 500 Internal Server Error
- Node does not have access to the private key for the specified address
- Wallet is locked or unavailable
- Internal signing error
- Invalid cryptographic operation

## Signature Format

### Schnorr Signature Structure
- **Total Length**: 65 bytes (130 hex characters)
- **Format**: `[prefix][a_component][z_component]`
  - `prefix`: 1 byte (0x02 or 0x03) - compressed public key format indicator
  - `a_component`: 32 bytes - the 'a' component of the signature (33 bytes total with prefix)
  - `z_component`: 32 bytes - the 'z' component of the signature

### Example Breakdown
For signature `"02f40cf9d43542868b3e97a790872812574a8be92fd02ce229908d578724c28b925fa689420f9be9f5ddb3d22a6b2a317351008ad38fe222f66aae251f04daae03"`:
- `02`: Prefix (compressed public key format)
- `f40cf9d43542868b3e97a790872812574a8be92fd02ce229908d578724c28b92`: 'a' component (32 bytes)
- `5fa689420f9be9f5ddb3d22a6b2a317351008ad38fe222f66aae251f04daae03`: 'z' component (32 bytes)

## Security Considerations

### Authentication
- The endpoint should require the same authentication as other wallet endpoints (API key in header)
- Only allow signing for addresses that are in the node's wallet

### Authorization
- Node must have access to the private key for the specified address
- Wallet must be unlocked if password-protected
- Prevent signing of malicious messages that could compromise funds

### Rate Limiting
- Implement rate limiting to prevent abuse
- Consider per-address rate limits
- Monitor for unusual signing patterns

## Implementation Notes

### Backend Integration
- Integrate with existing wallet infrastructure
- Use the same key derivation and storage mechanisms as existing signing operations
- Leverage existing Schnorr signature implementation in Ergo libraries

### Message Validation
- Validate that message is properly hex-encoded
- Check message length limits (prevent extremely large messages)
- Ensure message format is appropriate for Schnorr signing

### Address Resolution
- Currently only supports P2PK addresses
- Resolve addresses to their underlying public keys
- Verify that the node has the corresponding private key

## Use Cases

### Basis Tracker System
- Allow tracker servers to request signatures for redemption transactions
- Enable secure signature generation without exposing private keys to external services
- Support cross-verification of tracker commitments

### ChainCash Integration
- Enable off-chain signature generation for ChainCash protocols
- Support multi-party signing workflows
- Facilitate secure off-chain transaction creation

### Smart Contract Interactions
- Sign arbitrary messages for smart contract protocols
- Support oracle signature generation
- Enable secure multi-signature schemes

## Compatibility Status

**UPDATE**: The compatibility issue between the Ergo node's Schnorr signature implementation and the Basis tracker's verification algorithm has been **RESOLVED**. The Ergo node now produces signatures that are fully compatible with the Basis server's verification algorithm.

### Verification Results
- **Signature Format**: 65 bytes with correct structure (33-byte 'a' component + 32-byte 'z' component)
- **Challenge Computation**: Uses Blake2b256(a || message || public_key) with correct input ordering
- **Verification Equation**: `g^z == a * x^e` holds true with Ergo node signatures and Basis server verification
- **Message Format**: Follows the correct format: recipient_pubkey (33 bytes) || amount_be_bytes (8 bytes) || timestamp_be_bytes (8 bytes)

### Successful Integration
The Ergo node's `/utils/schnorrSign` API can now be used directly with the Basis server's verification algorithm:
- Signatures generated by the Ergo node pass verification against the Basis server's algorithm
- The verification equation `g^z == a * x^e` is satisfied with node-generated signatures
- All Schnorr signature test vectors pass with both implementations
- Cross-verification between systems is now possible

### Integration Workflow
1. Call Ergo node API: `POST /utils/schnorrSign` with address and message
2. Receive 65-byte signature from node
3. Verify using Basis server's Schnorr verification algorithm
4. Verification succeeds with compatible signature format and mathematical properties

### Signature Components from Successful Verification
Based on successful compatibility testing with the Ergo node at `http://159.89.116.15:11088`:

- **'a' component** (first 33 bytes): Random point in compressed format (starts with 0x02 or 0x03)
- **'z' component** (last 32 bytes): Scalar value
- **Challenge 'e'** (computed via Basis algorithm): `Blake2b256(a_bytes || message_bytes || public_key_bytes)`
- **Verification Equation**: `g^z == a * x^e` holds with node-generated signatures

Example from successful test:
- 'a' component: `039988efd9df3f87c26a96b99c5fd74eccd01b4234a55f696a5b482499606f3e78`
- 'z' component: `37d3db297527a80c122fc868c7430d390403fb4683775c76e11d2308cb87346c`
- Challenge 'e': `109390f8c2f153fca578b11e8bbf961467c00ee480f97014814127d35e24ac03`

## Testing Requirements

### Unit Tests
- Test with valid P2PK addresses
- Test with invalid addresses
- Test with malformed hex messages
- Test with missing private keys
- Test with locked wallets

### Integration Tests
- Test end-to-end signing workflow
- Verify signature validity against public key
- Test error handling for various failure scenarios
- Test rate limiting functionality

## Backwards Compatibility

- This is a new endpoint, so it doesn't affect existing functionality
- Follows the same authentication and error handling patterns as existing endpoints
- Uses the same data formats as other Ergo node API methods

## Performance Considerations

- Signing operations should be fast (typically < 100ms)
- Consider caching public keys for frequently used addresses
- Implement proper async handling for concurrent requests