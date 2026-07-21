# Alice and Bob Redemption Test Instructions

This document provides step-by-step instructions for testing the credit creation and redemption functionality with Alice and Bob.

## Step 1: Generate Alice's Keys

First, we'll generate Alice's secret key and public key:

```bash
# Generate Alice's keypair using basis-cli
cargo run --bin basis_cli -- generate-keypair > alice_keys_output.txt

# Extract Alice's secret key to secret.txt
# From the output, copy the secret key part to a file
echo "<alice_secret_key>" > secret.txt

# Extract Alice's public key to alice.txt  
# From the output, copy the public key part to a file
echo "<alice_public_key>" > alice.txt
```

## Step 2: Create Reserve via API

Create a reserve for Alice with 0.1 ERG collateral using the API:

```bash
# Create a reserve using the API
curl -X POST http://localhost:3048/reserves/create \
  -H "Content-Type: application/json" \
  -d '{
    "owner_pubkey": "<alice_public_key>",
    "amount": 10000000,  // 0.1 ERG in nanoERG
    "nft_id": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b"
  }'
```

## Step 3: Generate Bob's Public Key

Generate Bob's public key and save it:

```bash
# Generate Bob's keypair
cargo run --bin basis_cli -- generate-keypair > bob_keys_output.txt

# Extract Bob's public key to bob.txt
echo "<bob_public_key>" > bob.txt
```

## Step 4: Create IOU Note from Alice to Bob

Create an IOU note where Alice owes Bob 0.01 ERG:

```bash
# Create IOU note from Alice to Bob for 0.01 ERG (1,000,000 nanoERG)
# First, you'll need to sign the note using Alice's secret key
cargo run --bin basis_cli -- sign-note \
  --issuer-secret "<alice_secret_key>" \
  --recipient-pubkey "<bob_public_key>" \
  --amount 1000000 \
  --timestamp $(date +%s)
```

Then submit the signed note to the tracker:

```bash
# Submit the IOU note to the tracker
curl -X POST http://localhost:3048/notes \
  -H "Content-Type: application/json" \
  -d '{
    "recipient_pubkey": "<bob_public_key>",
    "amount_collected": 1000000,
    "amount_redeemed": 0,
    "timestamp": <timestamp>,
    "signature": "<signature_from_previous_step>",
    "issuer_pubkey": "<alice_public_key>"
  }'
```

## Step 5: Wait for Time Lock Expiration

The redemption requires a time lock (typically 1 week minimum). For testing, you can either:

1. Wait for the time lock to expire (real scenario)
2. Adjust the configuration to reduce the time lock period for testing
3. Check if there's a test configuration that allows immediate redemption

## Step 6: Prepare Redemption Transaction

To redeem the IOU note, you need to:

1. Get the AVL proof for the note from the tracker
2. Create Schnorr signatures from both Alice (issuer) and the tracker
3. Build the redemption transaction

### Get AVL Proof
```bash
# Get proof for the IOU note
curl -X GET "http://localhost:3048/proof?issuer=<alice_public_key>&recipient=<bob_public_key>"
```

### Create Redemption Request
```bash
# Submit redemption request
curl -X POST http://localhost:3048/redeem \
  -H "Content-Type: application/json" \
  -d '{
    "issuer_pubkey": "<alice_public_key>",
    "recipient_pubkey": "<bob_public_key>", 
    "amount": 1000000,
    "timestamp": <note_timestamp>,
    "reserve_box_id": "<reserve_box_id_from_step_2>",
    "avl_proof": "<avl_proof_from_get_proof>",
    "issuer_signature": "<alice_signature_for_redemption>",
    "tracker_signature": "<tracker_signature_for_redemption>",
    "recipient_address": "<bob_p2pk_address>"
  }'
```

## Step 7: Verify Redemption

After the redemption transaction is submitted:

1. Check that Alice's debt is reduced in the tracker
2. Verify that Bob received the funds
3. Confirm the reserve box collateral is reduced appropriately
4. Check that the tracker box update reflects the new state

## Alternative: Direct API Usage

If you want to test directly with the HTTP API, you can use the endpoints as follows:

1. **GET /** - Check server status
2. **POST /notes** - Submit IOU notes
3. **GET /notes/issuer/{pubkey}** - Get notes by issuer
4. **GET /notes/recipient/{pubkey}** - Get notes by recipient
5. **GET /notes/issuer/{issuer_pubkey}/recipient/{recipient_pubkey}** - Get specific note
6. **POST /redeem** - Initiate redemption process
7. **GET /proof** - Get AVL proof for redemption

## Sample API Requests

### Get all notes for Alice:
```bash
curl -X GET http://localhost:3048/notes/issuer/<alice_public_key>
```

### Get all notes for Bob:
```bash
curl -X GET http://localhost:3048/notes/recipient/<bob_public_key>
```

### Get specific note:
```bash
curl -X GET "http://localhost:3048/notes/issuer/<alice_pubkey>/recipient/<bob_pubkey>"
```

### Get AVL proof for redemption:
```bash
curl -X GET "http://localhost:3048/proof?issuer=<alice_pubkey>&recipient=<bob_pubkey>"
```

## Troubleshooting Tips

- Make sure the tracker box updater is running and has updated the blockchain with the latest AVL tree root
- Verify that timestamps are not in the future and meet the time lock requirements
- Check that the reserve has sufficient collateral for the redemption amount
- Ensure Schnorr signatures are in the correct 65-byte format (33-byte a + 32-byte z)
- Verify that public keys are in the correct 33-byte compressed format