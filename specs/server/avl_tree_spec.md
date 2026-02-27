# AVL Tree Specification for Basis Tracker

## Overview

This document specifies the implementation of AVL trees for the Basis Tracker system. The tracker uses AVL trees to maintain cryptographic commitments to off-chain credit data, enabling efficient verification of debt amounts during redemption. The AVL tree supports generating cryptographic proofs for individual debt records.

## Design Requirements

1. **Key-based Storage**: Each debt record is identified by `hash(issuer_pubkey || recipient_pubkey)` (32 bytes using Blake2b256)
2. **Cumulative Debt Tracking**: Store cumulative debt amounts (totalDebt) that only increase over time
3. **Cryptographic Proofs**: Support generating Merkle proofs for debt records against the tree root
4. **Efficient Updates**: Maintain O(log n) complexity for insertions and updates
5. **Persistent Storage**: Integrate with the existing storage layer using fjall database
6. **On-chain Commitment**: The AVL tree root digest is committed on-chain in tracker box R5 register

## Data Structure

### Debt Record Structure

The tracker stores cumulative debt records:

```rust
pub struct DebtRecord {
    /// Recipient's public key (33 bytes compressed secp256k1)
    pub recipient_pubkey: PubKey,
    /// Total cumulative debt amount (only increases)
    pub total_debt: u64,
    /// Timestamp of latest payment/update
    pub timestamp: u64,
    /// Signature from issuer (A) on message: hash(A||B) || totalDebt
    pub signature: Signature,
}
```

### Record Key Structure

The database key for each record is computed as:
- `key = blake2b256(issuer_pubkey_bytes || recipient_pubkey_bytes)` = 32 bytes
- This creates a unique identifier based on the debtor-creditor pair

## AVL Tree Integration

### AVL Tree Data Model

The AVL tree stores key-value pairs where:
- **Key**: `blake2b256(issuer_pubkey || recipient_pubkey)` = 32 bytes
- **Value**: `longToByteArray(totalDebt)` = 8 bytes (big-endian encoded)

This minimal storage format is used for the on-chain commitment in the tracker box R5 register.

### AVL Tree Update Algorithm

When a new or updated note is submitted, the following algorithm is executed:

1. **Note Validation**:
   - Perform all existing validation checks:
     - Verify signature authenticity
     - Validate timestamp is not in the future
     - Check for amount overflow conditions
     - Verify sufficient collateralization if required

2. **Storage Update**:
   - Update the persistent note storage using the key hash:
     - Insert or update the note in the note database partition
     - Key format: [issuer_hash][recipient_hash] (64 bytes)
     - Value format: [issuer pubkey][amount collected][amount redeemed][timestamp][signature][recipient pubkey]

