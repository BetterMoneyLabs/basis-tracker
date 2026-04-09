# Tracker Box Setup Guide

**Document Version:** 1.0  
**Last Updated:** 2026-03-02  
**Status:** Production Required

---

## Overview

The **tracker box** is an on-chain Ergo box that contains the tracker server's state commitment (AVL tree root digest). It is a **critical component** of the Basis system that must be created and maintained **before any redemptions can be processed**.

### Why is the Tracker Box Required?

The tracker box serves as the on-chain commitment to the tracker's offchain state:

1. **State Commitment**: Contains the AVL tree root digest (R5 register) that commits to all debt relationships
2. **Tracker Identity**: Contains the tracker's public key (R4 register) for signature verification
3. **System Integrity**: Links the tracker to the reserve contracts via the tracker NFT (R6 register)

**Without a tracker box:**
- ❌ CLI cannot generate valid redemption transactions
- ❌ Server cannot process redemptions
- ❌ No on-chain commitment to tracker state
- ❌ System falls back to placeholder values (transactions will fail)

---

## Prerequisites

Before setting up the tracker box, ensure you have:

### 1. Tracker NFT Created

The tracker NFT is a unique token that identifies your tracker instance. It should be created **before** the tracker box.

```bash
# Example: Create tracker NFT using Ergo node
# This creates a box with a unique token ID
curl -X POST http://<node-url>/wallet/transaction/send \
  -H "api_key: <api-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "requests": [{
      "address": "<your-address>",
      "value": 1000000,
      "assets": [{
        "tokenId": "<new-token-id>",
        "amount": 1
      }]
    }]
  }'
```

**Record the NFT token ID** - you'll need it for configuration.

### 2. Tracker Key Pair Generated

Generate a secp256k1 key pair for the tracker:

```bash
# Using the CLI
basis_cli keygen --output tracker_keys.json

# Or using ergo-lib tools
# The key should be in compressed format (33 bytes, 66 hex chars)
```

**Securely store the private key** - it will be used to sign redemptions.

### 3. Ergo Node Access

You need access to an Ergo node with:
- API key for authentication
- Wallet access for transaction submission
- Sufficient ERG for box creation (minimum 0.001 ERG per box)

---

## Configuration

### Step 1: Update Server Configuration

Edit your `basis.yaml` or `config/basis.yaml` file:

```yaml
ergo:
  # Tracker NFT ID (required - 64 hex chars = 32 bytes)
  tracker_nft_id: "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b"
  
  # Tracker public key (required - 66 hex chars = 33 bytes compressed)
  # Can be hex-encoded pubkey OR P2PK address (starts with '9' for mainnet)
  tracker_public_key: "030303030303030303030303030303030303030303030303030303030303030303"
  # OR
  tracker_public_key: "9fD5TqXvN8Z3k2LmP7wR4sY6uH1jC8bA0eG9iK3oM5nQ2xV"
  
  node:
    node_url: "http://159.89.116.15:11088"
    api_key: "hello"
    scan_name: "Basis Tracker Scanner"

transaction:
  # Change address (optional - derived from tracker_public_key if not set)
  change_address: "9fD5TqXvN8Z3k2LmP7wR4sY6uH1jC8bA0eG9iK3oM5nQ2xV"
  fee: 1000000  # 0.001 ERG
```

### Step 2: Verify Configuration

```bash
# Start the server and check logs
cargo run --bin basis_server

# Expected log output:
# [INFO] Tracker NFT ID from config: Some("69c5d7a4...")
# [INFO] Initializing tracker scanner with tracker NFT ID...
# [INFO] Tracker scan registered with ID: <scan_id>
# [INFO] Tracker scanner initialization completed successfully
```

---

## Creating the Initial Tracker Box

### Option 1: Automatic (Recommended)

The tracker box updater will **automatically create** the initial tracker box on startup if:
1. Tracker NFT ID is configured
2. Tracker public key is configured
3. No existing tracker box is found

**Process:**
1. Server starts and initializes tracker scanner
2. Scanner checks for existing tracker boxes
3. If none found, creates initial box with:
   - R4: Tracker public key (GroupElement)
   - R5: Empty AVL tree (root digest of empty tree)
   - R6: Tracker NFT ID

**Logs to watch for:**
```
[INFO] No tracker boxes found, creating initial tracker box
[INFO] Tracker box update transaction submitted: tx_id=<transaction_id>
[INFO] Tracker box created: box_id=<box_id>
```

### Option 2: Manual Creation

If automatic creation fails, create the tracker box manually:

#### Using Ergo Node API

```bash
# Create tracker box with proper registers
curl -X POST http://<node-url>/wallet/transaction/send \
  -H "api_key: <api-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "requests": [{
      "address": "<tracker-address>",
      "value": 1000000,
      "assets": [{
        "tokenId": "<tracker-nft-id>",
        "amount": 1
      }],
      "registers": {
        "R4": "<tracker-pubkey-as-group-element>",
        "R5": "<serialized-savl-tree>",
        "R6": "<tracker-nft-id>"
      }
    }]
  }'
```

**Register Values:**
- **R4**: Tracker public key as GroupElement (use `Constant::from(pubkey_bytes).sigma_serialize_bytes()`)
- **R5**: Serialized SAvlTree (43 bytes):
  - Byte 0: `0x64` (SAvlTree type)
  - Bytes 1-33: Root digest (33 bytes)
  - Byte 34: `0x01` (insert-only flag)
  - Bytes 35-38: `0x00000040` (key length = 64)
  - Bytes 39-42: `0x00000000` (value length = 0 for variable)
- **R6**: Tracker NFT ID (hex-encoded, 64 chars)

