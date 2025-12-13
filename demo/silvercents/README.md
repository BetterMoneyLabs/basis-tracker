# SilverCents Demo

**Silver-backed offchain cash for local circular economies**

This demo showcases the Basis protocol with SilverCents - a hybrid cryptocurrency backed 50% by DexySilver tokens and exchangeable 1:1 with constitutional silver dimes and quarters.

---

## Requirements

- **Bash 4.0+** (included in most Unix systems)
- **`bc`** (calculator command for decimal arithmetic)
  - **Linux:** `sudo apt-get install bc` or `sudo yum install bc`
  - **Mac:** Included by default
  - **Windows:** Use WSL (`wsl --install`) or install via [Chocolatey](https://chocolatey.org/): `choco install gnuwin32-coreutils.install`
- **`curl`** (for API calls - usually pre-installed)

---

## Quick Start

### 1. Run the Interactive Demo

```bash
cd demo/silvercents
chmod +x *.sh
./silvercents_demo.sh
```

This runs a complete farmers market scenario with 3 vendors and 2 customers.

### 2. Try the CLI Tools

**Vendor:**
```bash
# Initialize vendor
./silvercents_vendor.sh init "My Farm Stand" "Local Market"

# Create reserve
./silvercents_vendor.sh create-reserve "My Farm Stand" 10 1000

# Issue SilverCents
./silvercents_vendor.sh issue "My Farm Stand" <customer_pubkey> 50 "Fresh produce"

# Check status
./silvercents_vendor.sh status "My Farm Stand"
```

**Customer:**
```bash
# Initialize customer
./silvercents_customer.sh init "John"

# Check balance
./silvercents_customer.sh balance "John"

# List received SilverCents
./silvercents_customer.sh list "John"

# Redeem for silver
./silvercents_customer.sh redeem "John" <vendor_pubkey> 25 quarters
```

---

## What is SilverCents?

SilverCents are on-chain tokens using the Basis protocol with the following properties:

- **Backing:** 50% collateralized by DexySilver tokens
- **Redemption:** Exchangeable 1:1 with constitutional silver dimes/quarters
- **Use Case:** Local circular economies (farmers markets, food trucks, flea markets)
- **Participants:** Vendors (issuers) and Customers (receivers)

### Key Features

✅ **Silver-Backed** - 50% DexySilver token collateralization  
✅ **Physical Redemption** - Exchange for real silver coins  
✅ **Local Economy** - Perfect for farmers markets and small businesses  
✅ **Peer-to-Peer** - Customers can transfer to each other  
✅ **Transparent** - Collateralization tracked in real-time  

---

## Demo Scenario

**Portland Farmers Market - Saturday Morning**

### Vendors
1. **Bob's Farm Stand** - Fresh vegetables
2. **Carol's Bakery** - Artisan bread
3. **Dave's Coffee Cart** - Coffee and pastries

### Customers
1. **Alice** - Regular shopper
2. **Eve** - New customer

### Transaction Flow
1. Vendors create reserves with DexySilver backing
2. Alice buys vegetables from Bob (50 SilverCents)
3. Alice buys bread from Carol (30 SilverCents)
4. Eve buys coffee from Dave (15 SilverCents)
5. Alice transfers 10 SilverCents to Eve
6. Eve redeems 20 SilverCents from Dave for quarters

---

## CLI Commands

### Vendor CLI

| Command | Description |
|---------|-------------|
| `init <name> [location]` | Initialize vendor account |
| `create-reserve <name> <erg> <dexysilver>` | Create silver-backed reserve |
| `issue <name> <customer> <amount> [memo]` | Issue SilverCents |
| `status <name>` | Check reserve status |
| `redeem <name> <customer> <amount> [type]` | Process redemption |

### Customer CLI

| Command | Description |
|---------|-------------|
| `init <name>` | Initialize customer account |
| `balance <name>` | Check total balance |
| `list <name> [vendor]` | List received SilverCents |
| `redeem <name> <vendor> <amount> [prefer]` | Request redemption |
| `transfer <name> <to> <amount>` | Transfer to another customer |

---

## Silver Conversion

- **1 SilverCent** = 1 silver dime equivalent
- **2.5 dimes** = 1 quarter
- **10 dimes** = 1 dollar in silver

**Example:**
- 50 SilverCents = 50 dimes = 20 quarters
- 25 SilverCents = 25 dimes = 10 quarters

---

## Collateralization

### Requirements
- **Minimum:** 50% DexySilver tokens
- **Recommended:** 150%+ total collateral (ERG + DexySilver)
- **Warning:** 80% threshold triggers alerts
- **Critical:** Below 50% prevents new issuance

### Status Levels
- **EXCELLENT:** 200%+ collateralization
- **GOOD:** 150-200%
- **ADEQUATE:** 100-150%
- **WARNING:** 80-100%
- **CRITICAL:** 50-80%
- **UNDER-COLLATERALIZED:** <50%

---

## Technical Details

### Reserve Structure
```json
{
  "erg_collateral": 10000000000,      // 10 ERG
  "dexysilver_tokens": 1000,          // 1000 tokens
  "issued_silvercents": 7500000000,   // 7.5 SC
  "collateral_ratio": 2.0,            // 200%
  "min_ratio": 0.5                    // 50% minimum
}
```

### Note Structure
```json
{
  "issuer": "02abc...",               // Vendor pubkey
  "recipient": "02def...",            // Customer pubkey
  "amount": 5000000000,               // 5 SC (nanoERG)
  "timestamp": 1734096000,
  "signature": "304402...",
  "metadata": {
    "vendor_name": "Bob's Farm Stand",
    "memo": "Fresh vegetables",
    "silver_equivalent": "5 dimes"
  }
}
```

---

## Files

```
demo/silvercents/
├── silvercents_vendor.sh      # Vendor CLI
├── silvercents_customer.sh    # Customer CLI
├── silvercents_demo.sh        # Interactive demo
├── silvercents_utils.sh       # Shared utilities
└── README.md                  # This file
```

---

## Troubleshooting

### "Vendor not found"
Run `init` command first to create the account.

### "Insufficient collateralization"
Add more ERG or DexySilver tokens to the reserve.

### "Invalid pubkey"
Ensure pubkey is 66 characters (with 02/03 prefix).

### Demo cleanup
```bash
./silvercents_demo.sh cleanup
```

---

## Next Steps

1. **Run the demo** to see SilverCents in action
2. **Experiment with CLI** to understand the workflow
3. **Modify scenarios** to test different use cases
4. **Integrate with real Basis server** for production use

---

## Learn More

- **Basis Protocol:** See `chaincash/docs/basis.md`
- **SilverCents Spec:** See `chaincash/docs/silvercents.md`
- **Architecture:** See `docs/SILVERCENTS_ARCHITECTURE.md`

---

**Built for the LNMIIT Open Source Hackathon 2025**  
Team Dev Engers | Issue #2 - SilverCents Demo Implementation
