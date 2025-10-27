# Interactive Mode Demo - Alice → Bob Workflow

## Quick Start Guide

Since the CLI has state persistence issues between separate commands, use **Interactive Mode** for complete workflow testing:

```bash
# Start interactive mode
./target/debug/basis_cli interactive
```

## Interactive Mode Commands

Once in interactive mode, execute these commands in sequence:

### 1. Create Accounts
```
basis-cli [none] > account create alice
✅ Created account 'alice'

basis-cli [alice] > account create bob  
✅ Created account 'bob'
```

### 2. Alice Issues Debt to Bob
```
basis-cli [bob] > account switch alice
✅ Switched to account 'alice'

basis-cli [alice] > note create --recipient <BOB_PUBKEY> --amount 1000
✅ Note created successfully

basis-cli [alice] > note create --recipient <BOB_PUBKEY> --amount 1500
✅ Note created successfully

basis-cli [alice] > note create --recipient <BOB_PUBKEY> --amount 2000
✅ Note created successfully
```

### 3. Verify Notes
```
basis-cli [alice] > note list --issuer
Notes where you are the issuer:
  To: <BOB_PUBKEY>
    Amount: 1000 nanoERG
    ...
```

### 4. Bob Checks Received Debt
```
basis-cli [alice] > account switch bob
✅ Switched to account 'bob'

basis-cli [bob] > note list --recipient
Notes where you are the recipient:
  From: <ALICE_PUBKEY>
    Amount: 1000 nanoERG
    ...
```

### 5. Check Reserve Status
```
basis-cli [bob] > reserve status --issuer <ALICE_PUBKEY>
Reserve Status for <ALICE_PUBKEY>:
  Total Debt: 4500 nanoERG
  Collateral: 0 nanoERG
  ...
```

### 6. Bob Redeems Debt
```
basis-cli [bob] > note redeem --issuer <ALICE_PUBKEY> --amount 500
✅ Redemption initiated
✅ Redemption completed
```

### 7. Monitor Events
```
basis-cli [bob] > status
✅ Server is healthy
Recent Events (last 4):
  [timestamp] Redemption: Alice -> Bob (500 nanoERG)
  [timestamp] Note: Alice -> Bob (2000 nanoERG)
  ...
```

## Complete Workflow in One Session

The interactive mode maintains account state throughout the session, allowing you to:

1. **Create multiple accounts** and switch between them
2. **Issue debt notes** from Alice to Bob
3. **Monitor reserve status** and collateralization
4. **Execute redemptions** from Bob's perspective
5. **Track all events** in real-time
6. **Generate proofs** for verification

## Notes for Production Use

- **Blockchain Integration**: Reserve creation requires actual Ergo blockchain deployment
- **Key Management**: Production would need secure private key storage
- **Server Dependency**: All operations require Basis Tracker server running
- **Persistence**: Interactive mode maintains state only during the session

This interactive approach provides the most reliable way to test the complete Alice → Bob workflow with the current CLI implementation.