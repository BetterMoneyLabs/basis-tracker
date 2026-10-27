# Token-Backed Basis Reserves

> **Superseded by `specs/ergo/multi_collateral_reserves.md`.** The
> mutually-exclusive single-token mode described below no longer exists: a tracker
> now serves ERG plus any number of configured tokens simultaneously, debt records
> are keyed per denomination, and the token contract's debt-tree key carries the
> token id. This document is kept for historical reference; the configuration and
> API it describes (`reserve_token_id`, `reserve_token_decimals`,
> `is_token_reserve_mode`) were removed.

A Basis tracker instance can be configured to back reserves with a single custom
Ergo token instead of ERG. This is a per-tracker, mutually-exclusive setting:
one tracker operates either in ERG-reserve mode or in token-reserve mode, never
both at the same time.

The first supported example token is **USE (Dexy USD)**:

- Token ID: `a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669`
- Decimals: `3` (as reported by the Ergo node token registry)

All on-chain amounts remain raw token units; `reserve_token_decimals` is used
only for display/conversion helpers in the CLI and UI.

## Configuration

Add the following fields under the `[ergo]` section of `config/basis.toml`:

```toml
[ergo]
# ERG-backed reserve contract (always required as a fallback / compatibility field)
basis_reserve_contract_p2s = "2XoR96YVZfWybxeo7qdMkA1BRhxFivm8gTuLLuKREiW6833XWK4mA43DrJMDCvLqB9AwyduGgHBnGmmg2jrkUZAunRDu7t8718EkwiJTvp9rtEWEXMMJeMStjQB5BeuxsUc9ujW3aCMvxdS8VeYyZ14B8sSGu7CwJFmeaZDHiftsT3tUBqBfgau4wZTvhEtYEsFsqvpwsjcPAe4aGhFzJNSPhKqdSBrFKee3Dg4qFmGhb8Wu6DsdmhTrW6jnucnf17PDi2rQ9pgJvKrfs2KPbeZf9gV89vEga4kZk4u3bbSFTVk3s9EhuoUmfyhvC3ZaNv9DSPtzCcjvnL7x6smyLigsgb4QSEDnzX4HjJg5CfA6mh4Jn9hoL1412EVnY2xb8cYz76VEgU8J12n4DHrVsV9NzYAHxe6ogVNptUbAgykiru81pz3aZ6LfYySJt6ESVVGxm6i49ScGjawqsKCxjykCndh3MNQnRajZMgQMH2qFZhbd9KXCnLQkS6NAjtNfMVcx1KykdiSrLVTu3nphUZhT9MjhfxtZcojSZoQmGAy95zhAJpDMgUXBfuCSoUFUzPAuSMv2pBuEXiUR3wDQC7pDFYUDy68QCtTUTDtn9cYyNgc8EZWQSB76wsKhCNod1Q9RV85piLF5PJDMBXh4M8gjEzsq244xDdW6t2f4W4A2ZVDNw2gdSU3FHYxfB45c5cxkyR5qrXyoJT56c2STfFm3vK1ZfXzvckq48uAE1QRGpM9xALiXtdTx1iQaQjy2LkaURha6ewN5NDufyqntwBPk72NLadL8BDtLyReRtUWqbFZo94h4prjHVT25ABHbAqiJuTCHvmfTqi2V5gDwRW7oLk4gvBwswxJVvKmuKSG6sH5HAzQGoF6Kwcwdvq8Lbv5RbuAnm3N7hLnS1rBUWKXfaJCJjptMkeKxzMPhjfjnZTkMXwW3jSBxhpLeuWxrBrfjTFaVR93erQFVTvF9C1FsULgqUPjzPmcHn5Drp4cp4Y2pjv793k6KdykbugL6M6wt75hmKG5jsmrfsSL6AN13j3"

# Token-backed reserve contract (required when reserve_token_id is set)
basis_token_reserve_contract_p2s = "EUNip3sGUdzr5Leg8rRwRMyZGUis42syLiBYwsj5JbmrVNAh41LUzNFnFTEzpEvf3jMUzJdyfkJi6aVjbs6NLskmHiuCPYwcRFjdqfBwc9NXUnf4hv3heFRDnzXDWK92KAz8JpXSx9Rnd2b4HnkBq8HzbnfSRpRVQqRJZqQaVoceZqcT87sBTNii4Hu6zUPrGaQwsR3QFnNjoKsDrALMTELk8eUcxHhyWbJPPcSYWdf5nFTskqt5F2yZ5FBsKfkCsxskM31FKXRXt7RAWrJWA6QTUzrQT2kk4a8G1G7AYrXRFc2kXHGTH9bjXaNvM4JRu1CkDb2hqLzt5jouzT1UdZkCHqG9LwWa7ULSDfzvdgybQhBv8UC9LnHegQ7FEYyqHhL4ZZpsqr8cGbznE1QCWWbxcGLKWoDhCCt3j6Bv36AoaTPRcffevReEWMk7FTE6s2n391CLV6W8G2HS2zkStnmAKDmdardZJJ2tb3zQPrqM5Y2umufxDV9VCj6t18XBPSNDF8YtPXfwkPLUAyfeEE8JM4wCPNQ33vvH8irVphUagT92U5jLp9B8Z54ceJrLFodaj74kxECULy9ufmC6jGDNbeGj6AM6pxV6HBPgYpBNw8Qupc35se4YW7odPjErov8pZuVHnGusNu4huP1LFm2VL4aqMnLpriHTvx61DG7SCEUxZhEjWtnS5xxnp67fAzibCckBsKZeFsqEGro8pJnS1ZiPAwggz2dLorMoK285Rx8QZNLiRzfULwnWzSGgtoqKZCkKc7QJXiEzivAe4FHC32grsU2KvAb9sv79ardmpgpJTmbNLRAWiDwBXbTLGbF5AJkCcfPCvEoiaLCRERVYpJRZ4XZHo5A14eCFgwvUhDtPpZeJJXqNgGmsbaXZGPXKwdzaE8mA9uZ3ANcZ3KrdYL5HV7w7wrNNMWNzvBSfzM3uC5LiZmaxuWNph9YFLXHEbEfbhjXvGeAMZTGKntErw7NobRPQB6yCVeGMUk5HCDxJxkHAbn9oLxC2Nv3Wpp4cVMf6ixeTS5qL7he36pqkNPAcspPGHFJEajbs4fkaQd9wTgtbGCa7CXAWaP5s1S5pWCBSjnYkgxhjRYa6CwCnnV1u9ksA3Gb3PBr6Z8T2wicYqW925b3wxzNJKs5qotwCDzYthZjiGHGdtFA1vg32vKyGbeoPCXMgmn9uh1dcmnu2m7GVBEjQZ7LYHvuPZp34ykfTTFA"

# The token that backs reserves on this tracker. When empty or omitted, reserves are ERG-backed.
reserve_token_id = "a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669"

# Decimal places of the reserve token, used for display only.
reserve_token_decimals = 3
```

