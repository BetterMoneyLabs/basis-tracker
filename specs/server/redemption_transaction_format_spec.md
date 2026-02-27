# Redemption Transaction Format Specification

## Overview
This document specifies the format for redemption transactions that spend reserve boxes to pay out to note holders. The transaction follows the Ergo node's `/wallet/transaction/send` API format and includes all necessary context extension variables for the Basis reserve contract validation.

## Transaction Request Format

### Wallet Transaction Request Structure
For the `/wallet/transaction/send` endpoint, the redemption transaction request follows this structure according to the Ergo node API:

```json
{
  "requests": [
    {
      "address": "String",
      "value": "Number",
      "assets": [
        {
          "tokenId": "String",
          "amount": "Number"
        }
      ],
      "registers": {
        "R4": "String",
        "R5": "String",
        "R6": "String"
      }
    }
  ],
  "fee": "Number",
  "inputsRaw": [
    "HexString"
  ],
  "dataInputsRaw": [
    "HexString"
  ],
  "contextExtension": {
    "0": "Number",
    "1": "String",
    "2": "String",
    "3": "Number",
    "5": "String",
    "6": "String",
    "7": "String",
    "8": "String"
  }
}
```

### Top-Level Fields
- `requests`: Array of transaction requests (PaymentRequest objects)
- `fee`: Transaction fee in nanoERG (typically 1000000 for 0.001 ERG)
- `inputsRaw`: Array of hex-encoded serialized input box bytes (boxes to be spent)
- `dataInputsRaw`: Array of hex-encoded serialized data input box bytes (boxes to be referenced without spending)
- `contextExtension`: Map of context extension variables for contract validation

### Payment Request Fields
- `address`: Recipient's Ergo address (required)
- `value`: Amount to send in nanoERG (required)
- `assets`: Optional array of tokens to include
- `registers`: Optional register values to include

## Redemption-Specific Transaction Format

### Redemption Transaction Structure
A redemption transaction typically has the following structure:

```json
{
  "requests": [
    {
      "address": "9RecipientAddressHere...",
      "value": 500000000,
      "assets": [],
      "registers": {}
    },
    {
      "address": "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
      "value": 99900000000,
      "assets": [
        {
          "tokenId": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b",
          "amount": 1
        }
      ],
      "registers": {
        "R4": "02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
        "R5": "hex_encoded_updated_avl_tree_root_digest",
        "R6": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b"
      }
    }
  ],
  "fee": 1000000,
  "inputsRaw": [
    "hex_encoded_serialized_reserve_box_bytes"
  ],
  "dataInputsRaw": [
    "hex_encoded_serialized_tracker_box_bytes"
  ],
  "contextExtension": {
    "0": 0,
    "1": "02receiver_pubkey_bytes...",
    "2": "reserve_owner_signature_bytes...",
    "3": 5000000000,
    "5": "avl_insert_proof_bytes...",
    "6": "tracker_signature_bytes...",
    "7": "avl_lookup_proof_reserve_bytes...",
    "8": "avl_lookup_proof_tracker_bytes..."
  }
}
```

### Redemption Transaction Components

#### 1. Redemption Output (First Request)
- `address`: The recipient's address (the note holder claiming redemption)
- `value`: The amount being redeemed (in nanoERG)
- `assets`: Empty array (no tokens transferred to recipient in basic redemption)
- `registers`: Empty object (no special registers needed for recipient)

#### 2. Updated Reserve Output (Second Request)
- `address`: The issuer's address (where remaining collateral goes)
- `value`: Remaining collateral after redemption (original collateral - redeemed amount - fee)
- `assets`: Contains the tracker NFT token to maintain reserve identity
- `registers`:
  - `R4`: The issuer's public key (33-byte compressed format / GroupElement) - identifies the reserve owner (unchanged from input)
  - `R5`: The **updated** AVL tree root digest after inserting new redeemed amount
    - Stores: `hash(ownerKey || receiverKey) -> cumulativeRedeemedAmount`
    - Must be updated with: `newRedeemed = oldRedeemed + redeemedAmount`
  - `R6`: The NFT ID of the tracker server (bytes) - identifies which tracker server this reserve is linked to (unchanged from input)

#### 3. Data Inputs
- `dataInputsRaw[0]`: Serialized bytes of the tracker commitment box (for state verification)
  - Tracker's R4: Tracker's public key (GroupElement)
  - Tracker's R5: AVL tree commitment to `hash(A||B) -> totalDebt`

#### 4. Context Extension Variables

