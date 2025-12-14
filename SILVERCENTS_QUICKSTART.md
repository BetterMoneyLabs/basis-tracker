# SilverCents Quick Start Guide

## Installation

```bash
git clone https://github.com/BetterMoneyLabs/basis-tracker.git
cd basis-tracker
cargo build --release
```

## Quick Demo (5 minutes)

### 1. Start the tracker server
```bash
./run_server.sh
```

### 2. See what SilverCents is about
```bash
cargo run --bin basis-cli -- silvercents info
```

### 3. Run the interactive demo
```bash
cargo run --bin basis-cli -- silvercents demo --interactive
```

This shows Alice (vendor) issuing a note to Bob (customer) and Bob redeeming it for physical silver.

## Create Your Own Transactions

### Setup accounts
```bash
# Create vendor account
cargo run --bin basis-cli -- account create --name vendor
cargo run --bin basis-cli -- account select --name vendor
VENDOR_KEY=$(cargo run --bin basis-cli -- account list | grep vendor | awk '{print $2}')

# Create customer account  
cargo run --bin basis-cli -- account create --name customer
CUSTOMER_KEY=$(cargo run --bin basis-cli -- account list | grep customer | awk '{print $2}')
```

### Issue silver-backed note
```bash
cargo run --bin basis-cli -- account select --name vendor
cargo run --bin basis-cli -- silvercents issue \
  --recipient $CUSTOMER_KEY \
  --dimes 10 \
  --quarters 4 \
  --description "Groceries"
```

### Check balances
```bash
# Vendor's balance (what they owe)
cargo run --bin basis-cli -- account select --name vendor
cargo run --bin basis-cli -- silvercents balance --detailed

# Customer's balance (what they can redeem)
cargo run --bin basis-cli -- account select --name customer
cargo run --bin basis-cli -- silvercents balance --detailed
```

### Redeem for physical silver
```bash
cargo run --bin basis-cli -- account select --name customer
cargo run --bin basis-cli -- silvercents redeem \
  --issuer $VENDOR_KEY \
  --dimes 5 \
  --quarters 2
```

## Command Reference

| Command | Description |
|---------|-------------|
| `silvercents info` | Show system information |
| `silvercents issue` | Issue silver-backed note |
| `silvercents redeem` | Redeem note for physical silver |
| `silvercents balance` | Show balance by denomination |
| `silvercents demo` | Run interactive demonstration |

## Denominations

- **Silver Dime (SC-D)**: 1.00 ERG = 0.0723 troy oz silver
- **Silver Quarter (SC-Q)**: 2.50 ERG = 0.1808 troy oz silver

## Key Features

✅ Zero fees for offchain note creation  
✅ Backed 1:1 with physical constitutional silver  
✅ 1-week maturation period before redemption  
✅ Local vendor-customer credit relationships  
✅ Works with limited internet access  

## Architecture

```
Vendor Issues Note → Basis Tracker (Offchain) → 1 Week Wait → Customer Redeems → Ergo Blockchain
```

## Real-World Use Cases

1. **Grocery Store Credit**: Issue notes for purchases, redeem later
2. **Community Currency**: Local vendors accept shared silver-backed notes  
3. **Payroll System**: Pay employees in silver-backed SilverCents
4. **Supply Chain**: B2B payments backed by physical silver inventory

## Documentation

- Full documentation: [SILVERCENTS_DEMO.md](SILVERCENTS_DEMO.md)
- API documentation: [HTTP_API.md](HTTP_API.md)
- Basis contract: [contract/basis.es](contract/basis.es)

## Support

Questions? Open an issue at https://github.com/BetterMoneyLabs/basis-tracker/issues
