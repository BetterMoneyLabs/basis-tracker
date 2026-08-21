# Celaut + USE Stablecoin Agentic Credit Demo — Run 6 Report

**Date:** 2026-08-20  
**Network:** Ergo mainnet via local node `http://127.0.0.1:9053`  
**Demo directory:** `demo/agent_celaut_use/`  
**Log file:** `/tmp/celaut_use_demo_run6.log`

## Objective

Run the agentic credit + on-chain redemption demo that combines the Celaut service-market idea with the real USE stablecoin. The scenario had to show:

1. A pure-credit payment (no collateral).
2. A USE-collateralized credit payment.
3. On-chain redemption of the collateralized note into real USE tokens.

## Test configuration

| Parameter | Value |
|-----------|-------|
| USE token id | `a55b8735ed1a99e46c2c89f8994aacdf4b1109bdcf682f1e5b34479c6e392669` (3 decimals) |
| Tracker NFT | `1b8bf82db92827ed930ad33bffe3539e2774664a5f38d7967340f9682d3c981b` |
| Dave reserve NFT | `6265b8d55a4f61f9909c85545d5fcd8d4094fb6d3932f414b8fc509c64f819c8` |
| Dave public key | `03869a1e70fa2e421f6b8325d87adb74d41998e92a7f180a133f6bfed1638bdd47` |
| Dave secret | `018c22239370eec8569cc9017c14b881bb4a368fa0796bded69a72042d6ddda7` |
| Node API key | `hello` |

## Scenario flow

1. **Preflight** — created/verified Dave’s USE-backed reserve with **500 USE** collateral.
2. **Pure credit** — `user_charlie` paid `node_bob` 0.1 USE for a hash service; accepted as pure credit under Bob’s policy.
3. **Collateralized credit** — `user_dave` paid `node_bob` 0.1 USE for a hash service; the acceptance gate required and found Dave’s USE reserve, so the note was accepted.
4. **Tracker commitment** — both notes were committed to the on-chain tracker box and confirmed.
5. **Redemption** — Bob locally signed and broadcast a redemption transaction spending Dave’s reserve.

## On-chain transactions

| Step | Transaction ID | Status |
|------|----------------|--------|
| Tracker box update | `a34639b8483463f85b78cffd9cbc8b7e652e9542c97eb0afc3f3a6718a2d4f05` | Confirmed at height 1855153 |
| Bob’s redemption | `80a4e794356f8c8885333049f45cd1a23e9284f79a3a00cf118af1b85354cfb7` | Confirmed |

## On-chain verification

### Dave’s reserve after redemption

- **Old reserve box:** `e4bad5931fe5f1a9c934f553ea0e35e192cf7e15bd462b708eab78968df88755`
- **New reserve box:** `b3c15f52646ad49a5c30ff5eff3b2408f9064f3992266f38c8baff4943db7f10`
- **Tokens held:**
  - Reserve NFT (`6265b8d...`) × 1
  - USE token (`a55b873...`) × **400**

This confirms **100 USE were paid out** to the redeemer.

### Wallet balance change

- **Before demo:** 1500 USE tokens
- **After demo:** 1600 USE tokens
- **Change:** +100 USE (= 0.1 USE, matching the redeemed note amount)

## Fixes applied during this run

- `demo/agent_celaut_use/run.sh`: changed `python3 "$SCRIPT_DIR/orchestrator.py" --auto` to `python3 -u "$SCRIPT_DIR/orchestrator.py" --auto` to disable Python stdout buffering and get clean real-time demo output.

## Result

Demo completed successfully:

```
[INFO] Demo finished successfully.
```

The system demonstrated:

- Agentic issuance of both pure and collateralized credit.
- Acceptance-policy enforcement requiring 100% USE collateral for non-trusted issuers.
- Successful on-chain redemption of a USE-collateralized IOU into real USE stablecoin tokens using local signing against the main-chain node at `127.0.0.1:9053`.
