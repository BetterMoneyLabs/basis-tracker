# Redemption Transaction Format Specification

## Overview
This document specifies the format for redemption transactions that spend reserve boxes to pay out to note holders. The unsigned transaction can be signed either by the Ergo node (`/wallet/transaction/sign`) or entirely off-chain by the client, and is then broadcast via `/transactions`. It includes all necessary context extension variables for the Basis reserve contract validation. For the off-chain (client-side) signing path and its serialization requirements, see [offchain_redemption_signing.md](../client/offchain_redemption_signing.md).

## Transaction Request Format

### `/wallet/transaction/sign` Request Structure

The redemption transaction request follows this structure:

```json
{
  "tx": {
    "inputs": [
      {
        "boxId": "String",
        "extension": {
          "0": "ErgoConstant (Byte)",
          "1": "ErgoConstant (GroupElement)",
          "2": "ErgoConstant (Coll[Byte])",
          "3": "ErgoConstant (Long)",
          "5": "ErgoConstant (Coll[Byte])",
          "6": "ErgoConstant (Coll[Byte])",
          "7": "ErgoConstant (Coll[Byte])",
          "8": "ErgoConstant (Coll[Byte])"
        }
      },
      {
        "boxId": "String",
        "extension": {}
      }
    ],
    "dataInputs": [
      { "boxId": "String" }
    ],
    "outputs": [
      { /* recipient output */ },
      { /* updated reserve output */ },
      { /* fee output */ },
      { /* change output */ }
    ]
  },
  "inputsRaw": ["HexString"],
  "dataInputsRaw": ["HexString"],
  "secrets": {
    "dlog": ["recipient_private_key_hex"]
  }
}
```

### Top-Level Fields
- `tx`: The unsigned Ergo transaction
  - `inputs`: Inputs to spend. The first input is the reserve box with its context extension; subsequent inputs are wallet-owned fee boxes with empty extensions.
  - `dataInputs`: Data inputs referenced but not spent (the tracker commitment box).
  - `outputs`: Transaction outputs: recipient redemption, updated reserve, fee, and optional change.
