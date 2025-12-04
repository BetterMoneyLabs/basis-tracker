# Comprehensive Testing Plan: Reserve Creation, Note Signing, and Redemption Workflow

## Overview
This testing plan outlines a complete workflow for users to create reserves, sign notes against them, and redeem signed notes against the reserve using the Basis Tracker system.

## Prerequisites
- Access to Ergo node (mainnet/testnet)
- Wallet with ERG balance for creating reserves
- Basis Tracker server running
- Schnorr signature tools or library for note signing
- Two public-private key pairs (one for issuer, one for recipient)

## Phase 1: Reserve Creation

### Step 1: Generate NFT for Reserve
1. Create an NFT to associate with your reserve box
2. Record the NFT ID (token ID) - this will be used in the reserve creation

### Step 2: Prepare Reserve Creation Request
1. Prepare a POST request to `<tracker-server>/reserves/create`
2. Request body format:
```json
{
  "nft_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
  "owner_pubkey": "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12", 
  "erg_amount": 100000000000 
}
```

### Step 3: Execute Reserve Transaction
1. Execute the `/wallet/payment/send` API call with the JSON response from Step 2
2. Verify that the transaction is successful
3. Note the reserve box ID for testing

### Step 4: Verify Reserve Creation
1. Query the tracker server: `GET /reserves`
2. Verify the reserve appears with the correct owner and amount
3. Check collateralization status with `GET /key-status/{owner_pubkey}`

## Phase 2: Note Creation and Signing

### Step 1: Prepare Note Data
1. Create note parameters:
   - `recipient_pubkey`: Recipient's 33-byte compressed public key
   - `amount`: Amount of debt obligation (in nanoERG)
   - `timestamp`: Current Unix timestamp

### Step 2: Sign the Note
1. Create the signing message by concatenating:
   - Recipient public key (33 bytes)
   - Amount as big-endian 8-byte value
   - Timestamp as big-endian 8-byte value
2. Use Schnorr signature algorithm with the issuer's private key
3. Record the 65-byte signature (33-byte a value + 32-byte z value)

### Step 3: Submit Note to Tracker
1. Create request to `POST /notes`:
```json
{
  "recipient_pubkey": "03a1b2c3d4e5f6...",
  "amount": 5000000000,
  "timestamp": 1678886400,
  "signature": "12345678...",
  "issuer_pubkey": "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12"
}
```

### Step 4: Verify Note Creation
1. Verify the note appears in tracker: `GET /notes/issuer/{issuer_pubkey}`
2. Check that key status reflects the new debt: `GET /key-status/{issuer_pubkey}`
3. Confirm collateralization ratio is adequate

## Phase 3: Redemption Process

### Step 1: Initiate Redemption
1. Prepare redemption request to `POST /redeem`:
```json
{
  "issuer_pubkey": "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12",
  "recipient_pubkey": "03a1b2c3d4e5f6...",
  "amount": 2500000000,
  "timestamp": 1678972800
}
```

### Step 2: Verify Redemption Preparation
1. Check that a redemption ID is returned
2. Verify that the redemption appears in the tracker logs
3. Confirm proof is available if needed

### Step 3: Complete Redemption
1. Submit redemption completion request to `POST /redeem/complete`:
```json
{
  "redemption_id": "redemption-abc123",
  "issuer_pubkey": "03e8c3e4877e2f7b79e0e407421a81a1619ea64e37e5e4e77454d1e361e6f80b12",
  "recipient_pubkey": "03a1b2c3d4e5f6...",
  "redeemed_amount": 2500000000
}
```

### Step 4: Verify Redemption Results
1. Check that the note's `amount_redeemed` has increased
2. Verify that the issuer's debt is reduced accordingly
3. Ensure the collateralization ratio is updated

## Phase 4: End-to-End Validation Tests

### Test 1: Full Reserve-Note-Redemption Flow
1. Create a reserve with sufficient collateral
2. Create multiple signed notes against the reserve
3. Verify all notes appear in the tracker
4. Redeem some of the notes
5. Verify the redemption changes are reflected

### Test 2: Collateralization Validation
1. Create a reserve with minimum required collateral
2. Sign notes up to the collateralization limit
3. Attempt to create a note that exceeds the limit
4. Verify that the system rejects the over-collateralized note

### Test 3: Invalid Signature Handling
1. Create a note with an invalid signature
2. Submit to the tracker
3. Verify the system rejects the note with invalid signature error

### Test 4: Reserve Top-Up Scenario
1. Create a reserve with limited collateral
2. Submit notes that approach the limit
3. Top up the reserve with additional collateral
4. Submit additional notes and verify they are accepted

## Phase 5: Automated Testing Scripts

### Script 1: Basic Workflow Test
```bash
# 1. Create reserve and get payload
curl -X POST "$TRACKER_SERVER/reserves/create" \
  -H "Content-Type: application/json" \
  -d '{"nft_id": "$NFT_ID", "owner_pubkey": "$OWNER_PUBKEY", "erg_amount": $AMOUNT}'

# 2. Execute transaction with Ergo node
# (Use response from step 1 as payload to /wallet/payment/send)

# 3. Create and sign a note
# (Sign with issuer private key)

# 4. Submit note to tracker
curl -X POST "$TRACKER_SERVER/notes" \
  -H "Content-Type: application/json" \
  -d '{"recipient_pubkey": "...", "amount": "...", "timestamp": "...", "signature": "...", "issuer_pubkey": "..."}'

# 5. Initiate and complete redemption
curl -X POST "$TRACKER_SERVER/redeem" \
  -H "Content-Type: application/json" \
  -d '{"issuer_pubkey": "...", "recipient_pubkey": "...", "amount": "...", "timestamp": "..."}'
```

### Script 2: Stress Testing
- Create multiple reserves in parallel
- Submit multiple notes simultaneously
- Execute multiple redemptions concurrently
- Verify data consistency under load

### Script 3: Edge Case Testing
- Notes with future timestamps
- Notes with past timestamps beyond acceptable window
- Zero-value notes
- Maximum value notes
- Invalid public key formats

## Phase 6: Monitoring and Validation Points

### Tracker Server Logs
- Monitor for successful note creation events
- Track redemption initiation and completion
- Watch for any validation failures

### Blockchain Verification
- Verify reserve box creation on-chain
- Confirm register values (R4, R5) match expected values
- Validate that ERG amount matches the request

### Balance Verification
- Verify issuer debt amounts in the tracker
- Check recipient note receipts
- Confirm redemption amounts are properly subtracted

## Success Criteria
- All created notes are properly stored in the tracker
- Collateralization ratios are accurately calculated
- Redemption processes reduce outstanding debt appropriately
- No invalid notes are accepted by the system
- All blockchain interactions complete successfully
- Tracker state remains consistent with blockchain state

This testing plan ensures a comprehensive validation of the complete workflow from reserve creation through note redemption while covering various edge cases and stress scenarios.