3. **AVL Tree Update**:
   - Generate the AVL tree key from issuer and recipient public keys:
     - Calculate Blake2b256 hash of issuer public key
     - Calculate Blake2b256 hash of recipient public key
     - Concatenate the two hashes to form a 64-byte key
   - Serialize the note data for the tree value:
     - Include note fields in a consistent format
   - Insert or update the key-value pair in the AVL tree:
     - If key doesn't exist: Perform INSERT operation
     - If key exists: Perform UPDATE operation (no removal since system doesn't support it)

4. **State Commitment Update**:
   - Update the tracker state with the new AVL tree root digest
   - Update the state commitment timestamp

### AVL Tree Implementation

The AVL tree continues to use the existing `ergo_avltree_rust` crate with the following approach:

```rust
impl TrackerStateManager {
    /// Add a new note to the tracker state
    pub fn add_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        // Store note in persistent storage
        self.storage.store_note(issuer_pubkey, note)?;

        // Update AVL tree state
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);

        // Create value bytes matching persistence format
        let mut value_bytes = Vec::new();
        value_bytes.extend_from_slice(issuer_pubkey);
        value_bytes.extend_from_slice(&note.amount_collected.to_be_bytes());
        value_bytes.extend_from_slice(&note.amount_redeemed.to_be_bytes());
        value_bytes.extend_from_slice(&note.timestamp.to_be_bytes());
        value_bytes.extend_from_slice(&note.signature);
        value_bytes.extend_from_slice(&note.recipient_pubkey);

        // Try to insert, if key already exists then update
        if let Err(_e) = self.avl_state.insert(key.to_bytes(), value_bytes.clone()) {
            // For any error, try to update instead (assuming key already exists)
            self.avl_state
                .update(key.to_bytes(), value_bytes)
                .map_err(|e| NoteError::StorageError(e.to_string()))?;
        }

        self.update_state();
        Ok(())
    }

    /// Update an existing note in the tracker state
    pub fn update_note(&mut self, issuer_pubkey: &PubKey, note: &IouNote) -> Result<(), NoteError> {
        // Store note in persistent storage
        self.storage.store_note(issuer_pubkey, note)?;

        // Update AVL tree state
        let key = NoteKey::from_keys(issuer_pubkey, &note.recipient_pubkey);
        let value_bytes = [
            &note.amount_collected.to_be_bytes()[..],
            &note.amount_redeemed.to_be_bytes()[..],
            &note.timestamp.to_be_bytes()[..],
        ]
        .concat();

        self.avl_state
            .update(key.to_bytes(), value_bytes)
            .map_err(|e| NoteError::StorageError(e.to_string()))?;

        self.update_state();
        Ok(())
    }
}
```

## Persistence Layer Integration

The persistence layer is updated to work with key hashes instead of synthetic IDs:

- Database partition key: 64-byte combination of issuer and recipient public key hashes
- Value format remains the same but without synthetic ID
- All query functions (by issuer, recipient, etc.) work as before using the note structure

## Update Algorithm During Note Submission

The complete algorithm for note submission with AVL tree integration is:

1. **HTTP Request Reception** (same as before)
2. **Input Validation** (same as before)
3. **Note Creation**: Create note without synthetic ID
4. **Command Channel Communication** (same as before)
5. **Tracker Thread Processing**:
   - Store note in persistent storage using key hash as identifier
   - Update AVL tree with the new key-value pair (issuer/recipient -> note data)
   - Since no removal operation is supported, only INSERT or UPDATE operations are performed
6. **Validation and Error Handling** (same as before)
7. **Event Storage** (same as before)
8. **HTTP Response Generation** (same as before)

## Security Considerations

1. **Key-based Identification**: Using issuer+recipient key hashes provides a deterministic and unique identifier for each note
2. **Cryptographic Integrity**: The AVL tree provides cryptographic proof for each note against the root
3. **No Removal**: By not supporting removal operations, we maintain the integrity of historical state commitments
4. **Consistent Identifiers**: Key hashes ensure the same identifier is generated for the same issuer-recipient pair

## Performance Characteristics

1. **Time Complexity**:
   - Insertion: O(log n) where n is the number of notes in the tree
   - Update: O(log n)
   - Proof Generation: O(log n)
   - Lookup: O(log n)

2. **Space Complexity**: O(n) where n is the number of notes

## Initial State and Empty Tree Value

When the tracker system is initialized, the AVL tree starts with an empty state. The serialized representation of the empty AVL tree with insert operations enabled is:

```
644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900012000
```

This 37-byte value has the following structure:
- `64`: AVL tree type identifier
- `4ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900`: 33-byte root digest of empty tree
- `01`: Insert flag enabled (true)
- `20`: Key length (32 bytes)
- `00`: Value length (variable)

This value should be used as the initial value in R5 register of the tracker box.

## Synchronization Requirements

1. **Thread Safety**: All existing synchronization requirements remain the same
2. **AVL Tree Access**: Ensure proper synchronization when multiple threads access the AVL tree
3. **Consistency**: Maintain consistency between the database and AVL tree states