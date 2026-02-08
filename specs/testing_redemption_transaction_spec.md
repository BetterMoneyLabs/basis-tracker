# Specification for Testing Redemption Transaction Generation

## Overview

This document specifies the process for testing the redemption transaction generation functionality by running the server, polling notes, finding a suitable note with sufficient collateral, preparing redemption data, and generating an unsigned transaction JSON file with proper tracker box retrieval from the node and server APIs.

## Prerequisites

- Rust development environment (cargo, rustc)
- Access to the Ergo node at `http://159.89.116.15:11088` with API key "hello"
- Configuration files for the Basis Tracker server
- Sample IOU notes already created in the system

## Test Participants Keys

For this testing scenario, we will use the following predetermined keys for Alice (issuer) and Bob (recipient):

### Alice (Issuer)
- **Secret Key**: `9864a747e1b97f13e1a3ad0d3fbdc0ff350f51e49191cf47783c5e1fe77dae39`
- **Public Key**: `027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087`

### Bob (Recipient)
- **Secret Key**: `a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2`
- **Public Key**: `03c5424252a1a1e4c2f5d6e7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8`

### Test Scenario
- Notes will be issued from Alice to Bob
- Redemption transactions will involve Alice's reserve box and Bob as the recipient
- All cryptographic operations will use these predetermined keys for consistent testing

## Test Process Flow

### Step 1: Running the Server

1. **Configuration Setup**:
   - Set `ergo.node.node_url` to `http://159.89.116.15:11088` in the configuration file
   - Set `ergo.node.api_key` to `hello` for authentication
   - Verify `ergo.tracker_nft_id` is properly configured
   - Set `ergo.basis_reserve_contract_p2s` to the correct contract address
   - Configure `ergo.tracker_public_key` with the tracker's public key
   - Set appropriate transaction fee in `transaction.fee`

2. **Server Startup**:
   - Run the Basis Tracker server with `cargo run --bin basis_server`
   - Verify the server starts successfully and connects to the Ergo node at `159.89.116.15:11088`
   - Confirm that the tracker scanner is registered and monitoring for tracker boxes
   - Verify that the reserve scanner is monitoring for reserve boxes on the mainnet node
   - Check that the tracker box updater is running (updates every 10 minutes)

3. **Server Health Check**:
   - Verify the server is responding to HTTP requests on the configured port (default 3048)
   - Test basic endpoints like `GET /` to confirm server is operational
   - Confirm that the scanner is actively monitoring the blockchain at `159.89.116.15:11088`
   - Verify connectivity to the Ergo node by checking the `/info` endpoint

### Step 2: Polling the Notes

1. **API Endpoint Access**:
   - Use the `GET /notes` endpoint to retrieve all notes in the system
   - Alternatively, use `GET /notes/issuer/{pubkey}` or `GET /notes/recipient/{pubkey}` for specific notes
   - For notes between specific parties: `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}`
   - Specifically for this test: `GET /notes/issuer/027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087/recipient/03c5424252a1a1e4c2f5d6e7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8` to get notes from Alice to Bob

2. **Polling Mechanism**:
   - Implement a polling loop that queries the notes endpoint at regular intervals (e.g., every 30 seconds)
   - Store the retrieved notes in memory for processing
   - Handle API errors gracefully and retry if needed
   - Log the number of notes retrieved in each poll

