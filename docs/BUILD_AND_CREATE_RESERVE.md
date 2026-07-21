# Instructions to Build basis-cli and Create a Reserve for Alice

## Step 1: Build the basis-cli

```bash
# Navigate to the project root
cd /home/kushti/chaincash/basis-tracker

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
      "R4": "03bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273",
      "R5": "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900012000",
      "R6": "0e20160aca8aaa854a47a58f1d84f9d2e40cea473bd670982631884c76b4ebfca2a4"
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
      "R4": "03bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273",
      "R5": "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900012000",
      "R6": "0e20160aca8aaa854a47a58f1d84f9d2e40cea473bd670982631884c76b4ebfca2a4"
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
      "R4": "03bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273",
      "R5": "644ec61f485b98eb87153f7c57db4f5ecd75556fddbc403b41acf8441fde8e160900012000",
      "R6": "0e20160aca8aaa854a47a58f1d84f9d2e40cea473bd670982631884c76b4ebfca2a4"
    }
  }
]
```

This JSON array is ready to be submitted directly to the `wallet/payment/send` API endpoint of your Ergo node.

Note: Make sure your Ergo node is running and you have the correct API key. Also, ensure that your wallet has sufficient ERG and the specified NFT to create the reserve.