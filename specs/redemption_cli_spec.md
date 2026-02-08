# Specification for New Basis CLI Command: Generate Unsigned Ergo Transaction

## Overview

This document specifies a new command for the `basis-cli` tool that generates unsigned Ergo transaction JSON according to the transaction assembly and serialization specification. The command will create a redemption transaction for a given note (identified by issuer and recipient) and amount, properly handling reserve updates and following the Basis contract requirements.

## New API Method for Basis Tracker Server

### Endpoint: `GET /tracker/latest-box-id`

#### Description
Returns the ID of the most recently committed tracker box from the tracker scanner database. This box contains the current state commitment that should be used as a data input for redemption transactions.

#### Request
- Method: `GET`
- Path: `/tracker/latest-box-id`
- Headers: None required
- Query Parameters: None
- Request Body: None

#### Response Format

##### Success Response (200 OK)
```json
{
  "success": true,
  "data": {
    "tracker_box_id": "String",
    "timestamp": "Number",
    "height": "Number"
  },
  "error": null
}
```

##### Success Response Fields
- `success`: Boolean - Always true for successful responses
- `data`: Object - Contains the tracker box information
  - `tracker_box_id`: String - The hex-encoded ID of the most recent tracker box
  - `timestamp`: Number - Unix timestamp when the tracker box was created
  - `height`: Number - Block height when the tracker box was created
- `error`: Null - Always null for successful responses

##### Error Response (404 Not Found)
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "String",
    "message": "String"
  }
}
```

##### Error Response Fields
- `success`: Boolean - Always false for error responses
- `data`: Null - Always null for error responses
- `error`: Object - Contains error information
  - `code`: String - Error code (e.g., "TrackerBoxNotFound")
  - `message`: String - Human-readable error message

#### Error Conditions
- `404 Not Found`: No tracker box has been recorded yet
- `500 Internal Server Error`: Database or internal server error

#### Example Request
```
GET /tracker/latest-box-id
```

#### Example Success Response
```json
{
  "success": true,
  "data": {
    "tracker_box_id": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
    "timestamp": 1709594000,
    "height": 1500
  },
  "error": null
}
```

#### Example Error Response
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "TrackerBoxNotFound",
    "message": "No tracker box has been recorded yet"
  }
}
```

#### Integration with Tracker Scanner

This endpoint should integrate with the existing tracker scanner functionality that monitors tracker commitment boxes. The tracker scanner should:

1. Maintain a record of the most recent tracker box ID in the database
2. Update this record whenever a new tracker box is detected on the blockchain
3. Provide this information through the new API endpoint

#### Security Considerations

- The endpoint is read-only and doesn't expose sensitive information
- No authentication required as the tracker box ID is publicly available on the blockchain
- Rate limiting should be applied to prevent abuse

## New API Method for Basis Tracker Server - Get Reserve Box ID for Issuer

### Endpoint: `GET /reserves/issuer/{pubkey}`

#### Description
Returns the reserve box information for a specific issuer. This endpoint will be used to automatically determine the reserve box ID for redemption transactions based on the issuer's public key.

#### Request
- Method: `GET`
- Path: `/reserves/issuer/{pubkey}`
- Headers: None required
- Query Parameters: None
- Request Body: None
- `{pubkey}`: The hex-encoded public key of the issuer

#### Response Format

##### Success Response (200 OK)
```json
{
  "success": true,
  "data": {
    "reserve_boxes": [
      {
        "box_id": "String",
        "value": "Number",
        "collateral_ratio": "Number",
        "timestamp": "Number"
      }
    ]
  },
  "error": null
}
```

##### Success Response Fields
- `success`: Boolean - Always true for successful responses
- `data`: Object - Contains the reserve box information
  - `reserve_boxes`: Array - List of reserve boxes for the issuer
    - `box_id`: String - The hex-encoded ID of the reserve box
    - `value`: Number - The amount of nanoERG in the reserve box
    - `collateral_ratio`: Number - The current collateralization ratio
    - `timestamp`: Number - Unix timestamp when the reserve was created/updated
- `error`: Null - Always null for successful responses

##### Error Response (404 Not Found)
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "String",
    "message": "String"
  }
}
```

##### Error Response Fields
- `success`: Boolean - Always false for error responses
- `data`: Null - Always null for error responses
- `error`: Object - Contains error information
  - `code`: String - Error code (e.g., "IssuerNotFound")
  - `message`: String - Human-readable error message

#### Error Conditions
- `404 Not Found`: No reserve box exists for the specified issuer
- `500 Internal Server Error`: Database or internal server error

#### Example Request
```
GET /reserves/issuer/02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b
```

#### Example Success Response
```json
{
  "success": true,
  "data": {
    "reserve_boxes": [
      {
        "box_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        "value": 1000000000000,
        "collateral_ratio": 1.5,
        "timestamp": 1709594000
      }
    ]
  },
  "error": null
}
```

#### Example Error Response
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "IssuerNotFound",
    "message": "No reserve box found for the specified issuer"
  }
}
```

#### Security Considerations

