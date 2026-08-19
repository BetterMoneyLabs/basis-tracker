# Token-Backed Basis Reserves

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
basis_reserve_contract_p2s = "3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT"

# Token-backed reserve contract (required when reserve_token_id is set)
basis_token_reserve_contract_p2s = "96HrjMftJd4NbjzufhMHXyZqzaUbdc5zUqtSySUnyEZoHogM9TD9RsRNkhq83tGaWTH4ZiFeDKzAdHkQyi1SWwMdkKtDaDhobhsrb5tjEDZkYhhF5aPGf3b4fUo3W23tec9N7GyzA6buyhRYzf3DqVWRsJGzCEdYeDfcMYKXon3wMxMFgQPX4FUbCAGe32dCd1aiFwQwx4Rnjy19G6qPaACdZvfSNFLgM1ur3U6HSSMX22g5o8MbbxzNeKScWFedW76z1zCmC38BrgiSv1qC7395yi9y4dDy2y26Tgrc2MPxvned5j1F3cTTTY9HVPYcS9U4vQocDRHufEkMG4dyu7eQqiHbqa6962B1jKQUMofyNV2mehQDrTzfzT5yHPsTeAGMTbDHeDECsmmJ2ideonha9VBuEP6fivixWjej43spbMZDcy3SCNa5gTrLuhh8gN4j8CbMncTXYghrxNdRqPbiUfJy8rMpcdeRaDnbodibGAyJqzzh95ZC6FHPwL7UQZbFbc472WSPtPRXU9g7QpZasdYtZb4GrHqvUJASsLwgYsRevJ4QZ24nrcUXv5ttziaVsGfeCW35viGB5zc3Sxk8e37smwQzWPfAGvsLfgy98dJfmNFyQUCNUyGX5qq4uJrKNaRamouL4S1VNFcgVaJTdGDTT7ykcy6qLmaFg6tUiAsLLC2p4GGB83fqsw13hfmxCxMGuFQUFu6aNc9W31wMMJ2beCN7Xb7cUkffzEXZnjiqFKMicTHsmvLz2mZuAgiKykrxqNTMqmTd2DjGsuYmQNtLXqS4j5d9qXnSVxmeVGp3cqcC7NYF9ujvLT38yqfnk5tZ5X9gnMXgkkKk8MfuUgY5a2ccSgbetkj7yNk1ciHK1QYGrvydDPWB2kJEQb9yAb72Uf1jZXVH1kSAr7vv1BBRQWveJcsjD6GQVKXJrLTGLrR5XV1uYK7uQSR4XYEi5yJsjk4rPMCDtt2FDsrUk7bo9suFWq2sqZxQZZWEgoKkwog6DtvyX4cyMpT45eeqjctBTjnAnJRf8CgBuyEhfU8RMfG7dPNVk9EWS3JbGmwow4R95Ae5P"

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
