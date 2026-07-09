# Basis Redemption Execution Report

## Summary

This report documents the successful end-to-end execution of a Basis protocol redemption on a local Ergo testnet node. The flow covered:

1. Starting the tracker server with the updated tracker box updater.
2. Deploying a 1 ERG reserve by Alice.
3. Creating a 0.7 ERG Alice→Bob IOU note.
4. Waiting for the tracker server to commit the note to its on-chain tracker box.
5. Generating, signing, and broadcasting the redemption transaction.
6. Verifying the new reserve and redemption outputs on-chain.

**Redemption transaction:** `241ae6f475eb8599d50f11d722a0c3464af91a8c675a08ec314e8ae43605577e`  
**Result:** Success — Bob received 0.7 ERG, reserve collateral reduced to 0.3 ERG.

---

## Environment

| Component | Value |
|-----------|-------|
| Ergo node | `http://127.0.0.1:9053` |
| API key | `hello` (local test only) |
| Tracker server | `http://127.0.0.1:3048` |
| Network | Local testnet / private devnet |
| Transaction fee | 1,000,000 nanoERG (0.001 ERG) |

---

## Participants

| Role | Name | Public Key (compressed secp256k1, 33 bytes) |
|------|------|---------------------------------------------|
| Issuer / reserve owner | Alice | `0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83` |
| Recipient / creditor | Bob | `03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea` |
| Tracker | tracker | `024e564477ff457c601c01ad1cc31903f8b27b7d5e515bd03138891d8152d787b2` |

---

## Step-by-Step Execution

### Step 1: Start the Tracker Server

The server was started with `config/basis.toml` containing the local Ergo node URL, the tracker public key, the tracker secret key, and a change address for the tracker updater.

```bash
cargo run -p basis_server
```

Server logs confirmed:
- Reserve scanner started at the configured height.
- Tracker box updater started with the 10-minute interval.
- Server listening on `0.0.0.0:3048`.

### Step 2: Deploy the Reserve

Alice deployed a Basis reserve with 1 ERG collateral and a reserve NFT.

```bash
./target/debug/basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 1000000000 \
  --nft-id <RESERVE_NFT_ID>
```

The returned payload was submitted to the local Ergo node via `/wallet/payment/send`. The reserve box was confirmed on-chain:

| Field | Value |
|-------|-------|
| Reserve box ID | `66e8ff1dfcdc26a2cba6034cc525138d6f8394deea443c4878dcb4f0c0448ffb` |
| Collateral | 1,000,000,000 nanoERG (1.0 ERG) |
| Owner (R4) | `070377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83` |
| Tracker NFT (R6) | `0e20000b0695159e5f5c32c606385bd5f276d80133149c84c8b1325366381bf6f17f` |

### Step 3: Create the IOU Note

Alice created a 0.7 ERG note for Bob.

```bash
./target/debug/basis_cli note create \
  --demo \
  --amount 700000000 \
  --output alice_to_bob_note.json
```

The CLI signed the note with Alice's issuer key and obtained the tracker signature from the server. The note was then submitted to the tracker server and stored in its local AVL tree.

| Field | Value |
|-------|-------|
| Issuer | `0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83` |
| Recipient | `03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea` |
| Total debt | 700,000,000 nanoERG (0.7 ERG) |
| Timestamp | Note creation timestamp (milliseconds since Unix epoch) |

### Step 4: Wait for the Tracker Box Update

The tracker server's background updater detects the new note, rebuilds the AVL tree root, and submits a tracker box update transaction every 10 minutes. After the update was confirmed, the tracker box R5 register contained the new AVL root digest for the Alice→Bob note.

