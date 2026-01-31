# Redemption Transaction Format Specification

## Overview
This document specifies the format for redemption transactions that spend reserve boxes to pay out to note holders. The transaction follows the Ergo node's `/wallet/transaction/send` API format.

## Transaction Request Format

### Wallet Transaction Request Structure
For the `/wallet/transaction/send` endpoint, the redemption transaction request follows this structure according to the Ergo node API:

```json
{
  "requests": [
    {
      "address": "String",
      "value": "Number",
      "assets": [
        {
          "tokenId": "String",
          "amount": "Number"
        }
      ],
      "registers": {
        "R4": "String",
        "R5": "String"
      }
    }
  ],
  "fee": "Number",
  "inputsRaw": [
    "HexString"
  ],
  "dataInputsRaw": [
    "HexString"
  ]
}
```

### Top-Level Fields
- `requests`: Array of transaction requests (PaymentRequest objects)
- `fee`: Transaction fee in nanoERG (typically 1000000 for 0.001 ERG)
- `inputsRaw`: Array of hex-encoded serialized input box bytes (boxes to be spent)
- `dataInputsRaw`: Array of hex-encoded serialized data input box bytes (boxes to be referenced without spending)

### Payment Request Fields
- `address`: Recipient's Ergo address (required)
- `value`: Amount to send in nanoERG (required)
- `assets`: Optional array of tokens to include
- `registers`: Optional register values to include

## Redemption-Specific Transaction Format

### Redemption Transaction Structure
A redemption transaction typically has the following structure:

```json
{
  "requests": [
    {
      "address": "9RecipientAddressHere...",
      "value": 500000000,
      "assets": [],
      "registers": {}
    },
    {
      "address": "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
      "value": 99900000000,
      "assets": [
        {
          "tokenId": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b",
          "amount": 1
        }
      ],
      "registers": {
        "R4": "02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
        "R5": "hex_encoded_avl_tree_root_digest"
      }
    }
  ],
  "fee": 1000000,
  "inputsRaw": [
    "hex_encoded_serialized_reserve_box_bytes"
  ],
  "dataInputsRaw": [
    "hex_encoded_serialized_tracker_box_bytes"
  ]
}
```

### Redemption Transaction Components

#### 1. Redemption Output (First Request)
- `address`: The recipient's address (the note holder claiming redemption)
- `value`: The amount being redeemed (in nanoERG)
- `assets`: Empty array (no tokens transferred to recipient in basic redemption)
- `registers`: Empty object (no special registers needed for recipient)

#### 2. Updated Reserve Output (Second Request)
- `address`: The issuer's address (where remaining collateral goes)
- `value`: Remaining collateral after redemption (original collateral - redeemed amount - fee)
- `assets`: Contains the tracker NFT token to maintain reserve identity
- `registers`: 
  - `R4`: The issuer's public key (33-byte compressed format)
  - `R5`: The AVL tree root digest (32-byte hash + 1-byte height)

#### 3. Transaction Metadata
- `fee`: Transaction fee (typically 1000000 nanoERG = 0.001 ERG)
- `inputsRaw`: Serialized bytes of the reserve box being spent
- `dataInputsRaw`: Serialized bytes of the tracker commitment box (for state verification)

## Example Redemption Transaction

