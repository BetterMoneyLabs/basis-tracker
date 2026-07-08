# Specification for Basis CLI Redemption Transaction Generation

## Overview

This document specifies the CLI command for generating unsigned Ergo redemption transactions according to the Basis protocol contract (`basis.es`). The generated transaction is ready for signing via the Ergo node's `/wallet/transaction/sign` endpoint and spends a reserve box to pay a creditor while preserving the remaining collateral in a new reserve box.

## CLI Command Definition

### Command Name
`basis-cli transaction generate-redemption`

### Command Syntax
```bash
basis-cli transaction generate-redemption \
  --issuer-pubkey <HEX_ENCODED_PUBKEY> \
  --recipient-pubkey <HEX_ENCODED_PUBKEY> \
  --amount <AMOUNT_IN_NANOERG> \
  [--output-file <OUTPUT_JSON_FILE>] \
  [--emergency] \
  [--tracker-box-id <TRACKER_BOX_ID>] \
  [--change-address <CHANGE_ADDRESS>]
```

### Command Options
- `--issuer-pubkey`: Hex-encoded issuer public key (33 bytes compressed secp256k1)
- `--recipient-pubkey`: Hex-encoded recipient public key (33 bytes compressed secp256k1)
- `--amount`: Redemption amount in nanoERG (must be <= totalDebt - alreadyRedeemed)
- `--output-file`: Path to output the generated transaction JSON file (optional; defaults to stdout)
- `--emergency`: Emergency redemption flag (after 3 days / 2160 blocks tracker unavailability)
- `--tracker-box-id`: Tracker box ID to use as data input (optional; fetched from server if omitted)
- `--change-address`: Wallet change address for the fee-input change output (optional; defaults to recipient address)

### Required External State
- The CLI must have a current account selected whose public key matches `--issuer-pubkey` (the reserve owner signs the redemption message).
- The recipient address derived from `--recipient-pubkey` must be present in the Ergo node wallet, because the node must satisfy `proveDlog(receiver)` when signing the transaction.
- The local Ergo node is expected at `http://127.0.0.1:9053` with API key `hello`.

## Transaction Structure

### Input Validation
Before generating the transaction, the command validates:
1. Both public keys are 33-byte compressed secp256k1 points (66 hex characters)
2. Amount is positive
3. The note exists in the tracker's state
4. Redemption amount <= (totalDebt - alreadyRedeemed)
5. A reserve box with sufficient collateral exists for the issuer
6. A tracker box exists and is available on-chain
7. The wallet has a fee input with no tokens covering the required fee

### Public Key to Address Conversion
Addresses are derived from public keys using `ergo-lib` (compressed point -> `ProveDlog` -> P2PK address). The recipient address is used for both the redemption output and, by default, the fee-input change output.

### Transaction Components

#### 1. Inputs
- **Reserve Box**: The issuer's reserve box being spent, fetched from the server and verified on the Ergo node.
- **Fee Input(s)**: One or more wallet-owned P2PK boxes with no tokens, selected to cover the 1,000,000 nanoERG transaction fee. The reserve box itself is explicitly excluded from fee selection.

#### 2. Data Inputs
- **Tracker Box**: The tracker commitment box containing:
  - R4: Tracker's public key (GroupElement)
  - R5: AVL tree root digest tracking `hash(ownerKey||receiverKey) -> totalDebt`

#### 3. Outputs

**Output 0 - Updated Reserve** (must be at index 0):
- `value`: Original reserve value minus the redeemed amount
- `ergoTree`: The Basis reserve contract P2S address (from server configuration)
- `assets`: The reserve NFT token preserved from the input
- `additionalRegisters`:
  - `R4`: Issuer's public key (GroupElement)
  - `R5`: Updated reserve AVL tree root digest after inserting the new redeemed entry
  - `R6`: Tracker server NFT ID

