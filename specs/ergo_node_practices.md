# Ergo Node Practices for Basis Tracker Testing

This document collects practical lessons for interacting with a local Ergo node when running Basis protocol tests on mainnet (or testnet). It covers wallet checks, transaction submission formats, token issuance, fee handling, mempool monitoring, and the reserve/note/redemption flow.

## 1. Node and Wallet Health Checks

Before any transaction, verify the node and wallet state:

```bash
# Node info
curl -s -H "api_key: hello" http://127.0.0.1:9053/info

# Wallet status
curl -s -H "api_key: hello" http://127.0.0.1:9053/wallet/status

# Wallet balances
curl -s -H "api_key: hello" http://127.0.0.1:9053/wallet/balances

# Unspent boxes (useful to see plain ERG vs tokenized boxes)
curl -s -H "api_key: hello" \
  "http://127.0.0.1:9053/wallet/boxes/unspent?minConfirmations=0&maxConfirmations=-1"
```

Key fields to inspect:
- `info.fullHeight`: current chain height
- `info.unconfirmedCount`: mempool size
- `wallet.status.isUnlocked`: must be `true` for signing
- `wallet.status.changeAddress`: default change address used by the wallet

## 2. Two Wallet Endpoints for Transactions

The Ergo node exposes two related endpoints:

- **`POST /wallet/transaction/send`** — accepts a `RequestsHolder` object and supports
  payment requests, token burns, and **asset issuance** (`AssetIssueRequest`).
- **`POST /wallet/payment/send`** — accepts a JSON array of `PaymentRequest` objects
  (simpler, default fee of 0.001 ERG is used).

For reserve creation and most test flows, use `/wallet/payment/send` with a
`PaymentRequest` array.

## 3. Issuing a Reserve NFT

A reserve NFT is a token with **amount = 1** and **decimals = 0**. Use
`/wallet/transaction/send` with an `AssetIssueRequest`:

```bash
curl -s -X POST http://127.0.0.1:9053/wallet/transaction/send \
  -H "api_key: hello" \
  -H "Content-Type: application/json" \
  -d '{
    "requests": [{
      "amount": 1,
      "name": "Basis Reserve NFT",
      "description": "Reserve NFT for Basis test",
      "decimals": 0,
      "address": "9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ",
      "ergValue": 1000000
    }],
    "fee": 1000000
  }'
```

Response is a plain JSON string containing the transaction ID.

After the transaction confirms, the new token ID is the **first input box ID** of the
issuance transaction. Query it with:

```bash
curl -s -H "api_key: hello" \
  http://127.0.0.1:9053/blockchain/transaction/byId/<txid>
```

and read the `assets[0].tokenId` of the output box that carries the NFT.

### Notes
- Token issuance transactions can be large if the input box carries many tokens, which
  can delay confirmation. Use a dedicated plain-ERG input when possible.
- Wait for confirmation before using the NFT in a downstream reserve transaction.

## 4. Creating a Plain ERG Box for Fees

Basis tracker background tasks (e.g. the tracker-box updater) only select wallet boxes
that contain **no assets** when paying fees. If the wallet only has a single large box
with many tokens, create a plain ERG box first:

```bash
curl -s -X POST http://127.0.0.1:9053/wallet/payment/send \
  -H "api_key: hello" \
  -H "Content-Type: application/json" \
  -d '[{
    "address": "9hNQcqi72NB5u5Tw6tbfCGbEKByguR7njvcyZXnXPLvV3Do1DiJ",
    "value": 500000000
  }]'
```

Keep at least one such plain box (e.g. 0.3–0.5 ERG) available so the updater can pay
its 0.001 ERG fee each cycle.

## 5. Reserve Creation Flow

1. Call the tracker server to build the reserve payment payload:

```bash
curl -s -X POST http://127.0.0.1:3048/reserves/create \
  -H "Content-Type: application/json" \
  -d '{
    "nft_id": "08c9d7a2c43676f3f6e25f3fe713314a89c4ce3430941887889ff8e4b285f594",
    "owner_pubkey": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
    "erg_amount": 500000000
  }'
```

2. The server returns a `ReserveCreationResponse` with `requests`, `fee`, and
   `change_address`. Convert the response to the Ergo `/wallet/payment/send` format:
   - Replace `token_id` with **`tokenId`** (camelCase).
   - Pass only the `requests` array to `/wallet/payment/send`.

```bash
curl -s -X POST http://127.0.0.1:9053/wallet/payment/send \
  -H "api_key: hello" \
  -H "Content-Type: application/json" \
  -d '[{
    "address": "<reserve_p2s_address>",
    "value": 500000000,
    "assets": [{ "tokenId": "<reserve_nft_id>", "amount": 1 }],
    "registers": {
      "R4": "<owner_pubkey_with_07_prefix>",
      "R5": "<reserve_avl_digest>",
      "R6": "0e20<tracker_nft_id>"
    }
  }]'
```

