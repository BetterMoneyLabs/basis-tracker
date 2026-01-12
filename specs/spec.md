Check following things:
"a tracker holds A -> B debt (as positive number), along with ever increasing (on every operation) timestamp." - check that timestamp always increased ✓ COMPLETED

"tracker is periodically committing to its state (dictionary) by posting its digest on chain" - implement AVL+ tree with all notes and use its root digest as R5 value in tracker boxes ✓ COMPLETED

- Each note is stored in the AVL+ tree with a key derived from issuer and recipient public keys ✓ COMPLETED
- The AVL+ tree root digest is updated whenever notes are added, modified or redeemed ✓ COMPLETED
- The root digest is stored in R5 register of tracker commitment boxes on chain ✓ COMPLETED
- R4 register contains the tracker's public key (33-byte compressed secp256k1 point) ✓ COMPLETED
- The tracker periodically submits transactions to update R4 and R5 registers via the Ergo node API ✓ COMPLETED
- Tracker will abort with exit code 1 if no Ergo node URL is provided in configuration (no localhost default) ✓ COMPLETED
- make redemption with proper update of the tree etc ✓ COMPLETED

## Implementation Results

### AVL Tree State Management
- Fixed AVL tree operations to properly generate proofs after each operation (insert, update, remove)
- Ensured AVL tree root digest is updated after each operation through proper proof generation
- Implemented proper initialization of AVL tree with initial proof to ensure non-zero empty tree digest
- Fixed EcPoint creation from compressed public key bytes for R4 register

### Tracker Box Updater
- Implemented periodic submission of tracker box update transactions every 10 minutes
- Fixed error handling for "expected EcPoint, found SigmaProp" when extracting public key from R4 register
- Properly serialize R4 register with tracker public key as GroupElement (EcPoint)
- Properly serialize R5 register with AVL+ tree root digest as SAvlTree (non-zero when tree has content)
- Added comprehensive logging for tracker box update transactions

### Redemption Transaction Builder
- Implemented complete redemption transaction structure with proper validation
- Added validation for redemption parameters (sufficient collateral, time locks, etc.)
- Ensured proper transaction building with inputs, outputs, data inputs, and context extensions
- Added proper Schnorr signature validation (65-byte format)
- Implemented AVL proof inclusion in redemption transactions

### Integration
- Updated shared state management between tracker operations and box updater
- Enhanced error handling and logging throughout the tracker box update process
- Ensured proper synchronization between AVL tree state changes and blockchain commitments
- Connected reserve processing with tracker state updates where appropriate

### New API Endpoints with Real Cryptographic Operations
- Added `/tracker/signature` endpoint for real Schnorr signature generation
- Added `/redemption/prepare` endpoint for complete redemption preparation with real AVL proofs and tracker signatures
- Added `/proof/redemption` endpoint for redemption-specific proof generation
- All endpoints now use real cryptographic functions from `basis_offchain` crate instead of mock implementations
- Proper error handling when tracker private key is not configured (returns 500 instead of mock signatures)
- Real AVL+ tree lookup proofs generated from actual tracker state instead of placeholder strings
- Real 33-byte tracker state digests (1 byte height + 32 bytes hash) retrieved from shared tracker state