**Output 1 - Recipient Redemption**:
- `value`: The redemption amount
- `ergoTree`: P2PK contract for the recipient
- `assets`: Empty
- `additionalRegisters`: Empty

**Output 2 - Fee**:
- `value`: 1,000,000 nanoERG
- `ergoTree`: Standard fee recipient contract
- `assets`: Empty

**Output 3 - Change** (only if change amount > 0):
- `value`: Fee input total value minus transaction fee
- `ergoTree`: P2PK contract for the change address (recipient address by default, or `--change-address` if provided)
- `assets`: Empty

#### 4. Context Extension Variables

| ID | Name | Type | Description | Required |
|----|------|------|-------------|----------|
| #0 | action | Byte | `action*10 + index` (0x00 for redemption at output index 0) | Yes |
| #1 | receiver | GroupElement | Recipient's public key (33 bytes compressed) | Yes |
| #2 | reserveSig | Coll[Byte] | Reserve owner's 65-byte Schnorr signature (33-byte a + 32-byte z) | Yes |
| #3 | totalDebt | Long | Total cumulative debt amount (nanoERG) | Yes |
| #4 | timestamp | Long | Payment timestamp (milliseconds since Unix epoch) | Yes |
| #5 | insertProof | Coll[Byte] | AVL proof for inserting into the reserve tree | Yes |
| #6 | trackerSig | Coll[Byte] | Tracker's 65-byte Schnorr signature | Yes (normal redemption) |
| #7 | lookupProofReserve | Coll[Byte] | AVL proof for looking up `(timestamp, redeemedDebt)` in reserve tree | No (omit for first redemption) |
| #8 | lookupProofTracker | Coll[Byte] | AVL proof for looking up `totalDebt` in tracker tree | Yes |

### Transaction Metadata
- `fee`: 1,000,000 nanoERG (0.001 ERG), paid by wallet-owned inputs
- `inputsRaw`: Serialized bytes of the reserve box and all fee input boxes
- `dataInputsRaw`: Serialized bytes of the tracker commitment box
- `secrets.dlog`: Recipient's private key (fetched from the node wallet via `/wallet/getPrivateKey`) so the node can satisfy `proveDlog(receiver)` during signing

## Transaction Generation Process

### Step 1: Retrieve Note Information
Query the Basis Tracker server:
- `GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}`
- Verify the note exists and the redemption amount is valid.

### Step 2: Retrieve Issuer's Reserve Box
Query the server:
- `GET /reserves/issuer/{issuer_pubkey}`
- Select a reserve box with collateral >= redemption amount.

### Step 3: Retrieve Tracker Box
- Use the provided `--tracker-box-id`, or
- Query `GET /tracker/latest-box-id` from the server.
- Verify the tracker box exists on the Ergo node.

### Step 4: Retrieve AVL Proofs
- `GET /proof/redemption?issuer_pubkey={issuer}&recipient_pubkey={recipient}` returns the tracker lookup proof and total debt.
- `POST /redemption/prepare` (or equivalent server endpoint) returns the reserve insert proof and, for subsequent redemptions, the reserve lookup proof.

### Step 5: Fetch Wallet Inputs and Recipient Secret
- Query the Ergo node wallet for boxes covering the fee (`/wallet/boxes/unspentByErgoTree` or similar).
- Select P2PK boxes with no tokens, excluding the reserve box.
- Fetch the recipient's private key from the node wallet via `/wallet/getPrivateKey`.

### Step 6: Build Signing Message
Build the 48-byte message signed by both reserve owner and tracker:
```
key = blake2b256(issuer_pubkey || recipient_pubkey)          // 32 bytes
message = key || longToByteArray(totalDebt) || longToByteArray(timestamp)  // 48 bytes total
```
- `totalDebt`: 8-byte big-endian cumulative debt
- `timestamp`: 8-byte big-endian payment timestamp in milliseconds

