# SilverCents: Silver-Backed Offchain Cash Demo

## System Overview

SilverCents is a demonstration implementation of the Basis offchain cash system, showcasing how Basis can support real-world commodity-backed community currencies. SilverCents represent offchain IOU notes issued under the Basis model that are redeemable against on-chain reserves backed by physical U.S. constitutional silver coins (dimes and quarters).

### Key Components

1. **On-chain Reserve**: Uses the existing Basis reserve contract on Ergo blockchain
2. **Offchain SilverCent Notes**: IOU notes tracked by a Basis tracker
3. **CLI Demo Client**: Command-line interface for interacting with the system
4. **Offchain Reserve Ledger**: JSON-based ledger tracking physical silver reserves

## Economic Model

### Asset Backing
- Each SilverCent represents a claim on one physical U.S. constitutional silver coin
- Physical silver remains off-chain and is managed by a trusted custodian
- Total SilverCent supply is limited by declared physical silver reserves

### Issuance and Redemption Cycle
1. **Issuance**: Issuer declares silver deposit, updates offchain ledger, mints SilverCents as offchain notes
2. **Circulation**: SilverCents circulate as offchain payments between participants
3. **Redemption**: Holders redeem SilverCents against on-chain reserves, receiving physical silver

### Trust Assumptions
- Issuer is trusted to honestly manage physical silver reserves
- Tracker is trusted to correctly track and commit note states
- No price oracles or dynamic peg mechanisms required for demo

### Basis Protocol Integration
SilverCents demonstrates Basis's support for:
- Offchain issuance of commodity-backed cash
- Reserve-backed redemption
- Tracker-based accounting for community currencies
- Separation between offchain payments and on-chain settlement

## CLI Usage

### Prerequisites
- Running Basis tracker server
- Ergo node access
- Account configured with CLI

### Commands

#### Deposit Physical Silver
```bash
basis-cli silver-cents deposit --amount 1000
```
Deposits 1000 silver coins into the reserve ledger.

#### Issue SilverCents
```bash
basis-cli silver-cents issue --amount 100 --to <recipient_pubkey>
```
Issues 100 SilverCents to the specified recipient as an offchain note.

#### Pay with SilverCents
```bash
basis-cli silver-cents pay --to <recipient_pubkey> --amount 50
```
Makes an offchain payment of 50 SilverCents to the recipient.

#### Redeem SilverCents
```bash
basis-cli silver-cents redeem --issuer <issuer_pubkey> --amount 25
```
Redeems 25 SilverCents from the specified issuer against on-chain reserves.

#### Check Status
```bash
basis-cli silver-cents status
```
Displays current reserve status, collateralization ratios, and outstanding notes.
**Note**: Requires running Basis tracker server at `http://127.0.0.1:3048`

## Example CLI Session

```bash
# Initialize with physical silver deposit
$ basis-cli silver-cents deposit --amount 1000
Deposited 1000 physical silver coins. Total reserve: 1000

# Issue initial SilverCents
$ basis-cli silver-cents issue --amount 500 --to 02a123...
Issued 500 SilverCents to 02a123...

# Check status (requires tracker server)
$ basis-cli silver-cents status
SilverCents Status:
Physical silver reserve: 1000
SilverCents issued: 500
SilverCents redeemed: 0
Outstanding SilverCents: 500
Collateralization ratio: 200.00%
On-chain reserve collateral: 1000000000  # 10 ERG
On-chain total debt: 0
On-chain collateralization ratio: 100.00%

# Make a payment
$ basis-cli silver-cents pay --to 03b456... --amount 100
Paid 100 SilverCents to 03b456...

# Redeem some SilverCents
$ basis-cli silver-cents redeem --issuer 02a123... --amount 50
Redeemed 50 SilverCents from 02a123.... Physical silver release confirmed.

# Final status
$ basis-cli silver-cents status
SilverCents Status:
Physical silver reserve: 1000
SilverCents issued: 500
SilverCents redeemed: 50
Outstanding SilverCents: 450
Collateralization ratio: 222.22%
On-chain reserve collateral: 999500000  # Reduced by redemption
On-chain total debt: 0
On-chain collateralization ratio: 100.00%
```

## Testing the SilverCents Demo

### Prerequisites
1. Build the basis_cli binary:
   ```bash
   cargo build --bin basis_cli
   ```

2. Create test accounts:
   ```bash
   ./target/debug/basis_cli account create issuer1
   ./target/debug/basis_cli account create receiver1
   ./target/debug/basis_cli account switch issuer1
   ```

### Testing Deposit (Offline)
The deposit command works without a running tracker server:

```bash
# Test initial deposit
./target/debug/basis_cli silver-cents deposit --amount 1000
# Output: Deposited 1000 physical silver coins. Total reserve: 1000

# Verify silver_reserve.json file
cat silver_reserve.json
# Output shows: "total_physical_silver": 1000

# Test another deposit
./target/debug/basis_cli silver-cents deposit --amount 500
# Output: Deposited 500 physical silver coins. Total reserve: 1500
```

### Testing with Tracker Server
To test `issue`, `pay`, `redeem`, and `status` commands, you need a running Basis tracker server:

```bash
# In one terminal, start the tracker server
./run_server.sh

# In another terminal, test issuance and other commands
./target/debug/basis_cli silver-cents issue --amount 100 --to 02c98e43d1be8762c890ba30213ce1de85c04fdc689d32d3940b41dfa2e012ac2a

# Check status
./target/debug/basis_cli silver-cents status
```

### Known Limitations in Current Implementation
- `issue` command: Requires tracker server to submit notes
- `pay` command: Requires tracker server to update notes
- `redeem` command: Requires tracker server to process redemptions
- `status` command: Requires tracker server for on-chain data

The offline ledger (`silver_reserve.json`) tracks:
- Physical silver deposits
- Total issued SilverCents
- Total redeemed SilverCents
- Collateralization ratios

## Limitations of the Demo

### Technical Limitations
- Simplified offchain reserve ledger (JSON file)
- No real physical silver custody
- Mock custodian operations
- Single tracker instance
- No privacy features
- No multi-party governance

### Economic Limitations
- Trusted issuer assumption
- No price stabilization mechanisms
- No decentralized auditing
- No regulatory compliance
- Demo-scale only

### Security Considerations
- Not production-ready
- No formal security audit
- Simplified cryptographic operations
- No anti-censorship protections

## Implementation Details

### Offchain Reserve Ledger
Stored in `silver_reserve.json`:
```json
{
  "total_physical_silver": 1000,
  "total_silvercents_issued": 500,
  "total_silvercents_redeemed": 50
}
```

### Note Structure
SilverCent notes follow Basis IOU format:
- Recipient public key
- Total amount collected (cumulative debt)
- Total amount redeemed
- Latest timestamp
- Issuer signature

### Contract Integration
Uses existing Basis reserve contract with:
- Reserve tracking in ERG
- Redemption validation
- Double-spend prevention
- Tracker state commitments

## Future Extensions

### Production Readiness
- Real silver custody protocols
- Multi-signature issuer governance
- Decentralized auditing mechanisms
- Privacy-preserving note designs
- Multi-tracker federation

### Enhanced Features
- Price-pegged SilverCents
- Automated reserve management
- Cross-chain redemption
- Mobile wallet integration
- Merchant payment processing

## Conclusion

SilverCents demonstrates how Basis can power commodity-backed community currencies, showing the complete lifecycle from physical asset deposit through offchain circulation to on-chain redemption. While simplified for demonstration, it validates Basis's potential for real-world economic applications beyond traditional cryptocurrencies.