### Complete Example
```json
{
  "requests": [
    {
      "address": "9iJrR3pjgfAp7uVzmY54MSqFh6BEZG8XswWR8qMYj4Mx5e7yv",
      "value": 500000000,
      "assets": [],
      "registers": {}
    },
    {
      "address": "9fRusAarL1KkrWQVsxSRVYnvWxaAT2A96cKtNn9tvPh5XUyCisr33",
      "value": 99900000000,
      "assets": [
        {
          "tokenId": "69c5d7a4df2e72252b0015d981876fe338ca240d5576d4e731dfd848ae18fe2b",
          "amount": 1
        }
      ],
      "registers": {
        "R4": "02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
        "R5": "a1b2c3d4e5f67890123456789012345678901234567890123456789012345678"
      }
    }
  ],
  "fee": 1000000,
  "inputsRaw": [
    "0e2a0204a00b08cd0279a0e5d903050001d803d601b2a5730000d602e4c6a70407d603b2a5730100d604e4c6a70508d605b2a5730200d606e4c6a7060bd607b2a5730300d608e4c6a70705d609b2a5730400d60abe04d805d60c8c04d60d8b04d60e8a04d60f8904d6108804d6118704d6128604d6138504d6148404d6158304d6168204d6178104d6188004d6197f04d61a7e04d61b7d04d61c7c04d61d7b04d61e7a04d61f7904d6207804d6217704d6227604d6237504d6247404d6257304d6267204d6277104d6287004d6296f04d62a6e04d62b6d04d62c6c04d62d6b04d62e6a04d62f6904d6306804d6316704d6326604d6336504d6346404d6356304d6366204d6376104d6386004d6395f04d63a5e04d63b5d04d63c5c04d63d5b04d63e5a04d63f5904d6405804d6415704d6425604d6435504d6445404d6455304d6465204d6475104d6485004d6494f04d64a4e04d64b4d04d64c4c04d64d4b04d64e4a04d64f4904d6504804d6514704d6524604d6534504d6544404d6554304d6564204d6574104d6584004d6593f04d65a3e04d65b3d04d65c3c04d65d3b04d65e3a04d65f3904d6603804d6613704d6623604d6633504d6643404d6653304d6663204d6673104d6683004d6692f04d66a2e04d66b2d04d66c2c04d66d2b04d66e2a04d66f2904d6702804d6712704d6722604d6732504d6742404d6752304d6762204d6772104d6782004d6791f04d67a1e04d67b1d04d67c1c04d67d1b04d67e1a04d67f1904d6801804d6811704d6821604d6831504d6841404d6851304d6861204d6871104d6881004d6890f04d68a0e04d68b0d04d68c0c04d68d0b04d68e0a04d68f0904d6900804d6910704d6920604d6930504d6940404d6950304d6960204d6970104d6980004d699ff03d69a0004d69b0004d69c0004d69d0004d69e0004d69f0004d6a00004d6a10004d6a20004d6a30004d6a40004d6a50004d6a60004d6a70004d6a80004d6a90004d6aa0004d6ab0004d6ac0004d6ad0004d6ae0004d6af0004d6b00004d6b10004d6b20004d6b30004d6b40004d6b50004d6b60004d6b70004d6b80004d6b90004d6ba0004d6bb0004d6bc0004d6bd0004d6be0004d6bf0004d6c00004d6c10004d6c20004d6c30004d6c40004d6c50004d6c60004d6c70004d6c80004d6c90004d6ca0004d6cb0004d6cc0004d6cd0004d6ce0004d6cf0004d6d00004d6d10004d6d20004d6d30004d6d40004d6d50004d6d60004d6d70004d6d80004d6d90004d6da0004d6db0004d6dc0004d6dd0004d6de0004d6df0004d6e00004d6e10004d6e20004d6e30004d6e40004d6e50004d6e60004d6e70004d6e80004d6e90004d6ea0004d6eb0004d6ec0004d6ed0004d6ee0004d6ef0004d6f00004d6f10004d6f20004d6f30004d6f40004d6f50004d6f60004d6f70004d6f80004d6f90004d6fa0004d6fb0004d6fc0004d6fd0004d6fe0004d6ff0004"
  ],
  "dataInputsRaw": [
    "0e2a0204a00b08cd0279a0e5d903050001d803d601b2a5730000d602e4c6a70407d603b2a5730100d604e4c6a70508d605b2a5730200d606e4c6a7060bd607b2a5730300d608e4c6a70705d609b2a5730400d60abe04d805d60c8c04d60d8b04d60e8a04d60f8904d6108804d6118704d6128604d6138504d6148404d6158304d6168204d6178104d6188004d6197f04d61a7e04d61b7d04d61c7c04d61d7b04d61e7a04d61f7904d6207804d6217704d6227604d6237504d6247404d6257304d6267204d6277104d6287004d6296f04d62a6e04d62b6d04d62c6c04d62d6b04d62e6a04d62f6904d6306804d6316704d6326604d6336504d6346404d6356304d6366204d6376104d6386004d6395f04d63a5e04d63b5d04d63c5c04d63d5b04d63e5a04d63f5904d6405804d6415704d6425604d6435504d6445404d6455304d6465204d6475104d6485004d6494f04d64a4e04d64b4d04d64c4c04d64d4b04d64e4a04d64f4904d6504804d6514704d6524604d6534504d6544404d6554304d6564204d6574104d6584004d6593f04d65a3e04d65b3d04d65c3c04d65d3b04d65e3a04d65f3904d6603804d6613704d6623604d6633504d6643404d6653304d6663204d6673104d6683004d6692f04d66a2e04d66b2d04d66c2c04d66d2b04d66e2a04d66f2904d6702804d6712704d6722604d6732504d6742404d6752304d6762204d6772104d6782004d6791f04d67a1e04d67b1d04d67c1c04d67d1b04d67e1a04d67f1904d6801804d6811704d6821604d6831504d6841404d6851304d6861204d6871104d6881004d6890f04d68a0e04d68b0d04d68c0c04d68d0b04d68e0a04d68f0904d6900804d6910704d6920604d6930504d6940404d6950304d6960204d6970104d6980004d699ff03d69a0004d69b0004d69c0004d69d0004d69e0004d69f0004d6a00004d6a10004d6a20004d6a30004d6a40004d6a50004d6a60004d6a70004d6a800004d6a90004d6aa0004d6ab0004d6ac0004d6ad0004d6ae0004d6af0004d6b00004d6b10004d6b20004d6b30004d6b40004d6b50004d6b60004d6b70004d6b80004d6b90004d6ba0004d6bb0004d6bc0004d6bd0004d6be0004d6bf0004d6c00004d6c10004d6c20004d6c30004d6c40004d6c50004d6c60004d6c70004d6c80004d6c90004d6ca0004d6cb0004d6cc0004d6cd0004d6ce0004d6cf0004d6d00004d6d10004d6d20004d6d30004d6d40004d6d50004d6d60004d6d70004d6d80004d6d90004d6da0004d6db0004d6dc0004d6dd0004d6de0004d6df0004d6e00004d6e10004d6e20004d6e30004d6e40004d6e50004d6e60004d6e70004d6e80004d6e90004d6ea0004d6eb0004d6ec0004d6ed0004d6ee0004d6ef0004d6f00004d6f10004d6f20004d6f30004d6f40004d6f50004d6f60004d6f70004d6f80004d6f90004d6fa0004d6fb0004d6fc0004d6fd0004d6fe0004d6ff0004"
  ]
}
```

