# Specification for Basis CLI Redemption Transaction Generation

## Overview

This document specifies the CLI command for generating unsigned Ergo redemption transactions according to the Basis protocol contract requirements. The command generates transactions that spend reserve boxes to pay out to note holders, with proper AVL tree updates and context extension variables.

## CLI Command Definition

### Command Name
`basis-cli transaction generate-redemption`

### Command Syntax
```bash
basis-cli transaction generate-redemption \
  --issuer-pubkey <HEX_ENCODED_PUBKEY> \
  --recipient-pubkey <HEX_ENCODED_PUBKEY> \
  --amount <AMOUNT_IN_NANOERG> \
  --output-file <OUTPUT_JSON_FILE>
```

### Command Options
- `--issuer-pubkey`: Hex-encoded issuer public key (33 bytes compressed secp256k1)
- `--recipient-pubkey`: Hex-encoded recipient public key (33 bytes compressed secp256k1)
- `--amount`: Redemption amount in nanoERG (must be <= totalDebt - alreadyRedeemed)
- `--output-file`: Path to output the generated transaction JSON file (optional, defaults to stdout)
- `--emergency`: Flag to indicate emergency redemption (after 3 days tracker unavailability)

## Transaction Structure

### Input Validation
Before generating the transaction, the command must validate:
1. All public keys are properly hex-encoded 33-byte compressed secp256k1 points
2. Amount is a positive integer
3. The specified note exists in the tracker's AVL tree
4. Redemption amount <= (totalDebt - alreadyRedeemed)
5. Tracker server URL is available in the configuration
6. The issuer has a valid reserve box
7. For emergency redemption: verify 3 days have passed since tracker creation

### Public Key to Address Conversion
The command derives addresses from public keys using the Ergo node's `/utils/rawToAddress/{pubkeyHex}` API endpoint:
- Input: Hex-encoded compressed public key (33 bytes)
- Output: P2PK address in Base58 format (starting with '9' for mainnet or '3' for testnet)

### Transaction Components

#### 1. Inputs
The transaction has one input:
- **Reserve Box**: The reserve box identified by retrieving the issuer's reserve via the `/reserves/issuer/{pubkey}` API endpoint

#### 2. Data Inputs
The transaction has one data input:
- **Tracker Box**: The tracker box retrieved from the Basis Tracker server via the `/tracker/latest-box` API endpoint
  - This box contains the AVL tree commitment to `hash(A||B) -> totalDebt`
  - R4: Tracker's public key (GroupElement)
  - R5: AVL tree root digest

#### 3. Outputs
The transaction has two outputs:

**Output 1 - Redemption Payment**:
- `address`: The recipient address derived from the recipient public key
- `value`: The redemption amount specified via `--amount`
- `assets`: Empty array (no tokens transferred to recipient in basic redemption)
- `registers`: Empty object (no special registers needed for recipient)

**Output 2 - Updated Reserve**:
- `address`: The reserve contract P2S address (from configuration: `ergo.basis_reserve_contract_p2s`)
- `value`: Remaining collateral after redemption = original reserve value - redeemed amount - transaction fee
- `assets`: Contains the tracker NFT token to maintain reserve identity
- `registers`:
  - `R4`: The issuer's public key (GroupElement) - same as input
  - `R5`: The **updated** AVL tree root digest after inserting new redeemed amount
    - Key: `blake2b256(ownerKeyBytes || receiverBytes)`
    - Value: `alreadyRedeemed + redemptionAmount`
  - `R6`: The NFT ID of the tracker server (bytes) - same as input

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

## Transaction Generation Process

### Step 1: Load Configuration
1. Read the CLI configuration file to get the tracker server URL
   - Look for `tracker_server.url` in the configuration
   - Use default value `http://localhost:3048` if not specified

### Step 2: Retrieve Note Information from Tracker
1. Query the Basis Tracker server to get the note details:
   - Call `GET /proof/redemption?issuer_pubkey={issuer}&recipient_pubkey={recipient}`
   - Extract `total_debt` and `already_redeemed` from response
   - Verify the redemption amount <= (total_debt - already_redeemed)
   - Return error if amount exceeds available debt

### Step 3: Retrieve Issuer's Reserve Box
1. Query the Basis Tracker server to get the issuer's reserve box:
   - Call `GET /reserves/issuer/{issuer_pubkey}`
   - Select the appropriate reserve box with sufficient collateral
   - Extract the `box_id` from the response
   - Handle errors if no reserve box is found