| Field | Value |
|-------|-------|
| Old tracker box ID | `2a18a5c02dfb9950f0bb52bceeccbc5210f07ef2d66a49e62fa5e8d19afe7b38` |
| Tracker update tx ID | `d84c2f37a6e8d641910f002261cd5b0b0a459b2bc6ac22e393d9fbc3d6a0386d` |
| New tracker box ID | `b64ef2caa24b9abe44f6476dc1bc2cd12cc27ae5fec680b4b71a3bb0b6552174` |
| New tracker R5 | `64d5d44e152c7e42673dea178b918d9195c2ba689da94046384dc40c55a64c836a01012000` |

**Critical:** The reserve contract verifies `totalDebt` against the tracker box R5 AVL tree. Redemption can only succeed after the tracker box has been updated to include the note.

### Step 5: Generate the Redemption Transaction

Bob generated the unsigned redemption transaction using the CLI.

```bash
./target/debug/basis_cli transaction generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 700000000 \
  --output-file redemption_tx.json
```

The CLI performed the following actions:
1. Queried the tracker server for the note and outstanding debt.
2. Selected Alice's reserve box `66e8ff1d...`.
3. Fetched the latest tracker box ID `b64ef2caa...`.
4. Retrieved the tracker lookup proof and reserve insert proof from the server.
5. Signed the redemption message with Alice's issuer key (reserve owner signature).
6. Requested the tracker signature from the server via `/tracker/signature`.
7. Fetched Bob's private key from the node wallet to populate `secrets.dlog`.
8. Selected wallet fee inputs covering the 1,000,000 nanoERG fee.
9. Built the unsigned transaction in `/wallet/transaction/sign` format with `tx`, `inputsRaw`, `dataInputsRaw`, and `secrets.dlog`.

The generated transaction had this structure:

```json
{
  "tx": {
    "inputs": [
      {
        "boxId": "66e8ff1dfcdc26a2cba6034cc525138d6f8394deea443c4878dcb4f0c0448ffb",
        "extension": {
          "0": "0200",
          "1": "0703af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
          "2": "0e41...reserve_owner_signature...",
          "3": "050000000029b92700",
          "5": "0e...reserve_insert_proof...",
          "6": "0e41...tracker_signature...",
          "8": "0e...tracker_lookup_proof..."
        }
      },
      { "boxId": "...fee_input_box...", "extension": {} }
    ],
    "dataInputs": [
      { "boxId": "b64ef2caa24b9abe44f6476dc1bc2cd12cc27ae5fec680b4b71a3bb0b6552174" }
    ],
    "outputs": [
      { /* Bob's 0.7 ERG redemption output */ },
      { /* Updated reserve output with 0.3 ERG */ },
      { /* Fee output */ },
      { /* Optional change output */ }
    ]
  },
  "inputsRaw": ["...", "..."],
  "dataInputsRaw": ["..."],
  "secrets": {
    "dlog": ["bob_private_key_hex"]
  }
}
```

### Step 6: Sign the Transaction

The unsigned transaction was POSTed to the Ergo node's `/wallet/transaction/sign` endpoint. The node used `secrets.dlog` to satisfy the `proveDlog(receiver)` condition and returned a signed transaction.

```bash
curl -X POST http://127.0.0.1:9053/wallet/transaction/sign \
  -H "Content-Type: application/json" \
  -H "api_key: hello" \
  -d @redemption_tx.json > signed_tx.json
```

### Step 7: Broadcast the Transaction

The signed transaction was broadcast to the network via `/transactions`.

```bash
curl -X POST http://127.0.0.1:9053/transactions \
  -H "Content-Type: application/json" \
  -H "api_key: hello" \
  -d @signed_tx.json
```

The transaction was accepted and later included in a block.

### Step 8: Verify On-Chain

The redemption transaction was queried from the local node:

```bash
curl http://127.0.0.1:9053/blockchain/transaction/byId/241ae6f475eb8599d50f11d722a0c3464af91a8c675a08ec314e8ae43605577e
```

**Transaction details:**