## Transaction Validation Requirements

### Input Validation
- All input boxes must be unspent at the time of transaction submission
- All data input boxes must exist on the blockchain
- The sum of input values must be >= sum of output values + fee
- All asset IDs and amounts must be valid
- Register values must conform to Ergo's register format

### Contract Compliance
- The transaction must satisfy all spending conditions of the input boxes
- The redemption contract script must validate the redemption proof
- The AVL tree root in R5 must match the proof provided
- The public key in R4 must match the note issuer's public key

### Security Requirements
- All required signatures must be provided
- Signatures must be valid for the respective public keys
- The transaction must not violate any time locks
- The redemption must be for a valid outstanding note amount

## Error Handling

### Common Error Scenarios
- `Insufficient Funds`: Input boxes don't have enough value
- `Invalid Proof`: The AVL tree proof doesn't validate against the tracker state
- `Contract Violation`: Spending conditions not met
- `Double Spend`: Input boxes already spent in another transaction
- `Invalid Signature`: Required signatures are missing or incorrect

### Error Response Format
```json
{
  "error": {
    "code": "String",
    "message": "String",
    "details": "Object"
  }
}
```

## Integration with Redemption Process

### Transaction Building Process
1. Identify the reserve box to be spent (input)
2. Identify the tracker commitment box (data input)
3. Calculate redemption amount (typically half of note amount)
4. Build redemption output to recipient
5. Build updated reserve output with remaining collateral
6. Include tracker NFT in updated reserve output
7. Set R4 register to issuer public key
8. Set R5 register to updated AVL tree root
9. Calculate and include transaction fee
10. Serialize all components in required format

### Signature Requirements
- At least one signature from the reserve owner (issuer)
- May require additional signatures depending on contract conditions
- Signatures must be provided separately and attached to transaction

This specification provides the complete format for redemption transactions that can be submitted to the Ergo node's `/wallet/transaction/send` endpoint.