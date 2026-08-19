# Basis Real-USE Token 4-Redemption Test Report

## Summary

This report documents a successful mainnet test of the Basis protocol using the real **USE (DexyUSD)** token as reserve collateral. The test created a token-backed reserve with **0.5 USD** collateral and performed **four consecutive 0.1 USD redemptions**, with a tracker server restart after the second redemption.

**Result:** All four redemptions were broadcast and confirmed on-chain. The reserve collateral decreased from 500 USE → 200 USE, and the IOU note was fully redeemed.

---

## Environment

| Component | Value |
|-----------|-------|
| Network | Ergo mainnet |
| Ergo node | `http://127.0.0.1:9053` |
| API key | `hello` (local test only) |
| Tracker server | `http://127.0.0.1:3048` |
| Transaction fee | 1,000,000 nanoERG (0.001 ERG) |

### Token Under Test

| Field | Value |
|-------|-------|
| Name | USE (DexyUSD) |
| Token ID | `a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669` |
| Decimals (on-chain) | `3` |
| Reserve collateral | `500` raw units (= 0.5 USD) |
| Note total debt | `400` raw units (= 0.4 USD) |
| Per-redemption amount | `100` raw units (= 0.1 USD) |

**Note:** The on-chain token registry reports `decimals = 3` for this USE token. The earlier project documentation assumed 6 decimals; this test used the actual on-chain value.

---

## Participants

| Role | Public Key (compressed secp256k1) |
|------|-----------------------------------|
| Issuer / reserve owner | `033b0ae7905a7fa3d9bf6207a62747bf8d01b56a1253a4546362a9eec27c69f0bb` |
| Recipient / creditor | `03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea` |
| Tracker / wallet | `03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea` |

A **fresh issuer keypair** was used for this test. Re-using the issuer from the earlier test-token reserve failed because the server-side reserve AVL tree still contained redemption entries for the old token, causing the on-chain reserve insert proof to mismatch the empty R5 digest of the new reserve.

---

## Setup Transactions

| Step | Transaction ID | Block | Description |
|------|----------------|-------|-------------|
| Split tracker box #1 | `d42614822f243099ec75c8e808747ce41f1e0b1c72b8632c41f93ff317630d89` | 1854518 | Created a token-free ERG fee UTXO from the tracker box. |
| Split tracker box #2 | `05c70cb39e19112048de52dade2b961733c2f9882497e6f686fbc71f7c820da6` | 1854528 | Created a larger 10 ERG plain fee UTXO. |
| Issue reserve NFT | `1aa1de90408076c61d6709f91dda49ce28623752aa80ce398142c92966805b99` | 1854529 | Minted the reserve NFT used in the new reserve. |
| Create USE reserve | `95fffa62d62c1e14e8702e2481ee5083842c80a682cea0dc11b706ee6a1299b1` | 1854533 | Locked 500 USE + 3,000,000 nanoERG in the reserve contract. |
| Tracker note commit | `c996c61726878f3654fa54905a0b8d0cdf1a378b561c43d264cab9476b531fd8` | 1854537 | Tracker box update that committed the 400 USE note on-chain. |

**Initial reserve box:** `3c96b87a7cba776c0bb6187216da6bf85f3c2288d7fe640270fd5e1cb5a3ed05`  
**Tracker NFT:** `f7f55159e34ba09c7fd0d31d707008420188829bad35ade8480f3fd11f35fc91`  
**Reserve NFT:** `9fa7073f03adeeb0934cd21b63d03c1a360e684a11b889252cbf17d91f872303`

---

## Redemption Execution

| # | Transaction ID | Block | Reserve collateral before | Reserve collateral after | Notes |
|---|----------------|-------|---------------------------|--------------------------|-------|
| 1 | `684fc8286b116899b6f827761d3e5c7bc463b5ac5b2a984d6b1fc2e4041e8e15` | 1854540 | 500 USE | 400 USE | First redemption, insert proof. |
| 2 | `e406f61f3f076ea6640d1c8fa3c177808de1660c41d2be220e73d93409e0161c` | 1854542 | 400 USE | 300 USE | |
| — | *Tracker server restarted here* | — | — | — | To satisfy the test requirement. |
| 3 | `740860e2288f979d9ac263e0eb727292614797fb80e8ea241aa8585ff14abfb0` | 1854544 | 300 USE | 200 USE | Waited for scanner to catch the new reserve box after redemption #2. |
| 4 | `70a017f71580043dd76047a2ada2e9b7d994f26bafb8ed4474e995727a380e1e` | 1854545 | 200 USE | 100 USE | Final redemption; note fully redeemed. |

---

## Final State

| Field | Value |
|-------|-------|
| Final reserve box | `4ed2b66ebcead2eb6c11cdde5b67166af1315943814161801831522e4080e919` |
| Remaining collateral | `100` raw USE (0.1 USD) |
| Note `amount_collected` | `400` raw USE |
| Note `already_redeemed` | `400` raw USE |
| `redeemable_amount` | `0` |
| Status | `confirmed`, fully redeemed |

---

## Operational Findings

1. **Fee-box collisions with the tracker updater.** During earlier tests the tracker updater spent a fee input that a redemption transaction had already selected, causing the redemption to be dropped from the mempool. For this run the server was started with `BASIS_TRACKER_UPDATE_INTERVAL_SECONDS=3600` during redemption phases so the updater could not interfere.

2. **AVL-tree state isolation per issuer.** Switching a tracker from one reserve token to another while keeping the same issuer public key caused the server to generate reserve proofs against a non-empty reserve AVL tree, while the newly created reserve box had the empty-tree R5 digest. Using a fresh issuer keypair resolved the mismatch.

3. **Tracker update confirmation is the bottleneck.** Notes remain `local_only` until the tracker updater broadcasts a tracker box update and that update reaches `min_depth` confirmations. On a slow mainnet this required waiting for multiple blocks; redemptions cannot proceed until the note is confirmed.

4. **Scanner lag after redemptions.** Immediately after a redemption the server's selected reserve box can be one block behind. Attempting the next redemption before the scanner updates returns a spent reserve box and fails with a 404 from `/utxo/byId`. Waiting for `/reserves/issuer/{pubkey}` to report the new `box_id` avoids this.

---

## Recommendations for Future Runs

- Pre-create one or more token-free ERG boxes and keep them reserved for redemption fees.
- Run the tracker with a long update interval (or pause updates) during bursts of redemptions.
- Add wallet UTXO locking during `/redemption/build` so selected fee inputs cannot be spent by another process.
- Support batched redemptions in a single transaction to reduce fees and confirmation latency.
- Allow an optional higher transaction fee / RBF path for faster confirmation during mempool congestion.
- When changing `reserve_token_id`, either use a fresh issuer keypair or explicitly reset the server's reserve AVL tree state for that issuer.