- The endpoint is read-only and doesn't expose sensitive information
- No authentication required as reserve box information is publicly available
- Rate limiting should be applied to prevent abuse

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
- `--issuer-pubkey`: Hex-encoded issuer public key (33 bytes)
- `--recipient-pubkey`: Hex-encoded recipient public key (33 bytes)
- `--amount`: Redemption amount in nanoERG
- `--output-file`: Path to output the generated transaction JSON file (optional, defaults to stdout)

### Configuration Requirements
The command will read the tracker server URL from the CLI configuration file:
- Configuration key: `tracker_server.url`
- Default value: `http://localhost:3048`
- The configuration file location follows the standard basis-cli configuration path (typically `~/.basis/cli.toml`)

## Transaction Structure

### Input Validation
Before generating the transaction, the command must validate:
1. All public keys are properly hex-encoded 33-byte compressed secp256k1 points
2. Amount is a positive integer and does not exceed the note's outstanding debt
3. The specified note exists in the system and is valid for redemption
4. Tracker server URL is available in the configuration
5. The issuer has a valid reserve box

### Public Key to Address Conversion
The command will derive the recipient address from the recipient public key using the Ergo node's `/utils/rawToAddress/{pubkeyHex}` API endpoint:
- Input: Hex-encoded compressed public key (33 bytes)
- Output: P2PK address in Base58 format (starting with '9' for mainnet or '3' for testnet)

### Transaction Components

#### 1. Inputs
The transaction will have one input:
- **Reserve Box**: The reserve box identified by retrieving the issuer's reserve box via the `/reserves/issuer/{pubkey}` API endpoint

#### 2. Data Inputs
The transaction will have one data input:
- **Tracker Box**: The tracker box retrieved from the Basis Tracker server via the new `/tracker/latest-box-id` API endpoint

#### 3. Outputs
The transaction will have two outputs:

**Output 1 - Redemption Payment**:
- `address`: The recipient address derived from the recipient public key using the `/utils/rawToAddress/{pubkeyHex}` API
- `value`: The redemption amount specified via `--amount`
- `assets`: Empty array (no tokens transferred to recipient in basic redemption)
- `registers`: Empty object (no special registers needed for recipient)

**Output 2 - Updated Reserve**:
- `address`: The reserve contract P2S address (from configuration: `ergo.basis_reserve_contract_p2s`)
- `value`: Remaining collateral after redemption = original reserve value - redeemed amount - transaction fee
- `assets`: Contains the tracker NFT token to maintain reserve identity
- `registers`:
  - `R4`: The issuer's public key (33-byte compressed format) - same as `--issuer_pubkey`
  - `R5`: The updated AVL tree root digest (32-byte hash + 1-byte height) reflecting the redeemed note
  - `R6`: The NFT ID of the tracker server (bytes) - identifies which tracker server this reserve is linked to

#### 4. Transaction Metadata
- `fee`: Transaction fee (typically 1,000,000 nanoERG = 0.001 ERG)
- `inputsRaw`: Array containing the serialized bytes of the reserve box being spent
- `dataInputsRaw`: Array containing the serialized bytes of the tracker box

## Transaction Generation Process

### Step 1: Load Configuration
1. Read the CLI configuration file to get the tracker server URL
   - Look for `tracker_server.url` in the configuration
   - Use default value `http://localhost:3048` if not specified

### Step 2: Retrieve Note Information
1. Query the Basis Tracker server to get the note details:
   - Call `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}` to retrieve the note
   - Extract note details including amount collected, amount redeemed, and timestamp
   - Verify the note's signature is valid
   - Check that the redemption amount does not exceed the note's outstanding debt

### Step 3: Retrieve Issuer's Reserve Box ID
1. Query the Basis Tracker server to get the issuer's reserve box:
   - Call `GET /reserves/issuer/{issuer_pubkey}` to retrieve the reserve box information
   - Select the appropriate reserve box based on sufficient collateral for the redemption
   - Extract the `box_id` from the response
   - Handle errors if no reserve box is found for the issuer

### Step 4: Retrieve Tracker Box ID
1. Query the Basis Tracker server to get the latest tracker box ID:
   - Call `GET /tracker/latest-box-id` on the tracker server (using URL from config)
   - Extract the `tracker_box_id` from the response
   - Handle errors if no tracker box is found

### Step 5: Public Key to Address Conversion
1. Convert the recipient public key to a P2PK address using the Ergo node API:
   - Call `GET /utils/rawToAddress/{recipient_pubkey}` to get the recipient address
   - Call `GET /utils/rawToAddress/{issuer_pubkey}` to get the issuer address (for the updated reserve output)

### Step 6: Reserve Box Retrieval from Ergo Node
1. Query the Ergo node directly to retrieve the current reserve box details:
   - Use `GET /utxo/byId/{reserve_box_id}` on the Ergo node API at the configured node URL
   - Include API key in header if required: `api_key: <ergo_api_key>`
   - Extract the current value, assets, and register values
   - Verify the reserve has sufficient collateral for the redemption
   - Serialize the box to bytes using the node's serialization format
   - The serialized bytes will be used in the `inputsRaw` field of the transaction

