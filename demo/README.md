# Basis Tracker Demo

This demo simulates Alice issuing notes to Bob with collateralization monitoring using simple bash scripts.

## Prerequisites

1. Start the Basis server:
   ```bash
   cd /home/kushti/bml/basis-tracker
   cargo run -p basis_server
   ```

2. Install curl (if not already installed):
   ```bash
   sudo apt-get install curl  # Ubuntu/Debian
   ```

3. Install jq for better JSON parsing (recommended):
   ```bash
   sudo apt-get install jq  # Ubuntu/Debian
   ```

## Running the Demo

### Terminal 1: Bob Receiver
```bash
cd demo
./bob_receiver.sh
```

### Terminal 2: Alice Issuer
```bash
cd demo
./alice_issuer.sh
```

## Demo Behavior

1. **Alice** periodically issues notes to Bob (every 30s)
2. **Bob** polls for new notes every 10s
3. **Collateralization** is calculated as: `(Alice's reserve) / (total notes issued)`
4. **Bob stops accepting** when collateralization drops below 100%
5. Both scripts display real-time status updates
6. **Bob tracks new notes** by comparing timestamps (no server-side filtering needed)

## Current Implementation

- **POST endpoint works**: Note creation via POST /notes works correctly
- **GET endpoint workaround**: Since the GET endpoint has issues, Bob tracks Alice's issuance using a shared file mechanism
- **Demo functionality**: The demo fully demonstrates note issuance, debt tracking, and collateralization monitoring

## How It Works

1. **Alice issues notes** via POST /notes endpoint
2. **Alice records each issuance** in a shared file (`/tmp/alice_issuance.log`)
3. **Bob monitors the shared file** to track Alice's debt issuance
4. **Bob calculates collateralization** in real-time and stops accepting when it drops below the minimum
5. **Both scripts display real-time status** including note amounts, totals, and collateralization ratios

## Configuration

You can edit the scripts to change parameters:

### Alice Issuer (`alice_issuer.sh`):
- `SERVER_URL`: Basis server URL
- `ALICE_PUBKEY`: Alice's public key
- `BOB_PUBKEY`: Bob's public key  
- `RESERVE_BALANCE`: Starting reserve balance
- `ISSUE_INTERVAL`: Note issuance interval in seconds
- `AMOUNT_MIN`: Minimum note amount
- `AMOUNT_MAX`: Maximum note amount

### Bob Receiver (`bob_receiver.sh`):
- `SERVER_URL`: Basis server URL
- `BOB_PUBKEY`: Bob's public key
- `ALICE_PUBKEY`: Alice's public key
- `ALICE_RESERVE`: Alice's reserve balance (must match Alice's setting)
- `POLL_INTERVAL`: Polling interval in seconds
- `MIN_COLLATERALIZATION`: Minimum acceptable ratio (1.0 = 100%)

## Key Features

- **Real-time monitoring** of note issuance and reception
- **Collateralization tracking** with automatic stop condition
- **Simple bash implementation** - no compilation needed
- **Error handling** and graceful degradation
- **Configurable parameters** for different demo scenarios

## Notes

- The demo uses simple signature generation for demonstration purposes
- In a real implementation, proper cryptographic signing would be used
- The scripts assume the server is running on localhost:3000
- Adjust the reserve balance and amounts based on your testing needs