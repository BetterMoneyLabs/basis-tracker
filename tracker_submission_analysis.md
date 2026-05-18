# Tracker Box Updater: Request Body and Error Analysis

## Request Body Sent to `/wallet/payment/send`

```json
[{
  "address": "9f7ZXamnfaDZL7EWLKLuBZgWMuHCusQYK6yow2d7p2eES9oRRRe",
  "assets": [
    {
      "amount": 1,
      "tokenId": "000b0695159e5f5c32c606385bd5f276d80133149c84c8b1325366381bf6f17f"
    }
  ],
  "fee": 1000000,
  "registers": {
    "R4": "07024e564477ff457c601c01ad1cc31903f8b27b7d5e515bd03138891d8152d787b2",
    "R5": "64000000000000000000000000000000000000000000000000000000000000000000010000002000000000"
  },
  "value": 100000
}]
```

## Error Pattern Analysis

### Error Timeline

| Time | Error Message | Interpretation |
|------|--------------|----------------|
| 09:21 | `wallet is locked` | Wallet was locked, couldn't sign |
| 09:31 | `Script reduced to false` | Contract validation failed during signing |
| 09:41 | `Failed to sign boxes due to null` | Null pointer in signing process |
| 11:42 | `Failed to sign boxes due to 0` | Signing failed with error code 0 |

### Current Error: "Failed to sign boxes due to 0"

**Full error context:**
```
Bad request List(
  PaymentRequest(
    9f7ZXamnfaDZL7EWLKLuBZgWMuHCusQYK6yow2d7p2eES9oRRRe,  // tracker address
    100000,                                                   // output value
    [Lscala.Tuple2;@7a731f75,                               // assets array
    Map(
      R4 -> ConstantNode(ECPoint(4e5644,2bce8f,...),SGroupElement),
      R5 -> ConstantNode(CAvlTree(AvlTreeData(
        Coll(0,0,0,0,...),  // 33-byte AVL root (all zeros)
        AvlTreeFlags(true,false,false),
        0,                  // key length
        None                // value length
      ), SAvlTree)
    )
  ),
  PaymentRequest(
    2iHkR7CWvD1R4j1yZg5bkeDRQavjAaVPeTDFGGLZduHyfWMuYpmhHocX8GJoaieTx78FntzJbCBVL6rf96ocJoZdmWBL2fci7NqWgAirppPQmZ7fN9V6z13Ay6brPriBKYqLp1bT2Fk4FkFLCfdPpe,
    1000000,  // fee
    [Lscala.Tuple2;@7f97324e,
    Map()
  )
). Failed to sign boxes due to 0: Vector(
  ErgoBox(0b62d077..., 9843814086, ErgoTree(...), tokens: (...), eecb82d5..., 2, Map(), 1785901),
  ErgoBox(4269b360..., 100000, ErgoTree(...), tokens: (000b0695...:1), eecb82d5..., 0, Map(R4..., R5...), 1785901),
  ...
)
```

### Root Cause

The wallet `/payment/send` API is designed for **simple P2PK transactions**. It cannot properly handle:

1. **Spending boxes with custom contracts** - The tracker box has a complex ErgoScript contract
2. **Providing proper context variables** - The tracker contract may need data inputs or context extensions
3. **Satisfying contract conditions** - The wallet doesn't know how to create the proof the contract requires

The error progression shows:
- ✅ Wallet is now unlocked (no more "wallet is locked")
- ⚠️ Contract can't be satisfied by the simple payment API
- The error changed from "null" to "0" - both indicate signing failure at different stages

### Why This Happens

The `/wallet/payment/send` API:
1. Selects input boxes from the wallet
2. Creates output boxes as specified
3. Tries to sign all inputs
4. **Fails** when an input box has a custom contract it doesn't understand

The tracker box (box with token `000b0695...`) has:
- A P2PK address as the ergoTree (from the error: `ProveDlog(ECPoint(4e5644,2bce8f,...))`)
- But the box selection includes other boxes with complex contracts

### Solution Needed

To successfully update the tracker box, we need to either:

1. **Use `/wallet/transaction/generateUnsigned`** - Lower-level API that returns unsigned tx for manual signing
2. **Use `/wallet/transaction/sign`** - Sign an already-built unsigned transaction
3. **Build transaction manually** using ergo-lib with proper box selection and signing

The current approach using `/wallet/payment/send` will continue to fail because it's too high-level for custom contracts.

## Test Confirmation

✅ **Tracker IS submitting transactions every 10 minutes**  
✅ **Request body is correctly formatted**  
⚠️ **Ergo wallet API cannot handle custom contract spending**  

This is an expected limitation of the payment API, not a bug in the tracker.
