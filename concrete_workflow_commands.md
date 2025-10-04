# Concrete Alice → Bob Workflow Commands

## Complete Step-by-Step Commands

### 1. Start Server & Build CLI
```bash
# Terminal 1: Start Basis Tracker Server
cd /home/kushti/bml/basis-tracker
cargo run -p basis_server

# Terminal 2: Build CLI Client
cd /home/kushti/bml/basis-tracker
cargo build -p basis_cli
```

### 2. Account Creation
```bash
# Create Alice's account
./target/debug/basis_cli account create alice

# Create Bob's account  
./target/debug/basis_cli account create bob

# Verify accounts
./target/debug/basis_cli account list
```

### 3. Interactive Workflow Session
```bash
# Start interactive mode
./target/debug/basis_cli interactive
```

#### Inside Interactive Mode:
```
# Create accounts
account create alice
account create bob

# Switch to Alice and issue debt
account switch alice
note create --recipient <BOB_PUBKEY> --amount 1000
note create --recipient <BOB_PUBKEY> --amount 1500
note create --recipient <BOB_PUBKEY> --amount 2000

# Verify Alice's notes
note list --issuer

# Switch to Bob and check received notes
account switch bob
note list --recipient

# Check reserve status
reserve status --issuer <ALICE_PUBKEY>

# Bob redeems debt
note redeem --issuer <ALICE_PUBKEY> --amount 500
note redeem --issuer <ALICE_PUBKEY> --amount 1000

# Final verification
note list --recipient
reserve status --issuer <ALICE_PUBKEY>
status

# Exit
quit
```

### 4. Blockchain Reserve Deployment
*(Requires Ergo blockchain access)*

```scala
// Using Ergo AppKit to deploy reserve contract
import org.ergoplatform.appkit._

val ergoClient = RestApiErgoClient.create("http://localhost:9053", NetworkType.TESTNET, "", "http://localhost:9053")

ergoClient.execute { ctx =>
  val contract = BasisReserveContract(
    issuerPubKey = "<ALICE_PUBKEY>",
    collateralAmount = 10000000000L, // 10 ERG
    minCollateralRatio = 1500000000L
  )
  
  val tx = contract.deploy(ctx)
  println(s"Reserve deployed: ${tx.getId}")
}
```

### 5. Verification Commands
```bash
# Check server status
./target/debug/basis_cli status

# Check specific note
./target/debug/basis_cli note get --issuer <ALICE_PUBKEY> --recipient <BOB_PUBKEY>

# Generate proof
./target/debug/basis_cli proof --issuer <ALICE_PUBKEY> --recipient <BOB_PUBKEY>

# Check collateralization
./target/debug/basis_cli reserve collateralization --issuer <ALICE_PUBKEY>
```

## Expected Output Summary

### Account Creation
```
✅ Created account 'alice'
  Public Key: 03d8e49284c85bf1c2f5ceb90fd805749e0ee88ee5ff833ce7b40d0ce735a2f9e6
```

### Note Creation
```
✅ Note created successfully
```

### Redemption
```
✅ Redemption initiated
  Redemption ID: redemption_123456
  Amount: 500 nanoERG
  Proof available: true
✅ Redemption completed
```

### Reserve Status
```
Reserve Status for <ALICE_PUBKEY>:
  Total Debt: 3000 nanoERG
  Collateral: 10000000000 nanoERG
  Collateralization Ratio: 3333333.33
```

### Event History
```
Recent Events (last 6):
  [timestamp] Redemption: Alice -> Bob (1000 nanoERG)
  [timestamp] Redemption: Alice -> Bob (500 nanoERG)
  [timestamp] Reserve created: box123... (10000000000 nanoERG)
  [timestamp] Note: Alice -> Bob (2000 nanoERG)
  [timestamp] Note: Alice -> Bob (1500 nanoERG)
  [timestamp] Note: Alice -> Bob (1000 nanoERG)
```

## Workflow Summary

**Total Debt Issued**: 4500 nanoERG
**Total Redeemed**: 1500 nanoERG  
**Outstanding Debt**: 3000 nanoERG
**Collateral**: 10 ERG
**Collateralization Ratio**: 3333333.33 (Excellent)