3. Save the returned transaction ID and wait for confirmation.

### Change Address Warning
The server response may contain a `change_address` derived from the server
configuration (e.g. the tracker public key). If that address is **not** controlled by
your Ergo node wallet, the change output will become unspendable. For test wallets,
verify that the wallet owns the change address, or override the payload to use the
wallet's own change address.

## 6. Mempool Monitoring and Confirmation

After submitting a transaction, check the mempool and confirmations:

```bash
# Mempool contents
curl -s -H "api_key: hello" http://127.0.0.1:9053/transactions/unconfirmed

# Check a specific transaction
curl -s -H "api_key: hello" \
  http://127.0.0.1:9053/blockchain/transaction/byId/<txid>
```

Confirmation times vary with network congestion. The local mempool can be busy even
when `unconfirmedCount` is moderate; wait until `/blockchain/transaction/byId` returns
200 with `inclusionHeight` and `numConfirmations`.

If a transaction is stuck for many blocks, possible causes are:
- Low fee relative to transaction size.
- Input box is already spent in another pending transaction.
- The transaction is invalid and has been rejected by miners (but the node may still
  keep it in the unconfirmed pool).

## 7. Creating an IOU Note

After the reserve is confirmed on-chain, create the note through the tracker server:

```bash
curl -s -X POST http://127.0.0.1:3048/notes \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83",
    "recipient_pubkey": "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea",
    "amount": 400000000,
    "timestamp": <ms_since_epoch>,
    "signature": "<130_hex_char_schnorr_signature>"
  }'
```

The server stores the note in its local AVL tree. The background updater will detect
the new digest and submit a tracker-box update transaction every 10 minutes (the default
interval). A note is only redeemable after the tracker-box update confirms and the
server marks the note as `Confirmed`.

Monitor the tracker state with:

```bash
curl -s http://127.0.0.1:3048/tracker/state | python3 -m json.tool
```

## 8. Redemption

After the note is confirmed (`confirmed_digest` matches `local_digest` on
`/tracker/state`), generate the redemption transaction with the CLI:

```bash
./target/debug/basis_cli generate-redemption \
  --issuer-pubkey 0377709166937fcdc08bf7e841b31684e2377f489914c97ef7148de14d9c6e1f83 \
  --recipient-pubkey 03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea \
  --amount 200000000
```

The CLI produces an unsigned transaction compatible with the Ergo node's
`/wallet/transaction/sign` endpoint. Sign and broadcast it:

```bash
curl -s -X POST http://127.0.0.1:9053/wallet/transaction/sign \
  -H "api_key: hello" \
  -H "Content-Type: application/json" \
  -d @<unsigned_tx.json>

curl -s -X POST http://127.0.0.1:9053/transactions \
  -H "api_key: hello" \
  -H "Content-Type: application/json" \
  -d @<signed_tx.json>
```

The `/transactions` endpoint returns the transaction ID as a plain JSON string.

## 9. Common Pitfalls

| Issue | Cause / Fix |
|-------|-------------|
| "No wallet boxes available to pay transaction fee" | The wallet lacks a plain ERG box (no assets). Create one with `/wallet/payment/send`. |
| `token_id` rejected by `/wallet/payment/send` | The Ergo node expects camelCase `tokenId` in the `assets` array. |
| `/transactions` returns a string, not an object | The response is a plain JSON string like `"<txid>"`. Parse it as a string. |
| Reserve scanner fails to parse R6 | R6 is a `Coll[Byte]` constant (`0e20` + 64 hex chars), not a `u64`. |
| Tracker update never submits | The updater only runs every 10 minutes and only when the AVL digest changes. Ensure a plain fee box exists. |
| Tracker box accidentally spent as a fee input | The tracker box now holds the tracker NFT in a wallet-owned address. Exclude it from fee-input selection; it must be preserved as a data input for redemptions. |
| Redemption fails before confirmation | Notes must be `Confirmed` (tracker R5 digest matches local digest). Check `/tracker/state` and `/notes/state`. |

## 10. Summary Checklist

- [ ] Node is synced and wallet is unlocked.
- [ ] A plain ERG box (no assets) is available for fees.
- [ ] Reserve NFT is issued (amount=1, decimals=0) and confirmed.
- [ ] Reserve creation payload is converted to camelCase `tokenId` and submitted via `/wallet/payment/send`.
- [ ] Reserve transaction is confirmed on-chain.
- [ ] Note is created and signed, then submitted via `POST /notes`.
- [ ] Tracker-box update confirms (check `/tracker/state`).
- [ ] Redemption transaction is signed and broadcast.
- [ ] Redemption is confirmed and reserve collateral is reduced.