### Step 7: Sign with Issuer Key
Sign the message using the current CLI account's private key. The implementation retries nonces until both the challenge `e` and response `z` have their most-significant byte < 0x80 (ErgoScript signed-integer compatibility). See `specs/SCHNORR_SIGNATURE_SPEC.md`.

### Step 8: Request Tracker Signature
Query the server:
- `POST /tracker/signature` with `{issuer_pubkey, recipient_pubkey, total_debt, timestamp, emergency}`
- Extract the tracker's 65-byte Schnorr signature.

### Step 9: Assemble Transaction
Construct the unsigned transaction in the format expected by the Ergo node `/wallet/transaction/sign`:
```json
{
  "tx": {
    "inputs": [
      {
        "boxId": "<reserve_box_id>",
        "extension": {
          "0": "0200",
          "1": "07<recipient_pubkey_hex>",
          "2": "0e41<reserve_signature_hex>",
          "3": "05<long_to_vlq(totalDebt)>",
          "4": "05<long_to_vlq(timestamp)>",
          "5": "0e<insert_proof_hex>",
          "6": "0e41<tracker_signature_hex>",
          "8": "0e<tracker_lookup_proof_hex>"
        }
      },
      {
        "boxId": "<fee_input_box_id>",
        "extension": {}
      }
    ],
    "dataInputs": [{ "boxId": "<tracker_box_id>" }],
    "outputs": [
      { /* updated reserve box */ },
      { /* recipient output */ },
      { /* fee output */ },
      { /* change output */ }
    ]
  },
  "inputsRaw": ["<reserve_box_bytes>", "<fee_input_bytes>"],
  "dataInputsRaw": ["<tracker_box_bytes>"],
  "secrets": {
    "dlog": ["<recipient_private_key_hex>"]
  }
}
```

### Step 10: Output Generation
Write the assembled JSON to the file specified by `--output-file` or print it to stdout.

## Signing and Broadcasting

1. Sign the generated transaction with the Ergo node:
   ```bash
   curl -X POST http://127.0.0.1:9053/wallet/transaction/sign \
     -H "api_key: hello" \
     -H "Content-Type: application/json" \
     -d @transaction.json > signed_transaction.json
   ```

2. Broadcast the signed transaction:
   ```bash
   curl -X POST http://127.0.0.1:9053/transactions \
     -H "api_key: hello" \
     -H "Content-Type: application/json" \
     -d @signed_transaction.json
   ```

## Error Handling

- `InvalidPublicKey`: Public keys are not 33-byte compressed secp256k1
- `NoteNotFound`: No note exists for the issuer/recipient pair
- `InsufficientDebt`: Redemption amount exceeds available debt
- `NoReserveBox`: No reserve box with sufficient collateral found
- `NoTrackerBox`: No tracker box available
- `NoFeeInputs`: Wallet has no suitable P2PK/no-token boxes covering the fee
- `RecipientNotInWallet`: Node wallet does not contain the recipient's private key
- `SignatureError`: Schnorr signature generation failed (including compatibility retries)
- `AvlProofError`: AVL proof generation failed

## Security Considerations

1. **Private Key Handling**: The reserve owner private key is used only by the CLI account; the node never sees it. The recipient private key is fetched from the node wallet solely to satisfy the `proveDlog(receiver)` constraint during node signing.
2. **Input Validation**: All public keys, amounts, and proofs are validated before assembly.
3. **AVL Tree Consistency**: The reserve output R5 reflects the updated tree after the redemption insert.
4. **Double Redemption Prevention**: The reserve tree tracks cumulative redeemed amounts per `(owner, receiver)` pair.
5. **Emergency Redemption**: Only allowed after the tracker has been unavailable for 2160 blocks; the tracker signature field must still be present but verification is bypassed by the contract.

## References

- Contract: `chaincash/contracts/offchain/basis.es`
- Schnorr signature spec: `specs/SCHNORR_SIGNATURE_SPEC.md`
- Implementation: `crates/basis_cli/src/commands/transaction.rs`