### Step 4: Retrieve Tracker Box
1. Query the Basis Tracker server to get the latest tracker box:
   - Call `GET /tracker/latest-box` (returns full box, not just ID)
   - Extract the `box_id`, `box_bytes`, and AVL tree state
   - Handle errors if no tracker box is found

### Step 5: Public Key to Address Conversion
1. Convert public keys to addresses using Ergo node API:
   - Call `GET /utils/rawToAddress/{recipient_pubkey}` for recipient address
   - Call `GET /utils/rawToAddress/{issuer_pubkey}` for issuer address

### Step 6: Reserve Box Retrieval from Ergo Node
1. Query the Ergo node to retrieve the current reserve box details:
   - Use `GET /utxo/byId/{reserve_box_id}` on Ergo node API
   - Include API key in header if required: `api_key: <ergo_api_key>`
   - Extract current value, assets, and register values
   - Verify sufficient collateral for redemption
   - The serialized bytes will be used in `inputsRaw`

### Step 7: Tracker Box Retrieval from Ergo Node
1. Query the Ergo node to retrieve the tracker box details:
   - Use `GET /utxo/byId/{tracker_box_id}` on Ergo node API
   - Extract AVL tree root digest from R5 register
   - Extract tracker public key from R4 register
   - The serialized bytes will be used in `dataInputsRaw`