| ID | Name | Type | Description | Required |
|----|------|------|-------------|----------|
| #0 | action | Byte | Action byte: 0x00 for redemption | Yes |
| #1 | receiver | GroupElement | Receiver's public key | Yes |
| #2 | reserveSig | Coll[Byte] | Reserve owner's Schnorr signature (65 bytes) | Yes |
| #3 | totalDebt | Long | Total cumulative debt amount | Yes |
| #5 | insertProof | Coll[Byte] | AVL proof for inserting into reserve tree | Yes |
| #6 | trackerSig | Coll[Byte] | Tracker's Schnorr signature (65 bytes) | Yes |
| #7 | lookupProofReserve | Coll[Byte] | AVL proof for looking up in reserve tree | No (omit for first redemption) |
| #8 | lookupProofTracker | Coll[Byte] | AVL proof for looking up in tracker tree | Yes |

#### 5. Transaction Metadata
- `fee`: Transaction fee (typically 1000000 nanoERG = 0.001 ERG)
- `inputsRaw`: Serialized bytes of the reserve box being spent
- `dataInputsRaw`: Serialized bytes of the tracker commitment box

## Example Redemption Transaction

### Complete Example (First Redemption)
```json
{
  "requests": [
    {
      "address": "9iJrR3pjgfAp7uVzmY54MSqFh6BEZG8XswWR8qMYj4Mx5e7yv",
      "value": 500000000,
      "assets": [],
      "registers": {}
    },
    {
      "address": "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
      "value": 99900000000,
      "assets": [
        {
          "tokenId": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b",
          "amount": 1
        }
      ],
      "registers": {
        "R4": "02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
        "R5": "b2c3d4e5f6789012345678901234567890123456789012345678901234567890",
        "R6": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b"
      }
    }
  ],
  "fee": 1000000,
  "inputsRaw": [
    "hex_encoded_serialized_reserve_box_bytes"
  ],
  "dataInputsRaw": [
    "hex_encoded_serialized_tracker_box_bytes"
  ],
  "contextExtension": {
    "0": 0,
    "1": "02receiver_pubkey_hex_encoded...",
    "2": "reserve_owner_signature_65_bytes_hex...",
    "3": 5000000000,
    "5": "avl_insert_proof_hex_encoded...",
    "6": "tracker_signature_65_bytes_hex_encoded...",
    "8": "tracker_tree_lookup_proof_hex_encoded..."
  }
}
```

### Complete Example (Subsequent Redemption)
```json
{
  "requests": [
    {
      "address": "9iJrR3pjgfAp7uVzmY54MSqFh6BEZG8XswWR8qMYj4Mx5e7yv",
      "value": 300000000,
      "assets": [],
      "registers": {}
    },
    {
      "address": "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
      "value": 99600000000,
      "assets": [
        {
          "tokenId": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b",
          "amount": 1
        }
      ],
      "registers": {
        "R4": "02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
        "R5": "c3d4e5f678901234567890123456789012345678901234567890123456789012",
        "R6": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b"
      }
    }
  ],
  "fee": 1000000,
  "inputsRaw": [
    "hex_encoded_serialized_reserve_box_bytes"
  ],
  "dataInputsRaw": [
    "hex_encoded_serialized_tracker_box_bytes"
  ],
  "contextExtension": {
    "0": 0,
    "1": "02receiver_pubkey_hex_encoded...",
    "2": "reserve_owner_signature_65_bytes_hex...",
    "3": 5000000000,
    "5": "avl_insert_proof_hex_encoded...",
    "6": "tracker_signature_65_bytes_hex_encoded...",
    "7": "reserve_tree_lookup_proof_hex_encoded...",
    "8": "tracker_tree_lookup_proof_hex_encoded..."
  }
}
```

## Transaction Validation Requirements

### Input Validation
- All input boxes must be unspent at the time of transaction submission
- All data input boxes must exist on the blockchain
- The sum of input values must be >= sum of output values + fee
- All asset IDs and amounts must be valid
- Register values must conform to Ergo's register format

### Contract Compliance
- The transaction must satisfy all spending conditions of the input boxes
- The redemption contract script must validate the redemption proof
- The AVL tree root in R5 must match the proof provided
- The public key in R4 must match the note issuer's public key
- Tracker NFT ID in R6 must match the tracker box's NFT ID