### Semantics

- `reserve_token_id` — hex-encoded 32-byte token ID. When set and non-empty,
  the tracker runs in **token-reserve mode**.
- `basis_token_reserve_contract_p2s` — P2S address of the token-aware Basis
  reserve contract (`contract/basis-token.es`). Required in token-reserve mode.
- `reserve_token_decimals` — display helper; raw amounts stored and sent to the
  contract are always integer token units.

A tracker instance cannot simultaneously create ERG-backed and token-backed
reserves. Existing ERG reserves on chain are still tracked, but new reserves are
built using the configured token contract and token collateral.

## Creating a Token Reserve

### API

`POST /reserves/create`

```json
{
  "nft_id": "<64-hex reserve NFT ID>",
  "owner_pubkey": "<66-hex issuer public key>",
  "erg_amount": 2000000,
  "token_amount": 1000000,
  "token_id": "a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669"
}
```

- `erg_amount` is still required to cover the box's minimum ERG value and fees.
- `token_id` must exactly match the tracker's configured `reserve_token_id`.
- `token_amount` is the raw token units to lock as collateral.

### CLI

```bash
basis-cli reserve create \
  --nft-id <reserve-nft-id> \
  --owner <owner-pubkey> \
  --amount 2000000 \
  --token-id a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669 \
  --token-amount 1000000
```

