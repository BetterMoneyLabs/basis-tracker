# Instructions to Build basis-cli and Create a Reserve for Alice

## Step 1: Build the basis-cli

```bash
# Navigate to the project root
cd basis-tracker

# Build the entire workspace (this will build basis-cli)
cargo build --release

# Or build just the CLI component
cargo build -p basis_cli --release
```

The built binary will be available at:
- `target/release/basis_cli` (release build)
- `target/debug/basis_cli` (debug build)

## Step 2: Create an account for Alice

```bash
# Create an account named 'alice'
./target/release/basis_cli account create alice

# Verify the account was created
./target/release/basis_cli account list
```

## Step 3: Create the Reserve for Alice

```bash
# Create a reserve using the specified NFT ID
./target/release/basis_cli reserve create \
    --nft-id 3f62726ca1e597181b12de47e684fb74d3df86fb90fa19ea90cfdd6af28b6cee \
    --amount 1000000000  # 1 ERG in nanoERG
```

If you want to specify Alice's public key explicitly (instead of using the current account):
```bash
./target/release/basis_cli reserve create \
    --nft-id 3f62726ca1e597181b12de47e684fb74d3df86fb90fa19ea90cfdd6af28b6cee \
    --owner YOUR_ALICE_PUBLIC_KEY_HEX \
    --amount 1000000000
```

## Step 4: Submit the Reserve Creation Payload to Wallet API

The CLI command will output a JSON payload that you can use with the Ergo wallet API. The output will look something like this:

```json
[
  {
    "address": "3Wz...alice_address...",
    "value": 1000000000,
    "assets": [
      {
        "tokenId": "3f62726ca1e597181b12de47e684fb74d3df86fb90fa19ea90cfdd6af28b6cee",
        "amount": 1
      }
    ],
    "registers": {
      "R4": "0703bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273",
      "R5": "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000",
      "R6": "0e20160aca8aaa854a47a58f1d84f9d2e40cea473bd670982631884c76b4ebfca2a4",
      "R7": "05000000000000000000"
    }
  }
]
```

## Step 5: Submit to Wallet API

To submit this reserve creation transaction to the Ergo wallet, use the following curl command:

```bash
curl -X POST http://your-ergo-node:9053/wallet/payment/send \
    -H "Content-Type: application/json" \
    -H "api_key: your-api-key" \
    -d '[
  {
    "address": "3Wz...alice_address...",
    "value": 1000000000,
    "assets": [
      {
        "tokenId": "3f62726ca1e597181b12de47e684fb74d3df86fb90fa19ea90cfdd6af28b6cee",
        "amount": 1
      }
    ],
    "registers": {
      "R4": "0703bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273",
      "R5": "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000",
      "R6": "0e20160aca8aaa854a47a58f1d84f9d2e40cea473bd670982631884c76b4ebfca2a4",
      "R7": "05000000000000000000"
    }
  }
]'
```

## Alternative: Direct JSON Array for wallet/payment/send API

If you want the exact JSON array format that the wallet/payment/send API expects, you can use the CLI output directly. The CLI generates the correct format:

```json
[
  {
    "address": "3Wz...alice_address...",
    "value": 1000000000,
    "assets": [
      {
        "tokenId": "3f62726ca1e597181b12de47e684fb74d3df86fb90fa19ea90cfdd6af28b6cee",
        "amount": 1
      }
    ],
    "registers": {
      "R4": "0703bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273",
      "R5": "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900032000",
      "R6": "0e20160aca8aaa854a47a58f1d84f9d2e40cea473bd670982631884c76b4ebfca2a4",
      "R7": "05000000000000000000"
    }
  }
]
```

This JSON array is ready to be submitted directly to the `wallet/payment/send` API endpoint of your Ergo node.

## Submitting via the CLI or TUI

The tracker only **builds** the payload; it never broadcasts it, so the tracker's
wallet is never involved (PR #16 B4). The reserve owner submits the payload to
**their own Ergo node** — the node wallet must hold the reserve NFT and enough ERG
for the reserve value plus fee.

The tracker only tracks a reserve whose creating transaction spends a P2PK input of
the R4 owner key (issue #19; see `specs/ergo/reserve_admission.md`). With `node_url`
(and `node_api_key` if needed) set in `~/.basis/cli.toml`, the CLI can submit for you
and guarantees that proof input:

```bash
basis-cli reserve create --nft-id <your_nft_id> --amount 1000000000 --submit
```

Under `--submit` the CLI inspects the node wallet first:

- if every wallet box belongs to the owner key, the payload is sent as-is;
- otherwise the deployment inputs are pinned (`/wallet/transaction/send` with
  `inputsRaw`: the owner's P2PK box plus funding boxes, with explicit change);
- if the wallet holds no P2PK box for the owner key at all, the CLI first sends a
  pre-deployment transaction of 0.001 ERG to the owner's own address, waits for it to
  appear in the wallet, and then submits the pinned deployment spending it. (If the
  node wallet does not hold the owner key, this step fails with an explicit error.)

Without `--submit` (or without node settings) the CLI prints the payload for manual
submission. **Manual submission is only safe when every unspent box in the node
wallet belongs to the owner key** — otherwise the node may fund the transaction from
inputs with no P2PK(owner) box, and the tracker will quarantine the reserve as
unproven:

```bash
curl -X POST http://127.0.0.1:9053/wallet/payment/send \
  -H "Content-Type: application/json" \
  -H "api_key: <your_node_api_key>" \
  -d '<the requests array from the payload>'
```

The TUI wallet offers the same flow as `--submit` after generating the payload. The
requests array already uses the camelCase `tokenId` format the node expects and is
returned as the transaction id.

Note: Make sure your Ergo node is running and you have the correct API key. Also, ensure that your wallet has sufficient ERG and the specified NFT to create the reserve.