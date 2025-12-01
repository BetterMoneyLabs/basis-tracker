# Note Submission Algorithm

## Overview

This document describes the complete algorithm executed when a new or updated IOU note is submitted to the basis server, from receiving the submission via the HTTP API to final database/storage updates.

## Algorithm Flow

### Step 1: HTTP Request Reception
1. Client sends a POST request to `/notes` endpoint with a JSON payload containing:
   - `recipient_pubkey`: Hex-encoded recipient public key (33 bytes)
   - `amount`: IOU amount in nanoERG
   - `timestamp`: Unix timestamp of the note creation
   - `signature`: Hex-encoded cryptographic signature
   - `issuer_pubkey`: Hex-encoded issuer public key (33 bytes)

### Step 2: Input Validation
1. Validate and decode `recipient_pubkey` from hex to bytes
2. Convert to fixed-size array (33 bytes) or return 400 BAD REQUEST if invalid
3. Validate and decode `signature` from hex to bytes
4. Convert to fixed-size array (65 bytes) or return 400 BAD REQUEST if invalid
5. Validate and decode `issuer_pubkey` from hex to bytes
6. Convert to fixed-size array (33 bytes) or return 400 BAD REQUEST if invalid

### Step 3: IOU Note Creation
1. Create a new `IouNote` struct with:
   - `recipient_pubkey`: Decoded recipient public key
   - `amount_collected`: Amount from request payload
   - `amount_redeemed`: Initialized to 0
   - `timestamp`: Timestamp from request payload
   - `signature`: Decoded signature from request payload

### Step 4: Command Channel Communication
1. Create a oneshot channel for receiving the result from the tracker thread
2. Send an `AddNote` command via the MPSC channel to the tracker thread containing:
   - `issuer_pubkey`: The decoded issuer public key
   - `note`: The newly created IOU note
   - `response_tx`: The sender side of the oneshot channel for result response

### Step 5: Tracker Thread Processing
1. Tracker thread receives the `AddNote` command in its main loop
2. Calls `redemption_manager.tracker.add_note(&issuer_pubkey, &note)` method
3. The `add_note` method performs internal validation and storage in the tracker state:
   - Verifies signature authenticity
   - Checks for valid timestamp (not in the future)
   - Validates amount does not cause overflow
   - Ensures sufficient collateralization if needed
4. Updates the internal state with the new note

### Step 6: Validation and Error Handling
1. If `add_note` returns an error:
   - Error is sent back through the oneshot channel
   - Different error types are mapped to appropriate error messages:
     - `InvalidSignature`: "Invalid signature"
     - `AmountOverflow`: "Amount overflow"
     - `FutureTimestamp`: "Future timestamp"
     - `RedemptionTooEarly`: "Redemption too early"
     - `InsufficientCollateral`: "Insufficient collateral"
     - `StorageError`: "Storage error: [details]"

### Step 7: Event Storage and AVL Tree Update
1. If the note addition is successful (`Ok(())` response):
   - Create a `TrackerEvent` with type `EventType::NoteUpdated`
   - Set event fields:
     - `id`: 0 (will be set by event store)
     - `event_type`: `EventType::NoteUpdated`
     - `timestamp`: Timestamp from the note
     - `issuer_pubkey`: Hex-encoded issuer public key
     - `recipient_pubkey`: Hex-encoded recipient public key
     - `amount`: Note amount
     - Other fields set to `None`
   - Store the event in the event store using `state.event_store.add_event(event).await`
   - Update the AVL+ tree with the new/updated note: add the note to the tree structure using a key derived from issuer and recipient public keys
   - Update the AVL+ tree root digest which will be used for the R5 register in tracker boxes
   - If event storage fails, log a warning but continue

### Step 8: HTTP Response Generation
1. If successful:
   - Return `201 CREATED` status code
   - Return success response with no data: `ApiResponse { success: true, data: Some(()), error: None }`
2. If validation error during input processing:
   - Return `400 BAD REQUEST` status code
   - Return error response with validation message
3. If error from tracker thread:
   - Return `400 BAD REQUEST` status code
   - Return error response with appropriate message from step 6
4. If internal server error (channel failure, etc.):
   - Return `500 INTERNAL SERVER ERROR` status code
   - Return generic error response

## Key Components Involved

1. **API Handler**: `create_note` function in `api.rs`
2. **Tracker Thread**: Background thread that processes commands in `main.rs`
3. **Business Logic**: `add_note` method in `basis_store` crate
4. **Event Store**: In-memory event storage in `store.rs`
5. **Serialization/Deserialization**: Request/response models in `models.rs`

## Data Structures Used

- `CreateNoteRequest`: Incoming JSON request structure
- `IouNote`: Core note data structure from `basis_store`
- `TrackerCommand::AddNote`: Command sent to tracker thread
- `TrackerEvent`: Event stored in the event store for audit trail
- `ApiResponse`: Standard response wrapper

## Error Conditions

- Invalid hex encoding of public keys or signatures
- Incorrect byte lengths (not 33 bytes for public keys, not 65 for signatures)
- Invalid cryptographic signatures
- Amount overflow conditions
- Future timestamps
- Insufficient collateral for the note
- Internal communication failures between threads
- Event store failures (non-fatal - logged as warning)

## Concurrency Model

The algorithm uses a message-passing concurrency model where:
- HTTP request handling occurs on async Tokio threads
- Note storage and validation happens on a blocking thread pool
- Communication between the two uses MPSC and oneshot channels for thread safety
- Event storage is handled asynchronously using async mutexes

## Security Considerations

- All public keys and signatures are validated before processing
- Cryptographic signature verification is performed in the business logic layer
- Channel-based communication prevents race conditions
- Input validation occurs before any state changes