| Field | Value |
|-------|-------|
| Transaction ID | `241ae6f475eb8599d50f11d722a0c3464af91a8c675a08ec314e8ae43605577e` |
| Inputs spent | Reserve `66e8ff1d...`, fee input `15e5636f...` |
| Output 0 (new reserve) | `e51d7c383171af2b1863934776f340f1f8c8d8a37ccef490c9d6cb70b9091228` — 0.3 ERG |
| Output 1 (Bob's redemption) | `552a0408d724c19784db95184653eaa9ac271c3ed1d64896dc5411cd9d2637be` — 0.7 ERG |
| Output 2 (fee) | `244e4e1dd6fd87b73e07f4bf26fa12d07d514875de0d3a0e65e60d69ba9d79a8` — 0.001 ERG |
| Output 3 (change) | `768f06665d14f5d2d199697cf2c95f8c5f4d6155a9f7b9a797a5ee798297f144` — 5.998 ERG |

**New reserve box R5 (updated reserve AVL tree):**

```
641e7449a1967e7413e775aba413f24028c709fc7e15e8bd90bf34f24d6926ea9e01012000
```

---

## State Changes

### Tracker State

- The note was committed to the tracker AVL tree.
- The tracker box was updated on-chain to R5 `64d5d44e152c7e42673dea178b918d9195c2ba689da94046384dc40c55a64c836a01012000`.

### Reserve State

- The reserve AVL tree was updated from empty to a single entry.
- The key is `blake2b256(Alice_pubkey || Bob_pubkey)`.
- The value is `longToByteArray(700000000)`.
- The on-chain reserve collateral decreased from 1.0 ERG to 0.3 ERG.

### Bob's Balance

- Bob received a 0.7 ERG UTXO at output index 1 of the redemption transaction.

---

## Key Implementation Details

### Tracker Signature

The tracker server produced the 65-byte Schnorr signature required by the contract. The server was configured with `tracker_secret_key` in `config/basis.toml`, so it signed locally rather than delegating to the Ergo node's `/utils/schnorrSign` endpoint.

### Recipient Private Key

The CLI fetched Bob's private key from the local node wallet (`/wallet/getPrivateKey`) and included it in `secrets.dlog`. This allowed the node to satisfy the `proveDlog(receiver)` condition when signing the transaction. In production, Bob would run the redemption on a node that controls his own key.

### Transaction Format

The redemption transaction was built for the Ergo node's `/wallet/transaction/sign` endpoint, not the older `/wallet/transaction/send` format. This required:
- A top-level `tx` object with nested `inputs`, `outputs`, and `dataInputs`.
- `inputsRaw` and `dataInputsRaw` arrays with serialized box bytes.
- `secrets.dlog` containing the recipient private key.

### Action Byte and Output Index

Context variable #0 was set to `0x00` (Byte constant `0200`). The contract uses `action % 10` to determine the reserve output index, so the reserve output must be at index 0 in `tx.outputs`.

---

## Lessons Learned and Notes

1. **Tracker box must be current before redemption.** The first redemption attempt failed because the tracker box had not yet been updated with the new note. After waiting for the 10-minute updater cycle, the tracker box R5 contained the correct AVL root and redemption succeeded.

2. **Use `/wallet/transaction/sign` + `/transactions` for redemption.** The older `/wallet/transaction/send` format does not accept the nested `tx` structure with `inputsRaw`/`dataInputsRaw`/`secrets.dlog` required for the Basis contract.

3. **Recipient key must be in the signing node wallet.** The node must be able to satisfy `proveDlog(receiver)`. The CLI handles this by fetching the key from `/wallet/getPrivateKey`, but this only works when the node wallet controls the recipient address.

4. **Reserve output must be at index 0.** The action byte `0x00` resolves to output index 0, so the first output of `tx.outputs` must be the updated reserve box.

5. **Configuration secrets should not be committed.** During this work, `config/basis.toml` was removed from git tracking and replaced with `config/basis.toml.example` to avoid leaking `tracker_secret_key` or `api_key`.

---

## Second Redemption Test (0.5 ERG reserve, 0.3 ERG full redemption)

### Summary

A second end-to-end redemption test was executed to verify the flow with a fresh reserve:

1. Deployed a 0.5 ERG reserve using a freshly minted reserve NFT.
2. Created a 0.3 ERG Alice→Bob IOU note and submitted it to the tracker.
3. Waited for the tracker server to commit the note on-chain.
4. Generated, signed, and broadcast a full (0.3 ERG) redemption transaction.
5. Verified the new reserve and redemption outputs on-chain.

**Reserve creation transaction:** `5c9dbe9f02b460bc03a70764298d1e122b151490402634eb0ef19d39a1af1f48`
**Redemption transaction:** `a253c774afda254c4c20dc40a214fdb3e51414340b8166caf19f5c3b9bac09c8`
**Result:** Success — Bob received 0.3 ERG, reserve collateral reduced from 0.5 ERG to 0.2 ERG.

### Reserve Deployment

```bash
./target/debug/basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 500000000 \
  --nft-id 3b8ae4391c249ff3ad3d042fed78a87be9b37dae17ff9c4f2f365b7be51c497f
```

The CLI emits a human-readable payload, which was transcribed into the array format expected by `/wallet/payment/send` (a JSON array of `PaymentRequest` objects). The reserve was confirmed:

| Field | Value |
|-------|-------|
| Reserve box ID | `d272eacd0c65b228fdc60455b3b23b923be20f897d2a2e13f2657d8fa14c717f` |
| Collateral | 500,000,000 nanoERG (0.5 ERG) |
| Owner (R4) | `070377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83` |
| Empty AVL tree (R5) | `644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900012000` |
| Tracker NFT (R6) | `0e20000b0695159e5f5c32c606385bd5f276d80133149c84c8b1325366381bf6f17f` |

### Note Creation

Demo mode signs the note locally and only writes it to disk, so it was submitted to the tracker server explicitly:

```bash
./target/debug/basis_cli note create --demo --amount 300000000 --output alice_to_bob_03_note.json

curl -X POST http://127.0.0.1:3048/notes \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
    "recipient_pubkey": "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
    "amount": 300000000,
    "timestamp": 1783588179731,
    "signature": "023ccbbbd80e43b7fae3c39808adff2c284a079ff4ecd7dc91557c4dfc375e16ab47104eafcc4e2330ab5cf4bba198ba8b2a245b9676980f4df8ddbdffefffb6eb"
  }'
```

### Tracker Box Update

The tracker AVL tree root after the note was added became `8055c595de309006f31c4941908a066abab1dbbcc263ac0e5b498461386ad92d01`. The on-chain tracker box was updated by the background updater:

| Field | Value |
|-------|-------|
| Old tracker box ID | `b64ef2caa24b9abe44f6476dc1bc2cd12cc27ae5fec680b4b71a3bb0b6552174` |
| Tracker update tx ID | `e29faff66a3898667f8a573dd949fb1806983ad12a985c870ae3789bb5a57398` |
| New tracker box ID | `8de094ec99c1c502fe3ef8db21aa0bf9edd3bb60a04aaef7e4a3c15273bd7dc9` |
| New tracker R5 | `648055c595de309006f31c4941908a066abab1dbbcc263ac0e5b498461386ad92d01012000` |

**Note:** The background updater initially failed with "No wallet boxes available to pay transaction fee" because all wallet boxes held tokens (the updater only selects token-free boxes). A plain 0.5 ERG box was created in the wallet to fund subsequent updates.

### Redemption Transaction

Generated with the current tracker box ID passed explicitly (the server's tracked box ID was stale because the tracker scanner failed to parse the current box's R6 register — see "Lessons Learned"):

```bash
./target/debug/basis_cli transaction generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 300000000 \
  --tracker-box-id 8de094ec99c1c502fe3ef8db21aa0bf9edd3bb60a04aaef7e4a3c15273bd7dc9 \
  --output-file redemption_03_tx.json

curl -X POST http://127.0.0.1:9053/wallet/transaction/sign \
  -H "Content-Type: application/json" -H "api_key: hello" \
  -d @redemption_03_tx.json > redemption_03_signed.json

curl -X POST http://127.0.0.1:9053/transactions \
  -H "Content-Type: application/json" -H "api_key: hello" \
  -d @redemption_03_signed.json
```

**Confirmed transaction details:**

| Field | Value |
|-------|-------|
| Transaction ID | `a253c774afda254c4c20dc40a214fdb3e51414340b8166caf19f5c3b9bac09c8` |
| Inputs spent | Reserve `d272eacd...`, fee input `852571cb...` |
| Output 0 (new reserve) | `22346ef6e02c9ab4ff2cd2168bf5143dcd7122cb2e74d4602cbb30e49b8abdca` — 0.2 ERG |
| Output 1 (Bob's redemption) | `2e13126ac35abb0ab35f9da5fbc50860707153fbd24c8f5c9f06e39f287eb0fe` — 0.3 ERG |
| Output 2 (fee) | `d4f2c6a948a1898201f87cc777385e0683142c2af96a63e11fe747177e6ce0e2` — 0.001 ERG |
| Output 3 (change) | `6ccb1c5e08eee8b81fe34ea3a7ccf2e07798fc333c92ae82991653cb0f1a42eb` — 0.498 ERG |

**New reserve box R5 (updated reserve AVL tree):**

```
64e3caf2764ba4e91ba94876f34f65b4ad6ba58cff4be9b4f9b233e9bd9d4cb12301012000
```

The reserve now records the 0.3 ERG redemption in its on-chain AVL tree under `blake2b256(Alice_pubkey || Bob_pubkey)`.

### Lessons Learned from the Second Test

1. **Reserve creation CLI output is not directly submittable.** The CLI prints a human-readable payload with `token_id` (snake_case) and no surrounding `requests` wrapper. To submit via `/wallet/payment/send`, the payload must be transcribed into a JSON array of `PaymentRequest` objects with camelCase `tokenId`.

2. **Demo-mode notes are not auto-submitted.** `basis_cli note create --demo` signs locally and only writes the note file. The note must be submitted to the tracker server explicitly via `POST /notes` with the `CreateNoteRequest` shape (`issuer_pubkey`, `recipient_pubkey`, `amount`, `timestamp`, `signature`).

3. **The background updater needs token-free wallet boxes.** The updater's fee-input selector filters out boxes that hold any assets. If the wallet only has tokenized boxes, the updater logs "No wallet boxes available to pay transaction fee" every cycle. The fix is to keep at least one small plain ERG box in the wallet for fees.

4. **The broadcast endpoint returns a plain string.** `POST /transactions` returns the transaction id as a JSON string (e.g. `"abc..."`), not an object. The updater's `broadcast_transaction` parser only looks for `body["id"]` / `body["txId"]`, so it logs "Missing tx id" even though the broadcast succeeded. The transaction still gets included on-chain.

5. **The tracker scanner can fail to parse the current tracker box.** On this run the tracker scanner logged `Failed to parse tracker box ... Invalid R6 register: invalid digit found in string`, leaving the server's `tracker_box_id` shared state stale. The CLI's `generate-redemption` therefore needs `--tracker-box-id` passed explicitly when the server returns an old box id. The redemption itself does not depend on the shared-state box id because the CLI verifies the box against the node directly.

6. **End-to-end redemption remains reliable once the tracker box is current.** After the updater committed the new note on-chain, the full 0.3 ERG redemption succeeded on the first attempt.

---

- [Tracker Box Update Specification](server/tracker_box_update_spec.md)
- [Redemption Transaction Format Specification](server/redemption_transaction_format_spec.md)
- [Redemption State Specification](server/redemption_state_spec.md)
- [Interactive Demo](interactive_demo.md)
- Ergo node API: `/wallet/transaction/sign`, `/transactions`, `/wallet/getPrivateKey`