- `inputsRaw`: Array of hex-encoded serialized input box bytes (reserve box + fee inputs)
- `dataInputsRaw`: Array of hex-encoded serialized data input box bytes (tracker box)
- `secrets.dlog`: Array of hex-encoded private keys used by the node to satisfy `proveDlog` constraints (e.g., the recipient's private key)

### Output Fields
- `value`: Amount in nanoERG
- `ergoTree`: Hex-encoded Ergo contract bytes
- `creationHeight`: Current blockchain height
- `assets`: Optional array of tokens (camelCase: `tokenId`, `amount`)
- `additionalRegisters`: Optional register values (e.g., R4, R5, R6 for the reserve output)

## Redemption-Specific Transaction Format

### Redemption Transaction Structure
A redemption transaction has the following structure:

```json
{
  "tx": {
    "inputs": [
      {
        "boxId": "reserve_box_id",
        "extension": {
          "0": "0200",
          "1": "0703receiver_pubkey_hex...",
          "2": "0e4102reserve_owner_sig_hex...",
          "3": "05long_to_vlq(totalDebt)",
          "5": "0e...insert_proof_hex...",
          "6": "0e4102tracker_sig_hex...",
          "8": "0e...tracker_lookup_proof_hex..."
        }
      },
      {
        "boxId": "fee_input_box_id",
        "extension": {}
      }
    ],
    "dataInputs": [
      { "boxId": "tracker_box_id" }
    ],
    "outputs": [
      {
        "value": 500000000,
        "ergoTree": "recipient_p2pk_ergo_tree_hex",
        "creationHeight": 1234567,
        "assets": [],
        "additionalRegisters": {}
      },
      {
        "value": 99900000000,
        "ergoTree": "basis_reserve_contract_ergo_tree_hex",
        "creationHeight": 1234567,
        "assets": [
          { "tokenId": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b", "amount": 1 }
        ],
        "additionalRegisters": {
          "R4": "0703issuer_pubkey_hex...",
          "R5": "hex_encoded_updated_avl_tree_root_digest",
          "R6": "0e20tracker_nft_id_hex"
        }
      },
      {
        "value": 1000000,
        "ergoTree": "standard_fee_contract_ergo_tree_hex",
        "creationHeight": 1234567,
        "assets": [],
        "additionalRegisters": {}
      },
      {
        "value": 490000000,
        "ergoTree": "change_p2pk_ergo_tree_hex",
        "creationHeight": 1234567,
        "assets": [],
        "additionalRegisters": {}
      }
    ]
  },
  "inputsRaw": ["hex_encoded_serialized_reserve_box_bytes", "hex_encoded_serialized_fee_input_bytes"],
  "dataInputsRaw": ["hex_encoded_serialized_tracker_box_bytes"],
  "secrets": {
    "dlog": ["recipient_private_key_hex"]
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
    "0": "0200",
    "1": "0702d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
    "2": "0e4102a7c72ce8ec8fa336a984651d57d30d8d59482ad8be1f72c2bc2d3fd5e4c65be6d9ad5a543b623ff7b4bec075d85cd804d2cf01772674384e75eb4aab1e953fe0",
    "3": "0500000000012a05f200",
    "5": "0e2c0100000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    "6": "0e41031872fa7f83f1545d05a083921e4053f194e87a53facda97677da507a6daf15c348d1fd190990c17c0fe4387d9846bb26b9d8ae821492f3f936124102dc60e5b2",
    "8": "0e2c0100000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
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
    "0": "0200",
    "1": "0702d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
    "2": "0e4102a7c72ce8ec8fa336a984651d57d30d8d59482ad8be1f72c2bc2d3fd5e4c65be6d9ad5a543b623ff7b4bec075d85cd804d2cf01772674384e75eb4aab1e953fe0",
    "3": "0500000000012a05f200",
    "5": "0e2c0100000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    "6": "0e41031872fa7f83f1545d05a083921e4053f194e87a53facda97677da507a6daf15c348d1fd190990c17c0fe4387d9846bb26b9d8ae821492f3f936124102dc60e5b2",
    "7": "0e2c0100000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    "8": "0e2c0100000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
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

### Context Extension Format

Context extension values must be serialized as Ergo constants with type prefixes:

| Type | Prefix | Format | Example |
|------|--------|--------|---------|
| Byte | 0x02 | `02` + 1-byte hex | `0200` (byte value 0) |
| Long | 0x05 | `05` + 16-char hex (8 bytes big-endian) | `0500000000004a817c80` (5 ERG) |
| GroupElement | 0x07 | `07` + 66-char hex (33-byte compressed pubkey) | `0703af13e3...` |
| Coll[Byte] | 0x0e | `0e` + 4-char length + hex data | `0e4102a7c7...` (65 bytes) |

### Context Extension Validation
- **#0 (action)**: Must be `0200` (Byte constant with value 0x00) for redemption
- **#1 (receiver)**: Must be valid GroupElement constant (`07` + 33-byte compressed pubkey hex)
- **#2 (reserveSig)**: Must be Coll[Byte] constant (`0e` + length + 65-byte Schnorr signature hex)
- **#3 (totalDebt)**: Must be Long constant (`05` + 8-byte big-endian hex), must match value in tracker's AVL tree
- **#5 (insertProof)**: Must be Coll[Byte] constant (`0e` + length + AVL proof hex)
- **#6 (trackerSig)**: Must be Coll[Byte] constant (`0e` + length + 65-byte Schnorr signature hex), optional for emergency redemption after 3 days
- **#7 (lookupProofReserve)**: Coll[Byte] constant, required for subsequent redemptions, omitted for first
- **#8 (lookupProofTracker)**: Must be Coll[Byte] constant (`0e` + length + AVL proof hex)

### Signed Message (`bytes_to_sign`) and Extension Ordering

`proveDlog(receiver)` is computed over the transaction's `bytes_to_sign` — the serialization of the unsigned transaction. Each input is serialized as an `UnsignedInput = boxId (32 bytes) ++ ContextExtension`, so **the reserve input's serialized context extension is part of the signed message**. The extension must therefore be serialized in exactly the same byte order the node uses.

The reference client (`sigma.interpreter.ContextExtension`) serializes variables by iterating a `scala.collection.Map[Byte, _]` (`obj.values.foreach { (id, v) => put(id); putValue(v) }`), which is **index (HashMap) order, not insertion order**, and the order depends on the index *set*. A signer that emits variables in a different order produces a different `bytes_to_sign` and the node rejects the proof (`Scripts of all transaction inputs should pass verification … #0 => Success((false, _))`).

For the first-redemption set `{0,1,2,3,4,5,6,8}` the node (Scala) order is `0,5,1,6,2,3,8,4`; for the subsequent-redemption set `{0,1,2,3,4,5,6,7,8}` (with `#7`) it is `0,5,1,6,2,7,3,8,4`. Both are produced by reproducing Scala's `immutable.HashMap` (HashTrieMap) iteration order, validated against the on-chain-confirmed first-redemption order. See [offchain_redemption_signing.md](../client/offchain_redemption_signing.md) for details.

### Output Ordering

The redemption action encodes the reserve output index in context var `#0` (`index = action % 10`, action byte `0x00` ⇒ index `0`). **The updated reserve output must be at output index 0**, followed by the recipient output, the fee output, and the optional change output. (The `requests` examples below list the recipient first for readability; the on-wire `tx.outputs` must place the reserve at index 0.)

### Signature Message Format

**All Redemptions (normal and emergency):**
```
key = blake2b256(ownerKeyBytes || receiverBytes)
message = key || longToByteArray(totalDebt) || longToByteArray(timestamp)
```

- **key**: 32 bytes, `blake2b256(ownerKeyBytes || receiverBytes)`
- **totalDebt**: 8 bytes big-endian
- **timestamp**: 8 bytes big-endian, milliseconds since Unix epoch
- **Total message length**: 48 bytes

**Emergency Redemption:**
- Uses the same 48-byte message format as normal redemption
- Tracker signature bytes must still be provided in context var #6, but verification is bypassed after 3 days (3*720 blocks)
- Reserve owner signature is always required
- The contract checks `enoughTimeSpent` flag to bypass tracker signature verification

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
- Reserve owner's signature on `key || totalDebt || timestamp` (48 bytes, always required)
- Tracker's signature on `key || totalDebt || timestamp` (48 bytes, required in transaction but verification bypassed for emergency redemption after 3 days)
- Signatures must be provided as 65-byte Schnorr signatures (33 bytes 'a' + 32 bytes 'z')
- Signatures are attached via context extension variables #2 and #6

## Emergency Redemption

### Conditions
- Emergency redemption is available after 3 days (3 * 720 blocks) from tracker creation
- All debts associated with the tracker become eligible simultaneously
- Tracker signature bytes must still be provided in context var #6, but verification is bypassed

### Message Format
```
message = key || longToByteArray(totalDebt) || longToByteArray(timestamp)
```

### Transaction Format Changes
- Context var #6 (trackerSig) still required but may be invalid
- Same context extension structure as normal redemption
- Contract checks `enoughTimeSpent` flag to bypass tracker signature verification

This specification provides the complete format for redemption transactions that can be submitted to the Ergo node's `/wallet/transaction/send` endpoint, including all necessary context extension variables for contract validation.