3. **Note Processing**:
   - For each note, extract the following information:
     - `issuer_pubkey`: The public key of the note issuer (should match Alice's public key: `027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087`)
     - `recipient_pubkey`: The public key of the note recipient (should match Bob's public key: `03c5424252a1a1e4c2f5d6e7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8`)
     - `amount_collected`: Total amount collected in the note
     - `amount_redeemed`: Amount already redeemed from the note
     - `outstanding_debt`: Calculated as `amount_collected - amount_redeemed`
     - `timestamp`: Creation timestamp of the note
     - `signature`: The note's signature

### Step 3: Finding the First Note with Enough Collateral

1. **Collateral Verification**:
   - For each note from Alice to Bob, retrieve Alice's reserve information using `GET /reserves/issuer/027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087`
   - Connect to the Ergo node at `159.89.116.15:11088` to verify the actual collateral amount in Alice's reserve box
   - Compare the `outstanding_debt` of the note with the available collateral in Alice's reserve
   - Verify that the redemption amount does not exceed the available collateral

2. **Eligibility Criteria**:
   - Note must have a positive outstanding debt
   - Alice must have a reserve box with sufficient collateral (collateral >= redemption amount)
   - The note must meet the time lock requirements (current time >= note timestamp + time lock period)
   - Alice's reserve box must exist and be unspent on the blockchain at `159.89.116.15:11088`
   - Verify Alice's reserve box has the correct tracker NFT token

3. **Selection Process**:
   - Iterate through the notes from Alice to Bob in chronological order (oldest first) or by priority
   - For each note, check if it meets the eligibility criteria
   - Select the first note that has sufficient collateral for redemption
   - Log the selected note's details for verification

### Step 4: Preparing Redemption Data

1. **Redemption Parameters**:
   - Set the redemption amount (typically a portion of the outstanding debt, e.g., 50%)
   - Ensure the redemption amount is positive and does not exceed the outstanding debt
   - Verify that the redemption amount is within the available collateral limits

2. **API Data Preparation**:
   - Prepare the request payload for the `POST /redemption/prepare` endpoint:
     ```json
     {
       "issuer_pubkey": "027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087",
       "recipient_pubkey": "03c5424252a1a1e4c2f5d6e7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8",
       "amount": redemption_amount_in_nanoerg,
       "timestamp": current_unix_timestamp
     }
     ```

3. **Redemption Preparation**:
   - Call the `/redemption/prepare` endpoint to get:
     - `avl_proof`: AVL+ tree lookup proof for the specific note from Alice to Bob
     - `tracker_signature`: 65-byte Schnorr signature from tracker (generated via Ergo node's `/utils/schnorrSign` API at `159.89.116.15:11088`)
     - `tracker_pubkey`: Tracker's public key (hex-encoded, 66 characters)
     - `tracker_state_digest`: 33-byte AVL tree root digest (hex-encoded, 66 characters)
     - `block_height`: Current blockchain height at time of proof generation

### Step 5: Retrieving Tracker Box via Node API

1. **Get Latest Tracker Box ID from Server**:
   - Call `GET /tracker/latest-box-id` on the Basis Tracker server to get the latest tracker box ID
   - Extract the `tracker_box_id` from the response
   - Handle errors if no tracker box is found (this is normal in a fresh system)

2. **Retrieve Tracker Box from Ergo Node**:
   - Use the tracker box ID to query the Ergo node at `159.89.116.15:11088`
   - Call `/utxo/byId/{tracker_box_id}` to get the tracker box details
   - Extract the following information from the tracker box:
     - `box_id`: The tracker box ID
     - `value`: The box value in nanoERG
     - `assets`: The tokens in the box (should include the tracker NFT)
     - `additional_registers.R4`: The tracker public key
     - `additional_registers.R5`: The current AVL tree root digest
     - `creation_height`: The block height when the box was created

3. **Serialize Tracker Box for Transaction**:
   - Retrieve the tracker box from the Ergo node at `159.89.116.15:11088` using the `/utxo/byId/{tracker_box_id}` API endpoint
   - Serialize the tracker box to bytes using the Ergo node's serialization format
   - The serialized bytes will be used in the `dataInputsRaw` field of the transaction
   - The tracker box bytes should be hex-encoded when included in the transaction JSON
   - This ensures the redemption transaction includes the actual serialized tracker box from the blockchain

### Step 6: Retrieving Reserve Box via Node API

1. **Get Reserve Box from Server**:
   - Use Alice's public key to call `GET /reserves/issuer/027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087`
   - Extract Alice's reserve box ID from the response
   - Handle errors if no reserve box is found for Alice

2. **Retrieve Reserve Box from Ergo Node**:
   - Use Alice's reserve box ID to query the Ergo node at `159.89.116.15:11088`
   - Call `/utxo/byId/{reserve_box_id}` to get Alice's reserve box details
   - Extract the following information from Alice's reserve box:
     - `box_id`: The reserve box ID
     - `value`: The box value in nanoERG
     - `assets`: The tokens in the box (should include the tracker NFT)
     - `additional_registers.R4`: Alice's public key (`027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087`)
     - `additional_registers.R5`: The current AVL tree root digest for this reserve
     - `additional_registers.R6`: The tracker NFT ID (bytes) - identifies which tracker server this reserve is linked to
     - `creation_height`: The block height when the box was created

3. **Serialize Reserve Box for Transaction**:
   - Serialize Alice's reserve box to bytes using the Ergo node's serialization format
   - The serialized bytes will be used in the `inputsRaw` field of the transaction
   - Alice's reserve box bytes should be hex-encoded when included in the transaction JSON

### Step 7: Dumping Unsigned Transaction JSON into a File

1. **Transaction Assembly**:
   - Use the redemption preparation data, tracker box data, and Alice's reserve box data to construct the unsigned transaction
   - The transaction should follow the format specified in transaction_assembly_serialization.md:
     ```json
     {
       "requests": [
         {
           "address": "<recipient_address_derived_from_bob_pubkey>",
           "value": <redemption_amount>,
           "assets": [],
           "registers": {}
         },
         {
           "address": "W52Uvz86YC7XkV8GXjM9DDkMLHWqZLyZGRi1FbmyppvPy7cREnehzz21DdYTdrsuw268CxW3gkXE6D5B8748FYGg3JEVW9R6VFJe8ZDknCtiPbh56QUCJo5QDizMfXaKnJ3jbWV72baYPCw85tmiJowR2wd4AjsEuhZP4Ry4QRDcZPvGogGVbdk7ykPAB7KN2guYEhS7RU3xm23iY1YaM5TX1ditsWfxqCBsvq3U6X5EU2Y5KCrSjQxdtGcwoZsdPQhfpqcwHPcYqM5iwK33EU1cHqggeSKYtLMW263f1TY7Lfu3cKMkav1CyomR183TLnCfkRHN3vcX2e9fSaTpAhkb74yo6ZRXttHNP23JUASWs9ejCaguzGumwK3SpPCLBZY6jFMYWqeaanH7XAtTuJA6UCnxvrKko5PX1oSB435Bxd3FbvDAsEmHpUqqtP78B7SKxFNPvJeZuaN7r5p8nDLxUPZBrWwz2vtcgWPMq5RrnoJdrdqrnXMcMEQPF5AKDYuKMKbCRgn3HLvG98JXJ4bCc2wzuZhnCRQaFXTy88knEoj",
           "value": <remaining_collateral_after_redemption_minus_fee>,
           "assets": [
             {
               "tokenId": "<tracker_nft_id_from_alice_reserve>",
               "amount": 1
             }
           ],
           "registers": {
             "R4": "027e5a0a99998fa10474af3a2a704ecc657e4928300e020ac0e422627e8f01a087",
             "R5": "<updated_avl_tree_digest_after_redemption>",
             "R6": "<tracker_nft_id_from_alice_reserve>"
           }
         }
       ],
       "fee": 1000000,
       "inputsRaw": [
         "<hex_encoded_serialized_alice_reserve_box_bytes_from_node_api>"
       ],
       "dataInputsRaw": [
         "<actual_hex_encoded_serialized_tracker_box_bytes_from_node_api>"
       ]
     }
     ```

2. **JSON Generation**:
   - Create the transaction JSON structure with the following components:
     - `requests`: Array of payment requests (redemption payment to Bob and updated reserve for Alice)
     - `fee`: Transaction fee (typically 1,000,000 nanoERG)
     - `inputsRaw`: Array containing the hex-encoded serialized bytes of Alice's reserve box being spent (retrieved from Ergo node API)
     - `dataInputsRaw`: Array containing the hex-encoded serialized bytes of the tracker box (retrieved from Ergo node API)
   - When constructing the updated reserve output, ensure that:
     - R4 register contains Alice's public key (same as original)
     - R5 register contains the updated AVL tree root digest after redemption
     - R6 register contains the tracker NFT ID (preserved from the original reserve box to maintain tracker association)

3. **File Output**:
   - Write the transaction JSON to a file with a descriptive name (e.g., `alice_to_bob_redemption_transaction_{timestamp}.json`)
   - Include metadata about the transaction:
     - Original note details (from Alice to Bob)
     - Redemption amount
     - Timestamp of creation
     - Block height at time of preparation
     - Source Ergo node: `159.89.116.15:11088`
     - Tracker box ID used
     - Alice's reserve box ID used
   - Ensure the file is properly formatted with proper JSON indentation for readability

4. **Verification**:
   - Verify that the file was created successfully
   - Confirm the file contains valid JSON
   - Check that all required fields are present in the transaction structure
   - Validate that the file size is reasonable (not empty or excessively large)

## Expected Outcomes

- Server successfully starts and connects to the Ergo node at `159.89.116.15:11088`
- Notes are successfully polled from the server
- A suitable note with sufficient collateral is identified
- Redemption data is properly prepared with all required cryptographic proofs
- Tracker and reserve boxes are successfully retrieved from the Ergo node API
- Tracker and reserve box bytes are properly serialized for the transaction
- Unsigned transaction JSON is correctly generated and saved to a file
- The transaction file contains all necessary information for eventual signing and submission

## Error Handling

- Handle server startup failures (missing configuration, network issues with `159.89.116.15:11088`)
- Manage API request failures during note polling
- Handle cases where no notes meet the collateral requirements
- Deal with transaction assembly errors
- Manage file I/O errors during JSON output
- Handle Schnorr signature generation failures from the Ergo node at `159.89.116.15:11088`
- Handle tracker box not found errors (gracefully use placeholder if needed)

## Post-Processing

- The generated transaction file can be used for further testing or signing
- The transaction can be submitted to the Ergo node at `159.89.116.15:11088` after proper signing
- The process can be automated for continuous testing scenarios

## Node-Specific Considerations

- The Ergo node at `159.89.116.15:11088` is a mainnet node
- Use the API key "hello" for authentication when required
- Ensure the node is responsive and has the latest blockchain state
- Verify that the tracker NFT ID and reserve contract P2S are valid on this node
- Monitor the node's response times and adjust polling intervals accordingly

- Tracker box bytes are retrieved from the Ergo node API at `159.89.116.15:11088` using the `/utxo/byId/{box_id}` endpoint
- The serialized tracker box bytes are properly hex-encoded and included in the `dataInputsRaw` field of the transaction
- The tracker box ID is obtained from the Basis Tracker server's `/tracker/latest-box-id` endpoint
- Reserve box bytes are retrieved from the Ergo node API at `159.89.116.15:11088` using the `/utxo/byId/{box_id}` endpoint
- The serialized reserve box bytes are properly hex-encoded and included in the `inputsRaw` field of the transaction
- The reserve box ID is obtained from the Basis Tracker server's `/reserves/issuer/{pubkey}` endpoint

This specification provides a complete framework for testing the redemption transaction functionality using the specified Ergo node at `159.89.116.15:11088`, ensuring real-world blockchain integration and validation with proper retrieval of tracker and reserve box bytes from the node API.