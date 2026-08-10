# Basis Redemption Execution Report

> **Status: historical v1 execution record.** It documents past transactions
> and retired commands; it is not a current runbook, exposure statement, or
> deployment/readiness claim.

## Summary

This report documents successful end-to-end Basis protocol redemptions, beginning on a local Ergo testnet node and culminating in mainnet multi-redemption runs against a wallet-owned tracker. The flows covered:

1. Starting the tracker server with the updated tracker box updater.
2. Deploying reserves and creating IOU notes.
3. Waiting for the tracker server to commit notes to its on-chain tracker box.
4. Generating, signing, and broadcasting redemption transactions.
5. Verifying new reserves and redemption outputs on-chain.

**First local redemption transaction:** `241ae6f475eb8599d50f11d722a0c3464af91a8c675a08ec314e8ae43605577e`  
**Result:** Success — Bob received 0.7 ERG, reserve collateral reduced to 0.3 ERG.

**Latest mainnet multi-redemption:** two consecutive 0.1 ERG redemptions against one 0.3 ERG reserve (collateral reduced from 0.3 ERG → 0.2 ERG → 0.1 ERG). See the [Tenth Redemption Test](#tenth-redemption-test-automated-integration-runner-two-01-erg-local-sign-redemptions-against-one-03-erg-reserve) for full details.

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

## Third Redemption Test (0.5 ERG reserve, 0.4 ERG debt, 0.2 ERG partial redemption)

### Summary

A third end-to-end redemption validated the new confirmation-aware API (`/tracker/state`, `/tracker/pending-tx`, `/notes/state`) together with the tracker-box R6 parsing fix and the plain-string broadcast parser fix:

1. Deployed a 0.5 ERG reserve using an existing reserve NFT.
2. Created a 0.4 ERG Alice→Bob IOU note and submitted it to the tracker.
3. Waited for the tracker server to commit the note on-chain; verified confirmation via the new API.
4. Generated, signed, and broadcast a partial (0.2 ERG) redemption transaction.
5. Verified the new reserve (0.3 ERG) and Bob's redemption output (0.2 ERG) on-chain.

**Reserve creation transaction:** `6e022f4abb5e5236a35f0b5ba31d3f2ae502f1dcba44c1c20a347d4d4fe7460d`
**Tracker update transaction:** `c57ca332ed27f92ba49c9d944c6703edd1d3a93f02ffbcde7862ec9ec8059a27`
**Redemption transaction:** `96c55b392eeb3118dd009e301e31282f2af4da67952b4d6c89c44ef3458a35ea`
**Result:** Success — Bob received 0.2 ERG, reserve collateral reduced from 0.5 ERG to 0.3 ERG, with 0.2 ERG debt still outstanding.

### Reserve Deployment

An existing reserve NFT was reused instead of minting a new one (to avoid another slow token-issuance cycle):

```bash
./target/debug/basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 500000000 \
  --nft-id 08c9d7a2c43676f3f6e25f3fe713314a89c4ce3430941887889ff8e4b285f594
```

| Field | Value |
|-------|-------|
| Reserve creation tx | `6e022f4abb5e5236a35f0b5ba31d3f2ae502f1dcba44c1c20a347d4d4fe7460d` (height 1825416) |
| Reserve box ID | `f2e1594efdd1065e87d37e369212db075b5d61b597f4c693f59f7638eb718cb0` |
| Collateral | 500,000,000 nanoERG (0.5 ERG) |
| Reserve NFT | `08c9d7a2c43676f3f6e25f3fe713314a89c4ce3430941887889ff8e4b285f594` |

### Note Creation and On-Chain Confirmation

```bash
./target/debug/basis_cli note create --demo --amount 400000000 --output /tmp/note_04_erg.json
# then POST /notes with the CreateNoteRequest body (issuer, recipient, amount, timestamp, signature)
```

The background updater published the new AVL root once a token-free fee box was available:

| Field | Value |
|-------|-------|
| Old tracker box ID | `8de094ec99c1c502fe3ef8db21aa0bf9edd3bb60a04aaef7e4a3c15273bd7dc9` |
| Tracker update tx ID | `c57ca332ed27f92ba49c9d944c6703edd1d3a93f02ffbcde7862ec9ec8059a27` (height 1825604) |
| New tracker box ID | `b9c4bdbd8de1cee5d41006baf32c1c0ba5621bd7749c8a3ee9bc603cb95a6f72` |
| New tracker digest (R5) | `8f8e9d303b9fe3676432a671df8e2c1762013ca949c1c51632e9de8592c5d5fc01` |

The new confirmation API reported the state transition accurately. `GET /tracker/state` returned `local_digest == confirmed_digest == 8f8e9d30…` with `confirmed_box_id = b9c4bdbd…` and no pending tx, and `POST /notes/state` returned:

```json
{
  "local": 400000000,
  "confirmed": 400000000,
  "pending": null,
  "already_redeemed": 0,
  "redeemable": true,
  "redeemable_amount": 400000000,
  "status": "confirmed"
}
```

### Partial Redemption Transaction (0.2 ERG of 0.4 ERG debt)

```bash
./target/debug/basis_cli transaction generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 200000000 \
  --tracker-box-id b9c4bdbd8de1cee5d41006baf32c1c0ba5621bd7749c8a3ee9bc603cb95a6f72 \
  --change-address 9fPRvaMYzBPotu6NGvZn4A6N4J2jDmRGs4Zwc9UhFFeSXgRJ8pS \
  --output-file /tmp/redemption_02_tx.json

curl -X POST http://127.0.0.1:9053/wallet/transaction/sign \
  -H "Content-Type: application/json" -H "api_key: hello" \
  -d @/tmp/redemption_02_tx.json > /tmp/redemption_02_signed.json

curl -X POST http://127.0.0.1:9053/transactions \
  -H "Content-Type: application/json" -H "api_key: hello" \
  -d @/tmp/redemption_02_signed.json
```

**Confirmed transaction details (height 1825612):**

| Field | Value |
|-------|-------|
| Transaction ID | `96c55b392eeb3118dd009e301e31282f2af4da67952b4d6c89c44ef3458a35ea` |
| Inputs spent | Reserve `f2e1594e…` (0.5 ERG), fee input `839ca95e…` |
| Output 0 (new reserve) | `95cac2385ec569ee9967d04ed8d12b2d2d9f6efde13a4ece1dbdfa23da44be01` — 0.3 ERG |
| Output 1 (Bob's redemption) | `bf02a443a980dc7c9033f46ae63910abe6309fe02c94f0074144d158448e1d1a` — 0.2 ERG |
| Output 2 (fee) | `f923d95168fe3c65b0fe5ed4b1dbe3b81c32d5b22373e62c20ba6457bdde8263` — 0.001 ERG |
| Output 3 (change) | `785d480f17878c9a43f03ba0b77cba655f1ac12da3145dc2026bb347c8997e70` — 0.498 ERG |

**New reserve box R5 (updated reserve AVL tree):**

```
64c6db7c180c70664a4d71c8dd3c43ec47842e7d691d8e399753a793d32a4318b301012000
```

The reserve now records the 0.2 ERG redemption under `blake2b256(Alice_pubkey || Bob_pubkey)`; 0.2 ERG of the original 0.4 ERG debt remains outstanding and redeemable.

### Lessons Learned from the Third Test

1. **The confirmation API tracks on-chain truth correctly.** `/notes/state` flipped to `status = "confirmed"`, `redeemable = true`, `redeemable_amount = 400000000` exactly when the tracker update tx `c57ca332…` confirmed, and `/tracker/state` reconciled `confirmed_box_id` to `b9c4bdbd…` automatically. No stale-box workaround was required, though `--tracker-box-id` was still passed explicitly for safety.

2. **The R6 parsing fix works end-to-end.** The tracker scanner now parses R6 as a `Coll[Byte]` NFT id and uses `creation_height` for `last_verified_height`; the server no longer logs `Invalid R6 register` and keeps the shared `tracker_box_id` current.

3. **The plain-string broadcast parser fix works.** The updater logged `Tracker box update submitted. Transaction ID: c57ca332…` directly from the string response of `POST /transactions`, with no `Missing tx id` warning.

4. **`already_redeemed` lags the on-chain reserve R5 right after broadcast.** Immediately after confirmation, `/notes/state` still reported `already_redeemed = 0` even though the new reserve box R5 already recorded the 0.2 ERG redemption. The local note's `amount_redeemed` catches up on the next scanner cycle that processes the spent/recreated reserve box. The on-chain reserve R5 is the authoritative redemption record.

5. **The server listens on the configured port 3048, not 8080.** Health/state queries must target `http://127.0.0.1:3048`.

6. **A token-free fee box must remain available between cycles.** Reserve creation spent the only plain ERG box, so the updater logged `No wallet boxes available to pay transaction fee` until a fresh plain box (`b801f95f…`) confirmed. Keeping at least one small plain ERG box in the wallet avoids update stalls.

---

## Fourth Redemption Test (0.2 ERG reserve, 0.4 ERG debt, 0.1 ERG partial redemption, NFT `2e13126a…`)

### Summary

A fourth end-to-end redemption validated the confirmation-aware API against a fresh 0.2 ERG reserve created with reserve NFT `2e13126ac35abb0ab35f9da5fbc50860707153fbd24c8f5c9f06e39f287eb0fe`. The previously confirmed 0.4 ERG Alice→Bob IOU note (totalDebt 400,000,000 nanoERG, 0.2 ERG still outstanding after the third test) was redeemed for 0.1 ERG against the new reserve. Because the new reserve's AVL tree was empty, this was the first redemption recorded against that reserve.

1. Deployed a 0.2 ERG reserve with NFT `2e13126a…`.
2. Created a token-free 0.6 ERG fee box to fund updater/redemption fees.
3. Reused the already-confirmed Alice→Bob note (0.4 ERG totalDebt) rather than re-issuing a same-pair note (which would have altered totalDebt and desynced from the on-chain digest).
4. Generated, signed, and broadcast a 0.1 ERG redemption transaction via the CLI flow.
5. Verified the new reserve (0.1 ERG) and Bob's redemption output (0.1 ERG) on-chain.

**Reserve creation transaction:** `b134d6ad229e11fe7cc3acdc4330517a35c9e287dcd510fc2eecd2e605bbbb5b`
**Fee-box funding transaction:** `ff01f47ddd2a1cc678b45276880e19c6a079e68cc41b9597fd69e92836edeb14`
**Redemption transaction:** `8f647e58676d18f7b048bb8b0b78befb724ff54632f0d7c795ac0a5204537a9e`
**Result:** Success — Bob received 0.1 ERG, reserve collateral reduced from 0.2 ERG to 0.1 ERG.

### Reserve Deployment

```bash
./target/debug/basis_cli reserve create \
  --owner 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --amount 200000000 \
  --nft-id 2e13126ac35abb0ab35f9da5fbc50860707153fbd24c8f5c9f06e39f287eb0fe
```

| Field | Value |
|-------|-------|
| Reserve creation tx | `b134d6ad229e11fe7cc3acdc4330517a35c9e287dcd510fc2eecd2e605bbbb5b` (height 1825938) |
| Reserve box ID | `b4a1c033e9c7e825bd63c945e0dc0a8748e62ba10e48c503a249ba190138a9f2` |
| Collateral | 200,000,000 nanoERG (0.2 ERG) |
| Reserve NFT | `2e13126ac35abb0ab35f9da5fbc50860707153fbd24c8f5c9f06e39f287eb0fe` |

### Fee Box Funding

The wallet held no token-free boxes after reserve creation, so a plain 0.6 ERG box was funded for updater/redemption fees:

| Field | Value |
|-------|-------|
| Funding tx | `ff01f47ddd2a1cc678b45276880e19c6a079e68cc41b9597fd69e92836edeb14` (height 1825938) |
| Token-free box | `eaacaf565467fcd0151f38ac17d8e99d5e27cab8046601057aae7f0aa2399385` |
| Amount | 600,000,000 nanoERG (0.6 ERG) |

### Redemption Transaction (0.1 ERG)

```bash
./target/debug/basis_cli transaction generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 100000000 \
  --tracker-box-id b9c4bdbd8de1cee5d41006baf32c1c0ba5621bd7749c8a3ee9bc603cb95a6f72 \
  --change-address 9fPRvaMYzBPotu6NGvZn4A6N4J2jDmRGs4Zwc9UhFFeSXgRJ8pS \
  --output-file /tmp/redemption_01_test.json

curl -X POST http://127.0.0.1:9053/wallet/transaction/sign \
  -H "Content-Type: application/json" -H "api_key: hello" \
  -d @/tmp/redemption_01_test.json > /tmp/redemption_01_signed.json

curl -X POST http://127.0.0.1:9053/transactions \
  -H "Content-Type: application/json" -H "api_key: hello" \
  -d @/tmp/redemption_01_signed.json
```

`generate-redemption` auto-selected reserve `b4a1c033…` (empty reserve tree → first redemption).

**Confirmed transaction details (height 1825943):**

| Field | Value |
|-------|-------|
| Transaction ID | `8f647e58676d18f7b048bb8b0b78befb724ff54632f0d7c795ac0a5204537a9e` |
| Inputs spent | Reserve `b4a1c033…` (0.2 ERG), fee input `eaacaf56…` (0.6 ERG) |
| Output 0 (new reserve) | `9f1a413a239a9ad61561924834c0e2b9acbb6a3d0443fdc4b6aa355a83aaf450` — 0.1 ERG (P2S `4ZhBzJfN…`) |
| Output 1 (Bob's redemption) | `0beace95f9022d9407deaaa4e111d97297482a3bc3ac2e65dce548ffc9e3c867` — 0.1 ERG (`9hnupHc2…`) |
| Output 2 (fee) | `3357d8744999e4ad493106d4fddae50d7cff4b9adee493e530e4e05331e55371` — 0.001 ERG |
| Output 3 (wallet change) | `eaa24ce9494312af872e017eb37b22f3e97d4c61576c210a047dec5cab86c8fa` — 0.599 ERG (token-free) |

The redemption tx id is also recorded in `/tmp/redemption_txid.txt`.

### Lessons Learned from the Fourth Test

1. **Redeeming against a fresh reserve with an existing confirmed note works.** Reusing the already-confirmed Alice→Bob note (0.4 ERG totalDebt) avoided re-issuing a same-pair note, which would have changed totalDebt and desynced the local state from the on-chain tracker digest.

2. **Only the CLI `generate-redemption` path broadcasts on-chain.** `basis-ui` (`crates/basis_app/src/ui.rs`) and `basis_cli note redeem` only prepare/mark local state and never broadcast; `basis_cli transaction generate-redemption` is the proven path for confirmable redemptions.

3. **Wallet change address differs from the server address.** Reserve/fee-box funding via `/wallet/payment/send` routes change (and all tokens) to the wallet's own address `9fPRvaMYz…`, keeping them spendable. Funding payloads must be a JSON array of `PaymentRequest` with camelCase `tokenId`.

4. **A token-free change box is left for the next cycle.** After this redemption, change box `eaa24ce9…` (0.599 ERG, token-free) is available for subsequent updater/redemption fees, avoiding update stalls.

5. **Empty reserve tree ⇒ first redemption.** Because reserve `b4a1c033…` had no prior redemptions, this 0.1 ERG redemption was the first entry written to the reserve's AVL tree under `blake2b256(Alice_pubkey || Bob_pubkey)`.

## Fifth Redemption Test (0.1 ERG reserve, 0.4 ERG debt, 0.01 ERG partial redemption, **client-side `proveDlog`**)

Goal: verify that a redemption can be **signed entirely off-chain** (client-side `proveDlog` for BOTH the reserve input's receiver and the fee input's fee payer) and accepted by the node, instead of delegating signing to the node wallet (`/wallet/transaction/sign`). This is the path a TUI/cold-signer uses: the tracker only supplies proofs + its own signature; the receiver produces the `proveDlog` locally.

### Fixtures

- Tracker server rebuilt on **ergo-lib 0.28** (the deployed Basis contract uses the `Modulo`/`Exponentiate`/`MultiplyGroup` opcodes, which older ergo-lib could not reduce) and restarted; the existing committed Alice→Bob note (totalDebt 0.4 ERG, redeemable 0.4 ERG) was reloaded into tracker box `b9c4bdbd…`.
- Fresh empty reserve created by reusing an already-held singleton token as the reserve NFT (no new token mint):

| Field | Value |
|-------|-------|
| Reserve creation tx | `223fc52ae7058cfbad8fa5ff469e539f426cb0589ae3a2f56ac0c80d1facf292` (height 1826107) |
| Reserve box ID | `ddfd4223d3e6d3a9c4da0488b5daed0bbeaec51bcef9208ef4a756d5cbfecec1` |
| Collateral | 100,000,000 nanoERG (0.1 ERG), empty AVL tree (first redemption) |
| Reserve NFT | `68b5a5106ad1a1f54bea5a95cc4c6aebbc7bd0ee8b26eb52f9783efe719b3921` (reused singleton) |
| Tracker NFT (R6) | `000b0695159e5f5c32c606385bd5f276d80133149c84c8b1325366381bf6f17f` |

- Reserve creation consolidated the wallet into a single token-heavy box, so a fresh token-free fee box was funded: funding tx `dba382e73fdb528ed9978353307f02bd674a2091fee746c92a8a6bf3783144cb` (height 1826108), fee box `3e26e9d0bc1d4c0dd73cb84dc27c147b35eada8fbf3fccdb24dd641896e7a8da` (0.05 ERG, token-free).

### Redemption Command (client-side signing)

```bash
./target/debug/basis_cli transaction generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 10000000 \
  --tracker-box-id b9c4bdbd8de1cee5d41006baf32c1c0ba5621bd7749c8a3ee9bc603cb95a6f72 \
  --change-address 9fPRvaMYzBPotu6NGvZn4A6N4J2jDmRGs4Zwc9UhFFeSXgRJ8pS \
  --local-sign
```

`generate-redemption` auto-selected reserve `ddfd4223…` (empty tree → first redemption), fetched the receiver + fee-payer dlog secrets from the node wallet (stand-in for a local keystore), and signed in-process with ergo-lib's `Wallet::sign_transaction`.

### Root Cause Found and Fixed

The first local-sign attempts were rejected: `HTTP 400 Malformed transaction: Scripts of all transaction inputs should pass verification. <txid>: #0 => Success((false,1273))`. Decisive isolation showed the contract + fixtures + extension *values* were correct (a node-signed control of the same tx passed `/transactions/check`, and the deterministic extension vars `#0,#1,#3,#4,#5,#8` were byte-identical between local and node-signed), yet the node hashed the local tx to a different id than ergo-lib did.

**Cause:** the reserve input's `ContextExtension` is serialized as part of the `UnsignedInput` (`boxId ++ extension`), so the extension byte order is part of `bytes_to_sign`. The reference client (`sigma.interpreter.ContextExtension` in sigmastate-interpreter) stores variables in a `scala.collection.Map[Byte, _]` whose serializer does `obj.values.foreach { (id, v) => put(id); putValue(v) }` — i.e. it iterates the map in **index (HashMap) order, not insertion order**. ergo-lib serializes its insertion-ordered map in parse order, so the local `bytes_to_sign` diverged from the node's and `proveDlog(receiver)` failed. (A plain ERG payment — empty extension — matched node serialization, which masked this.)

**Fix:** before parsing/signing the unsigned tx in `sign_and_broadcast_local`, reorder the reserve input's extension keys to Scala's index order for the first-redemption variable set `{0,1,2,3,4,5,6,8}`, which is `0,5,1,6,2,3,8,4` (helper `reorder_reserve_extension_scala` in `crates/basis_cli/src/commands/transaction.rs`). Confirmed by an ergo-lib tx-id parity test (reordered → id `2022a955…`, matching the node) and by the confirmed broadcast below.

### Confirmed Transaction (height 1826140)

| Field | Value |
|-------|-------|
| Transaction ID | `c897018c1d59661769688feffddc2121923c64cf0769e4961f4b7c9f681558cd` |
| Inputs spent | Reserve `ddfd4223…` (0.1 ERG), fee input `3e26e9d0…` (0.05 ERG) |
| Output 0 (new reserve) | `b4b2c78adff7651cdb23a6558da661658c8430529d6c6f65b1b2dd667a1e4118` — 0.09 ERG (reserve NFT `68b5a510…`) |
| Output 1 (Bob's redemption) | `ac5af621b6c23bd84e64dff516def209b748495b1a93e36561da40ff266973fd` — 0.01 ERG (`9hnupHc2…`) |
| Output 2 (fee) | `3bcadc9ba846dee67362ac727d69c7b7999a0541f434544c6c928883c72162be` — 0.001 ERG |
| Output 3 (wallet change) | `350ecc812bcc4aee7557267743d763b071aa85b57d344f056268de4557ca3f4c` — 0.049 ERG (token-free) |

The tx id is recorded in `/tmp/redemption_txid.txt`. This is the first redemption confirmed with **client-side** `proveDlog` for both inputs — the node only validated; it did not sign.

### Lessons Learned from the Fifth Test

1. **Context-extension variable order is part of `bytes_to_sign`.** Because `UnsignedInput = boxId ++ extension`, the order in which context variables are serialized changes the signed message. Rust signers must emit the reserve input's extension in Scala's `ContextExtension` (index/HashMap) order — for the first-redemption set `{0,1,2,3,4,5,6,8}` that order is `0,5,1,6,2,3,8,4`. ergo-lib's insertion order is not compatible by default.

2. **`/transactions/check` validates without spending.** Useful as a no-cost control: the node-signed version of the exact redemption tx passed `/transactions/check`, proving the contract and fixtures were sound and isolating the failure to the locally-produced proof.

3. **Extension order depends on the variable *set*.** The order `0,5,1,6,2,3,8,4` is specific to keys `{0,1,2,3,4,5,6,8}` (first redemption, no `#7`). A subsequent redemption adds `#7` (reserve lookup proof), changing Scala's iteration order to `0,5,1,6,2,7,3,8,4` for the set `{0,1,2,3,4,5,6,7,8}`. Both orders are produced by `scala_context_extension_order`, which reproduces Scala's `immutable.HashMap` (HashTrieMap) iteration and was self-validated against the on-chain-confirmed first-redemption order, so local signing works for subsequent redemptions too.

4. **Empty extension ⇒ serializers agree.** A token-free, register-free, extension-free payment serialized identically in ergo-lib 0.28 and node 6.0.3; the divergence only appears once inputs carry a context extension, which is every Basis redemption/top-up.

5. **ergo-lib 0.28 reduces the Basis contract.** The `Modulo`/`Exponentiate`/`MultiplyGroup` opcodes in `basis.es` require ergo-lib ≥ 0.28 to reduce the reserve script to its residual `proveDlog(receiver)`; earlier versions threw `NotImplementedOpCode` during signing.

---

## Sixth Redemption Test (tracker-assisted 2-phase server endpoints, CLI-driven)

Goal: verify the new tracker-assisted 2-phase flow — `POST /redemption/build` (tracker builds the unsigned tx and signs the fee input(s) locally) → the client adds the reserve input's `proveDlog(recipient)` over the identical `bytes_to_sign` → `POST /redemption/submit` (tracker broadcasts). Neither end reorders the reserve input's context extension, so the Scala-order serialization produced by the build is preserved.

### Prerequisites fixed in this cycle

1. **`basis_trees/src/avl_tree.rs` `generate_insert_proof`** now always uses `Operation::InsertOrUpdate` (the deployed contract calls strict `insert`, but the proof bytes for an insert-or-update of a new key are identical, and this also supports the future `insertOrUpdate` contract).
2. **Deep rebuild of the temp tree from the proof cache** — `AVLTree::clone()` is a shallow `Rc<RefCell<Node>>` clone, so proof generation must rebuild from cached nodes instead of cloning.
3. **`complete_redemption` (`basis_store/src/redemption.rs`)** syncs the reserve AVL tree with the note's **pre-refresh** payment timestamp (the on-chain reserve tree value is `payment_timestamp || already_redeemed`; refreshing the note timestamp first would make subsequent proofs diverge from the on-chain entry).
4. **Reserve selection** picks the smallest sufficient unspent reserve whose on-chain R5 digest equals the tracker's local reserve-tree digest (`/utxo/byId` unspent check + R5 digest compare).

### Confirmed Transaction (height 1826973)

| Field | Value |
|-------|-------|
| Transaction ID | `e5a2845276d6294749bef40db2bab6fe4263ae18cfab00131f5d9a65a267f825` |
| Reserve spent | `6dda1af8…` (fresh, empty tree) |
| Output 0 (new reserve) | `61b1764c…` — 0.05 ERG, R5 digest `22259cd9…` |
| Recipient payout | Bob +0.05 ERG |

---

## Seventh Redemption Test (TUI-driven tracker-assisted redemption, submit state-sync fix)

Goal: exercise the same 2-phase endpoints from the TUI (`basis-ui`, account `bob`): Notes → redeem → amount → the TUI signs the issuer message locally, calls `/redemption/build`, adds `proveDlog(recipient)`, calls `/redemption/submit`.

### Setup

- Fresh reserve created (NFT `3ab6b8b19de05f0fefc1f9e62c1e9c20f8de717339624802d6710afad58937ab`, creation tx `7b087df542357c4f826e6ec974a7938106a206d41a85ea0afbc5b264c0664155`, height 1827020) → reserve box `2ced1079ef2a5869f5a3ea9f284bf04c84d22e6a3de80190aeff72060858eec5` (0.2 ERG, empty tree), picked up by the reserve scanner.
- Node-wallet fee box had to be recreated first (`/wallet/payment/send` of 0.05 ERG to the wallet's own address, tx `b72af2d2…`, height 1827030) — wallet payment txs consume token-free boxes, so a fee box must be recreated before each redemption test.

### Confirmed Transaction (height 1827032)

| Field | Value |
|-------|-------|
| Transaction ID | `85b14cc0dc5a0c65d009cf0c60159f7943caa0b39f6ccb5822e0f2890c92a273` |
| Inputs spent | Reserve `2ced1079…` (0.2 ERG), fee box `8e3b802b…` (0.05 ERG) |
| Output 0 (new reserve) | `9b7481643c8791be…` — 0.1 ERG, R5 digest `616188e2…` (entry: ts `1783796933524`, already_redeemed 0.1 ERG) |
| Output 1 (Bob's redemption) | `96e1f70c…` — 0.1 ERG (P2PK of Bob's key `03af13e3…`) |
| Output 2 (fee) | `3d81be40…` — 0.001 ERG |
| Output 3 (wallet change) | `b7f56bf6…` — 0.049 ERG (token-free) |

### Bug found: `/redemption/submit` did not sync local state

After this redemption the note still showed `amount_redeemed = 0.1 ERG` (should be 0.2 ERG) and the local reserve AVL tree was still empty while the on-chain reserve had the entry — the next redemption would have failed the reserve-selection R5 match. Root cause: `submit_redemption` only broadcast the transaction; it never updated the note or the reserve tree.

**Fix (this cycle):**

- `RedemptionBuildResponse` returns `new_already_redeemed` (the cumulative reserve-tree value proven on-chain, computed from the reserve-tree lookup, not the note record).
- `RedemptionSubmitRequest` carries `issuer_pubkey`, `recipient_pubkey`, `redeemed_amount`, `new_already_redeemed`; after a successful broadcast, `submit_redemption` sends `TrackerCommand::CompleteRedemption` with them. A state-sync failure is logged but does not fail the response (the tx is already on-chain).
- `complete_redemption(…, new_already_redeemed: Option<u64>)`: the note always accumulates `redeemed_amount`, but the reserve tree is synced to the explicit on-chain cumulative value when provided — the two can diverge (fresh reserves, repaired state) because each reserve has its own tree. Regression test: `test_complete_redemption_with_explicit_reserve_tree_value`.
- `POST /redeem/complete` accepts an optional `new_already_redeemed` (used for one-off state repairs); CLI (`transaction redeem-assisted`) and TUI pass the new fields through.

### State repair applied

`POST /redeem/complete` with `redeemed_amount=100000000, new_already_redeemed=100000000` → note `amount_redeemed = 0.2 ERG` (outstanding 0.2 ERG of 0.4 ERG), reserve tree entry `(ts 1783796933524, 0.1 ERG)` matching the on-chain R5. A subsequent CLI `redeem-assisted` dry run confirmed the build now selects reserve `9b748164…` (digest match) and generates proofs; it then fails local evaluation with `AvlTree: Incorrect insert` — the **known contract limitation** (`contract/basis.es:345` strict `insert`): the reserve already contains the note's entry, so further redemptions require a fresh empty-tree reserve until the contract switches to `insertOrUpdate`.

### Lessons Learned from the Sixth/Seventh Tests

1. **Broadcast is not state sync.** Any redemption path that mutates on-chain reserve/note state must update the tracker's note record *and* reserve AVL tree after broadcast, using the exact cumulative value proven on-chain.
2. **Note cumulative ≠ reserve-tree cumulative in general.** With multiple reserves per issuer (each with its own tree) or after state repair, the reserve-tree `already_redeemed` for a given reserve can differ from the note's `amount_redeemed`; the build must compute from the reserve-tree lookup and the submit must round-trip that value.
3. **Strict `insert` contract limitation resolved.** The reserve contract was updated to use `insertOrUpdate` for the reserve AVL tree (R5 flags `0x03`). Two consecutive redemptions against a single reserve are now possible; see the Eighth Redemption Test below. The old strict-insert P2S is still documented for historical reference.
4. **Fee boxes are consumable test fixtures.** Node-wallet payment txs consolidate/consume token-free boxes; recreate a ~0.05 ERG token-free fee box before each redemption test or `select_fee_inputs` fails with "no wallet boxes covering 1000000 nanoERG fee".
5. **The TUI is fully scriptable.** `basis-ui` reads line-based stdin (`read_line`); e.g. `printf '\n2\nr\n1\n100000000\nb\nq\n' | ./target/debug/basis-ui` drives a complete redemption non-interactively.

---

## Eighth Redemption Test (two consecutive 0.1 ERG redemptions against one 0.3 ERG reserve, mainnet)

Goal: verify the updated `insertOrUpdate` reserve contract supports sequential redemptions against the same reserve without needing a fresh empty-tree reserve.

### Setup

- Local Ergo node mainnet (`http://127.0.0.1:9053`, API key `hello`).
- Issuer/reserve owner: `022880fde8cace85c2c810fb32c5441a32198b0f7a122b9a672cfb7e50eb898cdc`.
- Recipient: `03dbc83fe0c803d370575d2a513247b741f2fe4fe45756cd9983ce087d788697a7`.
- Tracker public key: `024e564477ff457c601c01ad1cc31903f8b27b7d5e515bd03138891d8152d787b2`.
- Minted a new reserve NFT (tx `070fda838d6a019ed930370afb581ff9b834c5638ed4f7e371989d86220ab190`, token ID `e350a0f8112b7868f3c0ab56c04a10040e6afc037644e71f35e5f4b94f0ff254`).
- Deployed a 0.3 ERG reserve with the updated `insertOrUpdate` P2S (tx `d83e8b537f254ae0bad687fa4f9299410ade64636e33eae0b442f306386e68af`, reserve box `ddf6397150d1acca9a6a6986d8e6a2749d5e9732971692be1bd43b7292c51057`).
- Created a 0.2 ERG IOU note from issuer to recipient.
- The issuer's wallet only held token-bearing change boxes, so a token-free fee box was created first via `/wallet/payment/send` before each redemption.
- The old `InsertOnly` reserve (`acc414ec...`) still existed on-chain; the CLI reserve-selection code was updated to prefer the newest reserve when multiple reserves have the same collateral, avoiding the stale reserve.

### Transactions

| # | Tx ID | Inputs | Outputs |
|---|-------|--------|---------|
| First redemption | `b6d45ae533099f403b070a6706b1c2dd0a687d95d22d0a0c737149b5efa656f8` | Reserve `ddf63971...` (0.3 ERG), fee box (0.01 ERG) | New reserve `1c740dcc...` (0.2 ERG), recipient payout 0.1 ERG, fee 0.001 ERG, change 0.009 ERG |
| Second redemption | `bd95f930ec6a77fefa8c680b405fe69d72b6c6ee8ead8c79c1b20965764f7203` | Reserve `1c740dcc...` (0.2 ERG), fee box (0.009 ERG) | New reserve (0.1 ERG), recipient payout 0.1 ERG, fee 0.001 ERG, change 0.008 ERG |

### Result

Both transactions were accepted by the local node and confirmed on mainnet. The note was fully redeemed (0.2 ERG issued, 0.2 ERG redeemed, 0 ERG outstanding) and the reserve ended with 0.1 ERG collateral.

### Lessons Learned from the Eighth Test

1. **`insertOrUpdate` contract enables sequential redemptions.** The reserve R5 tree now uses flags `0x03` (insert + update), so a reserve can be updated multiple times for the same issuer/recipient pair.
2. **Reserve selection must avoid stale reserves.** When multiple reserves share the same issuer and collateral, the redemption builder should select the newest one (e.g., by `last_updated_height`), otherwise it may pick an older, un-spendable `InsertOnly` reserve.
3. **Token-free fee boxes remain required.** The wallet's consolidated change box contained tokens, so each redemption test required creating a new token-free fee box first.
4. **The tracker does not yet auto-detect on-chain redemptions.** After each redemption, the server state was advanced manually via `POST /redeem/complete` with the correct `redeemed_amount` and `new_already_redeemed`. The blockchain scanner currently updates reserve collateral but does not derive redemption events from reserve box changes.
5. **The tracker box updater and reserve output builder now both emit R5 flags `0x03`.** Consistency across the codebase is important: the tracker box updater previously used `0x01`, which would create a mismatch with the `insertOrUpdate` semantics.

---

## Ninth Redemption Test (scanner height-cache fix + note-timestamp desync repair, two 0.1 ERG redemptions against one 0.3 ERG reserve, mainnet)

Goal: reproduce the eighth-test flow end-to-end, fixing two operational bugs found along the way: (1) the reserve scanner stopped seeing new blocks, and (2) a repaired note state desynced the in-memory reserve AVL tree from the on-chain R5.

### Setup

- Same participants as the eighth test: issuer `022880fde8…`, recipient `03dbc83fe0…`, tracker `024e564477…`.
- Plain fee box: tx `606415c411b769cac4f8cb2c6be3f180a9bc00b459e1e815f18a1a49c7618607`.
- Reserve NFT: tx `6df8b04999e7ee973717cc2b6498a96117e350a99adda07bd154fdef443dfbeb`, token ID `018d29f4da1ea43f9d752b927200c54d9230637cc677c8a66d477f1684bd3098`.
- Reserve creation (0.3 ERG): tx `f3a6014870408cb9d84abe94d1cc6f4333e06cbd16ab0f2003af09085d402679`, reserve box `aa8340fc8ef5fa1ebb1edd31dd70d04f89f3b1357999f755a40e7d6b415185b6`.
- 0.2 ERG note from issuer to recipient (payment timestamp `1784494415054`).

### Bug 1: Reserve scanner stuck on cached blockchain height

**Symptom:** after the first redemption confirmed, `GET /reserves` kept returning the old spent reserve box. Server logs showed only `Starting reserve scanner background loop` and no further reserve updates for 10+ minutes.

**Root cause:** `reserve_scanner_loop` used `ServerState::get_current_height`, which caches the node height for 10 minutes **and persists the cache to disk** (`ScannerMetadataStorage`). After processing one block, the loop kept reading the same cached height, so `height > last_scanned_height` stayed false and `process_scan_boxes` never ran again.

**Fix (`crates/basis_store/src/ergo_scanner.rs`):**
- Added `ServerState::fetch_current_height` — always queries `/info` directly (and refreshes the shared cache for other callers).
- `reserve_scanner_loop` now uses `fetch_current_height` instead of the cached `get_current_height`.

After the fix, the scanner promptly detects each new block and persists reserve changes; the spent reserve is replaced by the new one within one scan cycle.

### Bug 2: Note timestamp desync broke the reserve-tree proof

**Symptom:** after a server restart, the second redemption's node-wallet signing failed with `Malformed request: null`. Incremental context-extension testing showed the failure appeared exactly when context var `#7` (reserve lookup proof) was present — i.e. the lookup proof did not verify against the on-chain reserve R5.

**Root cause:** the in-memory reserve AVL tree is lost on restart. Re-syncing it via `POST /redeem/complete` uses the note's **current** timestamp (`complete_redemption` refreshes `note.timestamp` on every call), but the on-chain reserve-tree value is `payment_timestamp || already_redeemed` with the note's **original** payment timestamp (`1784494415054`). With a mismatched timestamp the local tree root (`321a4ad5…`) diverged from the on-chain R5 digest (`f91e7e76…`), so the lookup proof failed on-chain.

**Repair:** reset the persisted note to `amount_redeemed = 0` and the original on-chain payment timestamp (one-off repair modeled on `basis_store/tests/fix_note_state.rs`), restarted the server, then re-ran `POST /redeem/complete` with `redeemed_amount = 100000000, new_already_redeemed = 100000000`. The local reserve-tree digest then matched the on-chain R5 exactly, and the redemption built and signed on the first attempt.

### Transactions

| # | Tx ID | Inputs | Outputs |
|---|-------|--------|---------|
| First redemption | `4b65c7cdf46ba7fdb741803e5d6534de70911f07a409c285532821054cc2959b` (height 1832709) | Reserve `aa8340fc…` (0.3 ERG) | New reserve `8dc21481ed3f084f99d021124c9923e418c1ece60f2d44f8885d48923b29dcd0` (0.2 ERG), recipient +0.1 ERG |
| Second redemption | `cfcd857961d06bfae66781dfe4a5a7f8581732a2c3e3f7f16571c979971d5863` (height 1833071) | Reserve `8dc21481…` (0.2 ERG), fee box (0.049 ERG) | New reserve `3ae59d54b1f57a9d…` (0.1 ERG, R5 `64d077c133e112d2d1…01032000`), recipient +0.1 ERG, fee 0.001 ERG, change 0.048 ERG |

### Result

Both redemptions confirmed on mainnet. The note is fully redeemed (`amount_collected = amount_redeemed = 200000000`, `redeemable = false`) and the reserve ends with 0.1 ERG collateral. After each redemption the server state was advanced via `POST /redeem/complete`, and the fixed scanner now keeps `GET /reserves` in sync with the chain (the spent `8dc21481…` was removed and `3ae59d54…` persisted within one scan cycle).

### Lessons Learned from the Ninth Test

1. **Never use a long-lived cached height in a scanner loop.** Any cache for `/info` must have a TTL far below the scan interval, or the scanner must bypass it entirely (`fetch_current_height`).
2. **Restarting the server loses the in-memory reserve AVL tree.** Re-syncing it requires the note's *original* payment timestamp, because the on-chain reserve-tree value is `payment_timestamp || already_redeemed`. Since `complete_redemption` refreshes `note.timestamp`, repeated `/redeem/complete` calls write later timestamps into the tree and desync it from the chain. A repair flow must reset the note timestamp first (as in `fix_note_state.rs`) or the API should accept an explicit timestamp.
3. **Node-wallet signing errors are diagnosable by adding context vars incrementally.** `None.get` (missing var) → `Script reduced to false` (vars present but proof/state mismatch) → `null` (var present but invalid against on-chain state, here the `#7` lookup proof).
4. **Recipient secret can be supplied without `--local-sign`.** `generate-redemption --recipient-secret <hex>` emits the unsigned tx with `secrets.dlog` populated, so the node wallet can sign even when the recipient key is not in the wallet.

---

## Tenth Redemption Test (automated integration runner, two 0.1 ERG local-sign redemptions against one 0.3 ERG reserve)

### Goal

Automate the Eighth/Ninth Redemption Test flow with a single self-contained script that:

- issues or reuses a reserve NFT,
- deploys a 0.3 ERG reserve,
- creates a 0.2 ERG IOU note,
- waits for on-chain tracker confirmation,
- creates a token-free fee box,
- runs two 0.1 ERG redemptions with `--local-sign`,
- advances tracker state after each redemption via `POST /redeem/complete`,
- verifies the note is fully redeemed.

### Script

`tests/test_local_sign_multiple_redemptions.sh` (Python runner: `tests/test_local_sign_multiple_redemptions.py`)

```bash
ISSUER_PRIVATE_KEY=<32-byte-hex-issuer-secret> \
  ./tests/test_local_sign_multiple_redemptions.sh
```

Optional overrides:

```bash
NODE_URL=http://127.0.0.1:9053 \
API_KEY=hello \
TRACKER_URL=http://127.0.0.1:3048 \
WALLET_ADDRESS=9fPRvaMYzBPotu6NGvZn4A6N4J2jDmRGs4Zwc9UhFFeSXgRJ8pS \
RESERVE_AMOUNT=300000000 \
NOTE_AMOUNT=200000000 \
REDEEM_AMOUNT=100000000 \
FEE_BOX_AMOUNT=50000000 \
  ./tests/test_local_sign_multiple_redemptions.sh
```

### Requirements

- A funded Ergo node wallet at `NODE_URL` (defaults to `127.0.0.1:9053`).
- A running Basis tracker server at `TRACKER_URL` (defaults to `127.0.0.1:3048`) with the reserve contract P2S and tracker NFT configured.
- `ISSUER_PRIVATE_KEY` set to the 32-byte hex secret of the reserve owner; the script imports it as a throw-away CLI account.
- `WALLET_ADDRESS` must be a wallet-owned P2PK address; it is used as the recipient/fee-payer and the destination for the issued reserve NFT. If omitted, the first address from `/wallet/addresses` is used.
- `RECIPIENT_PUBKEY` can be omitted; it is derived from `WALLET_ADDRESS` via `/utils/addressToRaw`.
- `basis_cli` local-signing currently derives P2PK addresses with mainnet prefix; run against a mainnet-equivalent node wallet so the derived recipient address is present in the wallet.

### Flow

1. **Build** `basis_cli` if missing.
2. **Import** the issuer secret into a temporary CLI config.
3. **Issue** a fresh reserve NFT to `WALLET_ADDRESS` unless `RESERVE_NFT_ID` is provided.
4. **Create** a 0.3 ERG reserve via `POST /reserves/create`, convert the payload to `/wallet/payment/send`, and submit it.
5. **Create** a 0.2 ERG note via `basis_cli note create --recipient <pubkey> --amount <amt>`.
6. **Wait** for `POST /notes/state` to report `status = confirmed` and `redeemable = true`.
7. **Fund** a token-free fee box via `/wallet/payment/send`.
8. **Redeem** 0.1 ERG with `basis_cli transaction generate-redemption ... --local-sign`.
9. **Complete** tracker state via `POST /redeem/complete` with `new_already_redeemed = 100000000`.
10. **Redeem** the second 0.1 ERG the same way.
11. **Complete** tracker state via `POST /redeem/complete` with `new_already_redeemed = 200000000`.
12. **Verify** `POST /notes/state` returns `redeemable = false` and `redeemable_amount = 0`.

### Setup Details

- Network: mainnet
- Ergo node: `http://127.0.0.1:9053` (local mainnet node)
- Tracker server: `http://127.0.0.1:3048`
- Issuer / recipient: `02725e8878d5198ca7f5853dddf35560ddab05ab0a26adae7e664b84162c9962e5` (address `9fPRvaMYzBPotu6NGvZn4A6N4J2jDmRGs4Zwc9UhFFeSXgRJ8pS`)
- Fee payer / tracker box owner: `03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea` (address `9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73`)
- Tracker NFT: `b159ad5c9062ec4c3f83cc478f1580f8312fd13439868551cc5905bc3c0ef42f`

### Transactions

| # | Tx ID | Height | Inputs | Outputs |
|---|-------|--------|--------|---------|
| Reserve NFT issuance | `91f9da52a510707da67f6839de43ca999f9a222f9c657f35f1fc3cfcc2c07c14` | 1842385 | Wallet input | NFT box `d87a6f5a314c62a3b83b2d5c4bd8ad36f35dab1f5ab0611bddc364dce1e8559b` holding token `44a901792054559ea73e1509634ec159564b606047cec3e62f280051c8cba6af` |
| Reserve creation | `f759077c0d951b3f9088c7785b2fa284699c0a16d1d016e59a7b909f500991ff` | 1842387 | Wallet inputs | Reserve box `414d8e83045066f8615cc7b6c4109d32c48466e3da4930fd5fd30a06623e6e85` (0.3 ERG) |
| Tracker update | `3c53d0c55282f4f8ddba32a3f2b6e4a3dfd52b10f390f6125f5eefc12fd6e5a6` | 1842408 | Old tracker box | New tracker box `0be254c7a3f39afa5eef85a8adff503359582c657b735d570c5dbf442170186b` (R5 digest `2d149875…`) |
| First redemption | `73c89a30dca3a1a85fc5eed25b8f170cf74841926fbf8f6e8a4230a7fa93cf31` | 1842764 | Reserve `414d8e83…` (0.3 ERG), fee box `ccdc74f8…` (0.049 ERG), data input tracker `0be254c7…` | New reserve `9034e962…` (0.2 ERG), recipient payout `9a6987b7…` (0.1 ERG), fee `4559ca71…` (0.001 ERG), change `7ff05e12…` (0.048 ERG) |
| Second redemption | `816349dfea63e7d39434d55eef04fc74ff0dabbfed5628fd013283bfa8217307` | 1842767 | Reserve `9034e962…` (0.2 ERG), fee/change box `7ff05e12…` (0.048 ERG), data input tracker `0be254c7…` | New reserve `aac7c1f2…` (0.1 ERG), recipient payout `6937708c…` (0.1 ERG), fee `aaeb520b…` (0.001 ERG), change `314161b4…` (0.047 ERG) |

### Result

| Field | Value |
|-------|-------|
| Reserve NFT | `44a901792054559ea73e1509634ec159564b606047cec3e62f280051c8cba6af` |
| Reserve creation tx | `f759077c0d951b3f9088c7785b2fa284699c0a16d1d016e59a7b909f500991ff` |
| Reserve box | `414d8e83045066f8615cc7b6c4109d32c48466e3da4930fd5fd30a06623e6e85` |
| First redemption tx | `73c89a30dca3a1a85fc5eed25b8f170cf74841926fbf8f6e8a4230a7fa93cf31` |
| Second redemption tx | `816349dfea63e7d39434d55eef04fc74ff0dabbfed5628fd013283bfa8217307` |

Both redemptions confirmed on mainnet. The reserve collateral decreased from 0.3 ERG → 0.2 ERG → 0.1 ERG, and the note was fully redeemed.

### Lessons Learned

1. **Tracker box is now wallet-owned and must be protected from fee selection.** The tracker address `9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73` holds the tracker NFT. The integration script explicitly excludes any wallet box containing the tracker NFT when selecting fee/issuance inputs, so the tracker box is never accidentally spent.
2. **Recipient and fee payer can be different wallet addresses.** The first redemption's recipient payout went to `9fPRvaMYzBPotu6NGvZn4A6N4J2jDmRGs4Zwc9UhFFeSXgRJ8pS`, while the fee input and change output used the tracker address `9hnupHc2udAoa7SV2UrWAba3N7pu9tR4RX662wv2iFa9gMn1E73`. `basis_cli --local-sign` fetched both secrets from the same node wallet.
3. **Automating the wait for tracker confirmation is the slowest part.** The background updater runs every 10 seconds in the current server configuration, so the script polls `POST /notes/state` until the note is confirmed.
4. **A token-free fee box must be created before the first redemption.** The reserve-creation transaction consumes the wallet's token-free boxes; the script creates a fresh 0.05 ERG fee box before the first redemption, and the first redemption's change output funds the second.
5. **The tracker state must be advanced after each on-chain redemption.** `POST /redeem/complete` carries the cumulative `new_already_redeemed` so the next redemption's reserve lookup/insert proofs match the on-chain reserve R5.
6. **Local-signing depends on the node wallet containing the recipient and fee-payer keys.** The CLI derives the mainnet P2PK address from the recipient public key and fetches the secret from the node wallet; ensure the wallet was created on the same network.

---

- [Tracker Box Update Specification](server/tracker_box_update_spec.md)
- [Redemption Transaction Format Specification](server/redemption_transaction_format_spec.md)
- [Redemption State Specification](server/redemption_state_spec.md)
- [Interactive Demo](interactive_demo.md)
- Ergo node API: `/wallet/transaction/sign`, `/transactions`, `/wallet/getPrivateKey`