### Context Extension Validation
- **#0 (action)**: Must be 0x00 for redemption
- **#1 (receiver)**: Must be valid GroupElement (33-byte compressed pubkey)
- **#2 (reserveSig)**: Must be valid 65-byte Schnorr signature on `key || totalDebt`
- **#3 (totalDebt)**: Must match value in tracker's AVL tree
- **#5 (insertProof)**: Must be valid AVL proof for inserting updated redeemed amount
- **#6 (trackerSig)**: Must be valid 65-byte Schnorr signature on `key || totalDebt` (or `key || totalDebt || 0L` for emergency)
- **#7 (lookupProofReserve)**: Required for subsequent redemptions, omitted for first
- **#8 (lookupProofTracker)**: Must be valid AVL proof for looking up totalDebt in tracker's tree

### Signature Message Format

**Normal Redemption:**
```
key = blake2b256(ownerKeyBytes || receiverBytes)
message = key || longToByteArray(totalDebt)
```

**Emergency Redemption (after 3 days from tracker creation):**
```
key = blake2b256(ownerKeyBytes || receiverBytes)
message = key || longToByteArray(totalDebt) || longToByteArray(0L)
```

### Security Requirements
- All required signatures must be provided
- Signatures must be valid for the respective public keys
- The transaction must not violate any time locks
- The redemption must be for a valid outstanding note amount
- Redeemed amount must be > 0 and <= (totalDebt - alreadyRedeemed)
- Tracker signature verification is bypassed only after 3 days (emergency)

## Error Handling

### Common Error Scenarios
- `Insufficient Funds`: Input boxes don't have enough value
- `Invalid Proof`: The AVL tree proof doesn't validate against the tracker/reserve state
- `Contract Violation`: Spending conditions not met
- `Double Spend`: Input boxes already spent in another transaction
- `Invalid Signature`: Required signatures are missing or incorrect
- `Tracker Debt Mismatch`: totalDebt doesn't match value in tracker's AVL tree
- `Redemption Exceeds Debt`: Attempting to redeem more than (totalDebt - alreadyRedeemed)
- `Invalid Context Extension`: Missing or malformed context extension variables

### Error Response Format
```json
{
  "error": {
    "code": "String",
    "message": "String",
    "details": "Object"
  }
}
```

## Integration with Redemption Process

### Transaction Building Process
1. Identify the reserve box to be spent (input)
2. Identify the tracker commitment box (data input)
3. Calculate redemption amount (must be <= totalDebt - alreadyRedeemed)
4. Build redemption output to recipient
5. Build updated reserve output with remaining collateral
6. Include tracker NFT in updated reserve output
7. Set R4 register to issuer public key (unchanged)
8. Set R5 register to **updated** AVL tree root (after inserting new redeemed amount)
9. Set R6 register to tracker NFT ID (unchanged)
10. Generate context extension variables:
    - #0: Action byte (0x00)
    - #1: Receiver pubkey
    - #2: Reserve owner's signature
    - #3: Total debt amount
    - #5: AVL insert proof
    - #6: Tracker's signature
    - #7: Reserve lookup proof (if not first redemption)
    - #8: Tracker lookup proof
11. Calculate and include transaction fee
12. Serialize all components in required format

### AVL Tree Operations

#### Reserve Tree Update
```
key = blake2b256(ownerKeyBytes || receiverBytes)
oldRedeemed = reserveTree.get(key, lookupProof) // 0 for first redemption
newRedeemed = oldRedeemed + redeemedAmount
updatedTree = reserveTree.insert((key, longToByteArray(newRedeemed)), insertProof)
```

#### Tracker Tree Verification
```
key = blake2b256(ownerKeyBytes || receiverBytes)
trackerTotalDebt = trackerTree.get(key, lookupProof)
verify: trackerTotalDebt == totalDebt
```

### Signature Requirements
- Reserve owner's signature on `key || totalDebt` (or `key || totalDebt || 0L` for emergency)
- Tracker's signature on `key || totalDebt` (or `key || totalDebt || 0L` for emergency)
- Signatures must be provided as 65-byte Schnorr signatures (33 bytes 'a' + 32 bytes 'z')
- Signatures are attached via context extension variables #2 and #6

## Emergency Redemption

### Conditions
- Emergency redemption is available after 3 days (3 * 720 blocks) from tracker creation
- All debts associated with the tracker become eligible simultaneously
- Tracker signature is still required but verification is bypassed

### Message Format
```
message = key || longToByteArray(totalDebt) || longToByteArray(0L)
```

### Transaction Format Changes
- Context var #6 (trackerSig) still required but may be invalid
- Same context extension structure as normal redemption
- Contract checks `enoughTimeSpent` flag to bypass tracker signature verification

This specification provides the complete format for redemption transactions that can be submitted to the Ergo node's `/wallet/transaction/send` endpoint, including all necessary context extension variables for contract validation.