### Step 8: Request AVL Proofs from Tracker
1. Request AVL proofs from the tracker server:
   - Call `POST /redemption/prepare` with:
     ```json
     {
       "issuer_pubkey": "<issuer_pubkey>",
       "recipient_pubkey": "<recipient_pubkey>",
       "total_debt": <total_debt>
     }
     ```
   - Extract from response:
     - `tracker_lookup_proof` (context var #8)
     - `reserve_lookup_proof` (context var #7, may be null for first redemption)
     - `reserve_insert_proof` (context var #5)

### Step 9: Request Signatures
1. Build signing message:
   - `key = blake2b256(issuer_pubkey_bytes || recipient_pubkey_bytes)`
   - Normal: `message = key || longToByteArray(totalDebt)`
   - Emergency: `message = key || longToByteArray(totalDebt) || longToByteArray(0L)`

2. Request reserve owner's signature:
   - CLI prompts user or uses configured key
   - Sign message with issuer's private key
   - Format: 65-byte Schnorr signature

3. Request tracker's signature:
   - Call `POST /tracker/signature` with:
     ```json
     {
       "issuer_pubkey": "<issuer_pubkey>",
       "recipient_pubkey": "<recipient_pubkey>",
       "total_debt": <total_debt>,
       "emergency": <true/false>
     }
     ```
   - Extract `tracker_signature` from response

### Step 10: Generate Updated AVL Tree
1. Calculate new redeemed amount:
   - `newRedeemed = alreadyRedeemed + redemptionAmount`

2. Generate updated AVL tree:
   - `key = blake2b256(ownerKeyBytes || receiverBytes)`
   - `updatedTree = reserveTree.insert((key, longToByteArray(newRedeemed)), insertProof)`
   - Extract new root digest for R5 register

### Step 11: Transaction Assembly
Assemble the transaction following the Ergo node's `/wallet/transaction/send` format:

```json
{
  "requests": [
    {
      "address": "<recipient_address>",
      "value": <redemption_amount>,
      "assets": [],
      "registers": {}
    },
    {
      "address": "<issuer_address>",
      "value": <remaining_collateral>,
      "assets": [
        {
          "tokenId": "<tracker_nft_id>",
          "amount": 1
        }
      ],
      "registers": {
        "R4": "<issuer_pubkey_hex>",
        "R5": "<updated_avl_tree_root_hex>",
        "R6": "<tracker_nft_id_hex>"
      }
    }
  ],
  "fee": 1000000,
  "inputsRaw": [
    "<hex_encoded_serialized_reserve_box>"
  ],
  "dataInputsRaw": [
    "<hex_encoded_serialized_tracker_box>"
  ],
  "contextExtension": {
    "0": 0,
    "1": "<receiver_pubkey_hex>",
    "2": "<reserve_owner_signature_hex>",
    "3": <total_debt>,
    "5": "<avl_insert_proof_hex>",
    "6": "<tracker_signature_hex>",
    "7": "<reserve_lookup_proof_hex>",
    "8": "<tracker_lookup_proof_hex>"
  }
}
```

Note: Context var #7 should be omitted from the JSON if this is the first redemption (reserve_lookup_proof is null).

### Step 12: Output Generation
1. Write the transaction JSON to the specified output file or stdout
2. Include metadata about the transaction:
   - Original note details (totalDebt, alreadyRedeemed)
   - Redemption amount
   - Expected transaction fee
   - Estimated confirmation time

## Example Transaction

### First Redemption (no context var #7)
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
  "inputsRaw": ["hex_encoded_reserve_box"],
  "dataInputsRaw": ["hex_encoded_tracker_box"],
  "contextExtension": {
    "0": 0,
    "1": "02receiver_pubkey_hex...",
    "2": "reserve_owner_signature_hex...",
    "3": 5000000000,
    "5": "avl_insert_proof_hex...",
    "6": "tracker_signature_hex...",
    "8": "tracker_lookup_proof_hex..."
  }
}
```

### Subsequent Redemption (with context var #7)
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
  "inputsRaw": ["hex_encoded_reserve_box"],
  "dataInputsRaw": ["hex_encoded_tracker_box"],
  "contextExtension": {
    "0": 0,
    "1": "02receiver_pubkey_hex...",
    "2": "reserve_owner_signature_hex...",
    "3": 5000000000,
    "5": "avl_insert_proof_hex...",
    "6": "tracker_signature_hex...",
    "7": "reserve_lookup_proof_hex...",
    "8": "tracker_lookup_proof_hex..."
  }
}
```

## Error Handling

### Validation Errors
- `InvalidPublicKey`: If public keys are not properly formatted
- `InsufficientDebt`: If redemption amount exceeds (totalDebt - alreadyRedeemed)
- `NoteNotFound`: If the note doesn't exist in tracker's AVL tree
- `InvalidAmount`: If the redemption amount is not positive
- `EmergencyRedemptionTooEarly`: If emergency redemption requested before 3 days
- `ConfigurationError`: If the tracker server URL is not available
- `IssuerHasNoReserve`: If the issuer doesn't have a reserve box

### Network Errors
- `TrackerServerUnavailable`: If the Basis Tracker server is unreachable
- `ErgoNodeUnavailable`: If the Ergo node is unreachable
- `BoxNotFound`: If the specified reserve or tracker box doesn't exist
- `AddressDerivationError`: If public key to address conversion fails
- `SignatureGenerationError`: If signature generation fails

### Transaction Assembly Errors
- `InvalidTransactionStructure`: If transaction doesn't conform to required format
- `SerializationError`: If there's an error serializing the transaction
- `AvlProofError`: If AVL proof generation fails

## Security Considerations

1. **Private Key Protection**: The command generates unsigned transactions only - private keys are never handled by the CLI (except optionally for reserve owner signature)
2. **Input Validation**: All inputs are thoroughly validated before transaction generation
3. **AVL Tree Consistency**: The AVL tree state is updated to reflect the redemption
4. **Time Lock Verification**: Emergency redemption time locks are enforced (3 days from tracker creation)
5. **Address Derivation**: Addresses are derived from public keys using the official Ergo node API
6. **Tracker Verification**: totalDebt is verified against tracker's AVL tree commitment
7. **Double Redemption Prevention**: AVL tree design prevents redeeming same debt twice

## Integration Points

### API Dependencies
- Basis Tracker Server: `/proof/redemption`, `/redemption/prepare`, `/tracker/signature`
- Basis Tracker Server: `/reserves/issuer/{pubkey}`, `/tracker/latest-box`
- Ergo Node API: `/utils/rawToAddress/{pubkeyHex}`, `/utxo/byId/{box_id}`
- Ergo Node API: `/utils/schnorrSign` (for tracker signatures via tracker server)

### Configuration Integration
- The command reads the tracker server URL from the CLI configuration file
- Uses the `tracker_server.url` configuration key
- Falls back to default value if not specified

### Output Format
The generated transaction follows the Ergo node's `/wallet/transaction/send` API format, making it compatible with standard Ergo wallet tools for signing and submission.

## Post-Transaction Steps

After generating the unsigned transaction:

1. **Sign the Transaction**:
   - Use Ergo node's `/wallet/transaction/sign` endpoint
   - Or use external signing tools

2. **Submit the Transaction**:
   - Use Ergo node's `/wallet/transaction/send` endpoint
   - Or broadcast via other means

3. **Monitor Confirmation**:
   - Track transaction status on the blockchain
   - Verify reserve box update

This specification provides a complete framework for implementing the CLI command that generates unsigned redemption transactions according to the Basis protocol contract requirements.
