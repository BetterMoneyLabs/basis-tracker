# Alice-Bob Workflow Test Plan

## Prerequisites
- Basis tracker server running on localhost:3048
- Ergo node accessible at configured URL

## Step-by-step Process

### 1. Create Alice's keys
```bash
# Generate Alice's keypair and save separately
cargo run --bin basis_cli -- generate-keypair > alice_keypair.txt

# Extract secret key to secret.txt (manually extract from alice_keypair.txt)
# Extract public key to alice.txt (manually extract from alice_keypair.txt)
```

### 2. Create Alice's reserve (manual API call)
```bash
# Use the API to create a 0.1 ERG reserve for Alice
curl -X POST http://localhost:3048/reserves/create \
  -H "Content-Type: application/json" \
  -d '{
    "owner_pubkey": "03bc58014bd741ea06d6f3b1de5d0847b71758a64c6f04e6c98639dcf3d12be273",
    "amount": 10000000,
    "nft_id": "1518955f0402051bdcab4b368406ed40de1c86cf8bfa0023285b94571fd72d69"
  }'
```

### 3. Create Bob's public key
```bash
# Generate Bob's keypair and save public key
cargo run --bin basis_cli -- generate-keypair > bob_keypair.txt

# Extract Bob's public key to bob.txt (manually extract from bob_keypair.txt)
```

### 4. Create IOU note from Alice to Bob (0.01 ERG)
```bash
# Switch to Alice's account
cargo run --bin basis_cli -- account switch alice

# Create and sign IOU note from Alice to Bob for 0.01 ERG (1,000,000 nanoERG)
cargo run --bin basis_cli -- note create \
  --recipient $(cat bob.txt) \
  --amount 1000000
```

This command automatically:
- Uses the current account (Alice) to sign the note
- Creates the proper signing message: recipient pubkey + amount (BE bytes) + timestamp (BE bytes)
- Submits the signed note to the tracker server
- Shows the reserve status before and after note creation

### 5. Wait for time lock (or adjust for testing)

### 6. Redemption Process

#### 6.1 Get reserve box ID
```bash
# Get Alice's reserves to find the reserve box ID
cargo run --bin basis_cli -- reserve list

# Or use API to get reserve information
curl "http://localhost:3048/reserves/issuer/$(cat alice.txt)"
```

#### 6.2 Get AVL proof for the note
```bash
# Get proof for the specific note between Alice (issuer) and Bob (recipient)
cargo run --bin basis_cli -- note get \
  --issuer $(cat alice.txt) \
  --recipient $(cat bob.txt)

# Or use API directly
curl "http://localhost:3048/proof?issuer=$(cat alice.txt)&recipient=$(cat bob.txt)"
```

#### 6.3 Prepare redemption transaction
```bash
# Use the built-in redemption command which handles signatures automatically
cargo run --bin basis_cli -- note redeem \
  --issuer $(cat alice.txt) \
  --amount 1000000
```

This command automatically:
- Gets the reserve box ID for the issuer
- Obtains the AVL proof for the note
- Signs the redemption with the issuer's private key (current account)
- Requests tracker signature from the server
- Submits the redemption transaction

### 7. Verification
- Check tracker state: `curl http://localhost:3048/`
- Check notes for Alice: `curl http://localhost:3048/notes/issuer/<ALICE_PUBKEY>`
- Check notes for Bob: `curl http://localhost:3048/notes/recipient/<BOB_PUBKEY>`
- Check redemption status in blockchain explorer

## Expected Outcomes
1. Alice's reserve has 0.1 ERG collateral
2. IOU note from Alice to Bob for 0.01 ERG is recorded
3. After time lock, Bob can redeem 0.01 ERG from Alice's reserve
4. Tracker state is updated to reflect the redemption
5. Alice's debt to Bob is reduced appropriately