#### Using CLI (Future Feature)

```bash
# This command may be added in a future version
basis_cli tracker create-initial \
  --nft-id <nft-id> \
  --pubkey <pubkey> \
  --node-url <node-url> \
  --api-key <api-key>
```

---

## Verification

### Check Tracker Box Exists

```bash
# Query the tracker box via API
curl http://localhost:3048/tracker/latest-box-id

# Expected response:
{
  "tracker_box_id": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
  "timestamp": 1234567890,
  "height": 1000000
}
```

### Check Tracker Scanner Status

```bash
# Server logs should show:
[INFO] Processed 1 tracker boxes (1 successful)
[INFO] Updated tracker state with 1 boxes
```

### Test Redemption Flow

After tracker box is created, test the redemption flow:

```bash
# 1. Create a test note
basis_cli note create \
  --recipient <recipient-pubkey> \
  --amount 1000000000

# 2. Get tracker proof
curl "http://localhost:3048/tracker/proof?issuer_pubkey=<issuer>&recipient_pubkey=<recipient>"

# Expected: Valid proof data (not placeholder)
{
  "success": true,
  "data": {
    "key": "...",
    "value": "...",
    "proof": "...",
    "total_debt": 1000000000
  }
}

# 3. Get reserve proof
curl "http://localhost:3048/reserve/proof?issuer_pubkey=<issuer>&recipient_pubkey=<recipient>"

# Expected: Valid proof with insert_proof field
{
  "success": true,
  "data": {
    "key": "...",
    "value": "...",
    "proof": null,  // null for first redemption
    "insert_proof": "...",  // ← This should NOT be placeholder
    "already_redeemed": 0,
    "is_first_redemption": true
  }
}
```

---

## Troubleshooting

### Issue: "No tracker boxes found in scanner"

**Symptoms:**
```
[WARN] No tracker boxes found in scanner
[WARN] Tracker scanner not initialized
```

**Causes:**
1. Tracker NFT ID not configured
2. Tracker scanner failed to register scan
3. No tracker box exists on-chain

**Solutions:**
1. Verify `tracker_nft_id` in configuration
2. Check Ergo node connectivity
3. Create initial tracker box (see "Creating the Initial Tracker Box" above)

---

### Issue: "Tracker scan registration failed"

**Symptoms:**
```
[WARN] Failed to register tracker scan: <error>
```

**Causes:**
1. Ergo node API unreachable
2. Invalid API key
3. Scan name conflict

**Solutions:**
1. Verify `node_url` is accessible
2. Check `api_key` is correct
3. Try changing `scan_name` in configuration

---

### Issue: "Failed to get tracker box ID from storage"

**Symptoms:**
```
[ERROR] Failed to get tracker box ID from storage: <error>
```

**Causes:**
1. Tracker storage directory not writable
2. Database corruption
3. Tracker scanner not running

**Solutions:**
1. Check `data/tracker_boxes` directory permissions
2. Delete and recreate storage directory (will rescan)
3. Restart server and check logs

---

### Issue: CLI shows "using placeholder" warnings

**Symptoms:**
```
⚠️  Tracker box not found, using placeholder.
⚠️  Could not retrieve reserve contract P2S from server, using placeholder
```

**Causes:**
1. Tracker box doesn't exist yet
2. Server not running or unreachable
3. Configuration mismatch

**Solutions:**
1. Create tracker box (see above)
2. Verify server is running: `curl http://localhost:3048/`
3. Check CLI configuration matches server

---

## Maintenance

### Tracker Box Updates

The tracker box updater **automatically updates** the tracker box every 10 minutes (configurable):

1. Fetches current AVL tree root digest
2. Creates update transaction with new R5 value
3. Submits to Ergo node

**Logs:**
```
[INFO] Tracker Box Update Transaction Submitted: R4=..., R5=..., tx_id=...
```

### Monitoring

Monitor tracker box health:

```bash
# Check latest tracker box
curl http://localhost:3048/tracker/latest-box-id

# Check tracker proof (verifies AVL tree is working)
curl "http://localhost:3048/tracker/proof?issuer_pubkey=<issuer>&recipient_pubkey=<recipient>"

# Check server health
curl http://localhost:3048/
```

### Backup

Backup tracker storage:
```bash
# Backup tracker box database
cp -r data/tracker_boxes /backup/tracker_boxes_$(date +%Y%m%d)

# Backup scanner metadata
cp -r data/tracker_scanner_metadata /backup/scanner_metadata_$(date +%Y%m%d)
```

---

## Security Considerations

### Private Key Storage

⚠️ **CRITICAL**: The tracker's private key must be stored securely:

- **DO**: Use hardware wallet or HSM for production
- **DO**: Restrict file permissions on key files
- **DON'T**: Store private key in configuration files
- **DON'T**: Commit keys to version control

### Tracker Box Access

The tracker box update mechanism requires:
- Ergo node API access with wallet permissions
- Sufficient ERG balance for transaction fees

**Recommendations:**
- Use dedicated node for tracker operations
- Monitor ERG balance for fee payments
- Set up alerts for failed box updates

---

## Related Documentation

- [CONFIGURATION.md](../CONFIGURATION.md) - Server configuration reference
- [BUILD_AND_CREATE_RESERVE.md](../BUILD_AND_CREATE_RESERVE.md) - Reserve creation guide
- [specs/spec.md](../specs/spec.md) - Basis protocol specification

---

## Support

For issues or questions:
1. Check server logs for error messages
2. Verify configuration matches this guide
3. Test with `curl` commands above
4. Review troubleshooting section

**Production deployments should test tracker box setup in a staging environment first.**
