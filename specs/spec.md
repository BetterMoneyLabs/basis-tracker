Check following things:
"a tracker holds A -> B debt (as positive number), along with ever increasing (on every operation) timestamp." - check that timestamp always increased

"tracker is periodically committing to its state (dictionary) by posting its digest on chain" - implement AVL+ tree with all notes and use its root digest as R5 value in tracker boxes

- Each note is stored in the AVL+ tree with a key derived from issuer and recipient public keys
- The AVL+ tree root digest is updated whenever notes are added, modified or redeemed
- The root digest is stored in R5 register of tracker commitment boxes on chain
- R4 register contains the tracker's public key (33-byte compressed secp256k1 point)
- The tracker periodically submits transactions to update R4 and R5 registers via the Ergo node API
- Tracker will abort with exit code 1 if no Ergo node URL is provided in configuration (no localhost default)
- make redemption with proper update of the tree etc