### Step 7: Tracker Box Retrieval from Ergo Node
1. Query the Ergo node directly to retrieve the current tracker box details:
   - Use `GET /utxo/byId/{tracker_box_id}` on the Ergo node API at the configured node URL
   - Include API key in header if required: `api_key: <ergo_api_key>`
   - Extract the current AVL tree root digest from R5 register
   - Extract the tracker public key from R4 register
   - Serialize the tracker box to bytes using the node's serialization format
   - The serialized tracker box bytes will be used in the `dataInputsRaw` field of the transaction

### Step 8: AVL Tree Update and Register Preservation
1. Generate the updated AVL tree state after the redemption:
   - Update the note's `amount_redeemed` field by adding the redemption amount
   - Update the note's timestamp to current time
   - Generate the new AVL tree root digest reflecting the updated note state
2. Preserve existing register values:
   - R6 register value (tracker NFT ID) should be copied from the input reserve box to maintain tracker association

### Step 9: Transaction Assembly
1. Create the transaction structure following the format specified in transaction_assembly_serialization.md:
   ```json
   {
     "requests": [
       {
         "address": "<derived_recipient_address>",
         "value": <redemption_amount>,
         "assets": [],
         "registers": {}
       },
       {
         "address": "<derived_issuer_address>",
         "value": <remaining_collateral>,
         "assets": [
           {
             "tokenId": "<tracker_nft_id>",
             "amount": 1
           }
         ],
         "registers": {
           "R4": "<issuer_pubkey>",
           "R5": "<updated_avl_tree_digest>",
           "R6": "<tracker_nft_id>"
         }
       }
     ],
     "fee": 1000000,
     "inputsRaw": [
       "<hex_encoded_serialized_reserve_box>"
     ],
     "dataInputsRaw": [
       "<hex_encoded_serialized_tracker_box_from_node_api>" // Actual serialized tracker box bytes retrieved from the Ergo node via the /utxo/byId/{box_id} API
     ]
   }
   ```

### Step 10: Output Generation
1. Write the transaction JSON to the specified output file or stdout
2. Include metadata about the transaction:
   - Original note details
   - Redemption amount
   - Expected transaction fee
   - Estimated confirmation time

## Error Handling

### Validation Errors
- `InvalidPublicKey`: If public keys are not properly formatted
- `InsufficientCollateral`: If the reserve doesn't have enough collateral
- `NoteNotFound`: If the specified note doesn't exist
- `InvalidAmount`: If the redemption amount exceeds the note's outstanding debt
- `RedemptionTooEarly`: If the time lock has not expired yet
- `ConfigurationError`: If the tracker server URL is not available in the configuration
- `IssuerHasNoReserve`: If the issuer doesn't have a reserve box

### Network Errors
- `TrackerServerUnavailable`: If the Basis Tracker server is unreachable
- `ErgoNodeUnavailable`: If the Ergo node is unreachable
- `BoxNotFound`: If the specified reserve or tracker box doesn't exist
- `AddressDerivationError`: If the public key to address conversion fails
- `TrackerBoxNotAvailable`: If the tracker box ID cannot be retrieved from the tracker server

### Transaction Assembly Errors
- `InvalidTransactionStructure`: If the transaction doesn't conform to the required format
- `SerializationError`: If there's an error serializing the transaction

## Security Considerations

1. **Private Key Protection**: The command generates unsigned transactions only - private keys are never handled by the CLI
2. **Input Validation**: All inputs are thoroughly validated before transaction generation
3. **State Consistency**: The AVL tree state is updated to reflect the redemption before generating the transaction
4. **Time Lock Verification**: Redemption time locks are enforced to prevent premature redemptions
5. **Address Derivation**: Addresses are derived from public keys using the official Ergo node API to ensure correctness
6. **Tracker Box Verification**: The tracker box ID is retrieved from the trusted tracker server to ensure correct state commitment

## Integration Points

### API Dependencies
- Basis Tracker Server: To retrieve the latest tracker box ID via the new `/tracker/latest-box-id` API
- Basis Tracker Server: To retrieve note details via `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}`
- Basis Tracker Server: To retrieve issuer's reserve box via `GET /reserves/issuer/{pubkey}`
- Ergo Node API: To convert public keys to addresses using `/utils/rawToAddress/{pubkeyHex}`
- Ergo Node API: To retrieve current reserve and tracker box states
- AVL Tree Module: To generate updated state commitments

### Configuration Integration
- The command reads the tracker server URL from the CLI configuration file
- Uses the `tracker_server.url` configuration key
- Falls back to default value if not specified

### Output Format
The generated transaction follows the Ergo node's `/wallet/transaction/send` API format, making it compatible with standard Ergo wallet tools for signing and submission.

This specification provides a complete framework for implementing the new CLI command that generates unsigned redemption transactions according to the Basis protocol requirements, with automatic public key to address derivation, tracker box ID retrieval from the tracker server using the configuration file, and automatic determination of the reserve box from the issuer's public key.