Add `--submit` to broadcast the generated payload through the tracker's Ergo
node.

### Example for USE

Locking `1 000.000 USE` requires `--token-amount 1000000` because USE has
3 decimals. The CLI status view will display this as `1000.000 USE`.

## Scanning and Collateral

In token-reserve mode the scanner treats the configured token's balance in a
reserve box as the reserve's collateral, not the ERG value. The ERG in the box
is ignored for collateralization calculations.

The tracker stores the reserve token ID together with each `ReserveInfo`, and
`KeyStatusResponse` exposes it as `reserve_token_id` so clients know whether a
reserve is token-backed.

## Redemption

The redemption transaction builder detects token-backed reserves and constructs
an unsigned transaction that pays the redeemed amount in the reserve token to
the recipient, preserving the reserve NFT and remaining collateral tokens in
the reserve output.

Emergency redemptions and the 65-byte Schnorr signature format are unchanged
from ERG-backed reserves; only the output asset type differs.

## Reserve Status

`basis-cli reserve status --issuer <pubkey>` prints:

- `Total Debt` and `Collateral` in raw units.
- `Collateralization Ratio` as `collateral / debt`.
- `Reserve Token ID` when the reserve is token-backed.

For ERG-backed reserves the CLI also prints ERG-converted values; for token
reserves it leaves conversion to the user (or UI) using
`reserve_token_decimals`.

## TUI Wallet Support

The interactive TUI wallet (`crates/basis_app`) is token-reserve aware:

- On refresh it fetches `GET /config/reserve-token` and caches the configured
  token ID and decimals in `App::reserve_token_config`.
- `My Reserves` shows the reserve token ID for token-backed reserves and
  displays liabilities/collateral in raw token units plus a converted value
  using `reserve_token_decimals`.
- `Create Reserve` prompts for an ERG box-value amount and, when the tracker is
  in token mode, an additional token amount to lock. The configured token ID is
  sent automatically.
- Wallet stats, note lists, note creation, redemption, and acceptance-policy
  labels switch from "nanoERG"/"ERG" to "units"/"token" when the tracker is
  configured for token reserves.
- The USE token (`a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669`)
  is displayed with the `$` symbol in the TUI (e.g. `1000.000000 $`). Other
  custom tokens keep the generic "token" label. The display still uses the
  configured `reserve_token_decimals` for unit conversion.

## Testing

Relevant test suites:

```bash
cargo test -p basis_server create_reserve_tests -- --nocapture
cargo test -p basis_store ergo_scanner -- --nocapture
cargo test -p basis_store transaction_builder -- --nocapture
```

The TUI token-reserve display helpers have dedicated unit tests in
`crates/basis_app/src/ui.rs` covering:

- `asset_label` returns `$` for USE, `token` for other token modes, and `ERG`
  for ERG mode.
- `amount_label` returns `units` in token mode and `nanoERG` in ERG mode.
- `decimals` falls back to 9 in ERG mode and respects
  `reserve_token_decimals` in token mode.
- `format_units` handles zero-decimal, 3-decimal, and 6-decimal conversions.
- USE token ID matching is case-insensitive.

Run the TUI helper tests with:

```bash
cargo test -p basis_app ui::tests -- --nocapture
```

All workspace tests should pass after enabling token-reserve support:

```bash
cargo test --workspace
```
