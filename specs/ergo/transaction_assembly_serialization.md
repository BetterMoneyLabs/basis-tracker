# Ergo Transaction Assembly and Serialization Specification

## Overview

This document specifies how Ergo transactions are assembled, serialized, and deserialized in the Ergo blockchain protocol. Transactions represent state transitions in the UTXO model, consuming existing boxes and creating new ones.

## Transaction Structure

### Basic Components
An Ergo transaction consists of:
- **inputs**: List of box IDs to be consumed
- **data_inputs**: List of box IDs for read-only access (not consumed)
- **outputs**: List of newly created boxes
- **creation_height**: Block height when transaction is created
- **tx_id**: Unique transaction identifier (Blake2b hash of serialized transaction)

### Unsigned Transaction
Before signing, transactions exist in unsigned form with:
- Input boxes referenced by ID
- Output boxes with complete data
- No spending proofs initially

## Transaction Assembly Process

### 1. Input Collection
Collect input boxes that will be consumed:
- Retrieve boxes from the UTXO set
- Verify boxes exist and are unspent
- Ensure sufficient value and tokens for transaction

### 2. Data Input Collection
Collect boxes for read-only access:
- Retrieve boxes that will be accessed but not consumed
- Use for oracle data, price feeds, etc.
- Do not require spending proofs

### 3. Output Creation
Create new boxes for transaction outputs:
- Define value amounts for each output
- Specify tokens to be transferred
- Set guarding scripts (ErgoTrees) for each output
- Add optional registers (R4-R9) with additional data

### 4. Fee Calculation
Ensure transaction meets minimum fee requirements:
- Calculate total input value vs output value
- Account for mining fees
- Verify no value is created or destroyed except fees and emission

### 5. Context Formation
Build execution contexts for script evaluation:
- Prepare input contexts for each input
- Include data inputs in context
- Set up context for script execution

## Transaction Signing Process

### 1. Unsigned Transaction Preparation
- Create `UnsignedTransaction` with inputs, data_inputs, and outputs
- Ensure all required data is available

### 2. Proof Generation
- For each input, generate a proof satisfying its guarding script
- Use private keys to create cryptographic proofs
- Generate sigma protocols for complex conditions

### 3. Signed Transaction Assembly
- Combine unsigned transaction with generated proofs
- Create final `Transaction` object
- Verify all proofs satisfy their respective scripts

## Serialization Format

### Serialized Fields Order
1. **inputs** (variable): Collection of input box IDs
2. **data_inputs** (variable): Collection of data input box IDs
3. **outputs** (variable): Collection of output boxes
4. **creation_height** (4 bytes): Creation height as 32-bit unsigned integer (little-endian)

### Detailed Serialization Steps

1. **Inputs Serialization**:
   - Serialize count of inputs as VLQ (Variable-Length Quantity)
   - Serialize each input as 32-byte BoxId

2. **Data Inputs Serialization**:
   - Serialize count of data inputs as VLQ
   - Serialize each data input as 32-byte BoxId

3. **Outputs Serialization**:
   - Serialize count of outputs as VLQ
   - Serialize each output box using ErgoBox serialization format

4. **Creation Height Serialization**:
   - Serialize as 4-byte little-endian unsigned integer

## Deserialization Process

### 1. Header Parsing
- Read and validate the counts of inputs, data_inputs, and outputs
- Verify format compliance

### 2. Component Reconstruction
- Deserialize input box IDs
- Deserialize data input box IDs
- Deserialize output boxes using ErgoBox deserialization

### 3. Validation
- Verify all referenced boxes exist (for inputs and data_inputs)
- Validate output box structure
- Check transaction invariants

## Assembly Operations

### Transaction Builder API

#### `TxBuilder::new(inputs, data_inputs, outputs, ...) -> TxBuilder`
Creates a new transaction builder with specified components.

**Parameters:**
- `inputs`: Vector of input boxes to consume
- `data_inputs`: Vector of read-only data input boxes
- `outputs`: Vector of output boxes to create
- Additional parameters for fee calculation, change handling, etc.

**Returns:**
- `TxBuilder` instance for further configuration

#### `tx_builder.build() -> Result<Transaction, TxBuilderError>`
Assembles and validates the transaction.

**Returns:**
- `Ok(Transaction)` on successful assembly
- `Err(TxBuilderError)` on validation failure

#### `tx_builder.sign_with(secret_keys) -> Result<SignedTransaction, TxSigningError>`
Signs the transaction with provided secret keys.

**Parameters:**
- `secret_keys`: Collection of secret keys for signing

**Returns:**
- `Ok(SignedTransaction)` on successful signing
- `Err(TxSigningError)` on signing failure

### Input Selection

#### `box_selector.select(target_amount, tokens) -> Result<SelectionResult, SelectionError>`
Selects appropriate input boxes to meet target amount and tokens.

**Parameters:**
- `target_amount`: Required nanoErg amount
- `tokens`: Required tokens with amounts

**Returns:**
- `Ok(SelectionResult)` with selected boxes and change calculation
- `Err(SelectionError)` if insufficient funds

## Unsigned Transaction Structure

### Basic Components
An unsigned Ergo transaction consists of:
- **inputs**: List of box IDs to be consumed (without spending proofs)
- **data_inputs**: List of box IDs for read-only access (not consumed)
- **outputs**: List of newly created box candidates (without transaction ID and index)

### Unsigned Transaction Operations

#### `UnsignedTransaction` Structure
The `UnsignedTransaction` struct represents a transaction before signing:

```rust
pub struct UnsignedTransaction {
    pub inputs: TxIoVec<UnsignedInput>,
    pub data_inputs: Option<TxIoVec<DataInput>>,
    pub output_candidates: TxIoVec<ErgoBoxCandidate>,
}
```

#### Binary Serialization for Unsigned Transactions

##### `sigma_serialize() -> Vec<u8>`
Serializes the unsigned transaction to binary bytes using the Sigma serialization protocol.

**Returns:**
- `Vec<u8>` containing serialized unsigned transaction data

##### `sigma_parse_bytes(bytes) -> Result<UnsignedTransaction, SigmaParsingError>`
Deserializes unsigned transaction from binary bytes using the Sigma serialization protocol.

**Parameters:**
- `bytes`: Serialized unsigned transaction data

**Returns:**
- `Ok(UnsignedTransaction)` on successful deserialization
- `Err(SigmaParsingError)` on parsing failure

#### JSON Serialization for Unsigned Transactions

When the "json" feature is enabled, `UnsignedTransaction` supports JSON serialization:

**JSON Structure:**
- `inputs`: Array of unsigned input objects with boxId and extension
- `dataInputs`: Optional array of data input objects
- `outputs`: Array of output box candidate objects

**Example JSON:**
```json
{
  "inputs": [
    {
      "boxId": "a1b2c3d4e5f6...",
      "extension": {}
    }
  ],
  "dataInputs": [
    {
      "boxId": "b2c3d4e5f6..."
    }
  ],
  "outputs": [
    {
      "value": 1000000,
      "ergoTree": "100204a00b08cd02168401...",
      "assets": [],
      "creationHeight": 123456,
      "additionalRegisters": {}
    }
  ]
}
```

**Usage:**
```rust
#[cfg(feature = "json")]
{
    use serde_json;

    let unsigned_tx = UnsignedTransaction::new(inputs, data_inputs, outputs)?;
    let tx_json = serde_json::to_string(&unsigned_tx).unwrap();
    let tx_from_json: UnsignedTransaction = serde_json::from_str(&tx_json).unwrap();
}
```

## Serialization Operations

### Binary Serialization

#### `sigma_serialize() -> Vec<u8>`
Serializes the transaction to binary bytes using the Sigma serialization protocol.

**Returns:**
- `Vec<u8>` containing serialized transaction data

#### `sigma_parse_bytes(bytes) -> Result<Transaction, SigmaParsingError>`
Deserializes transaction from binary bytes using the Sigma serialization protocol.

**Parameters:**
- `bytes`: Serialized transaction data

**Returns:**
- `Ok(Transaction)` on successful deserialization
- `Err(SigmaParsingError)` on parsing failure

### JSON Serialization (Feature: "json")

When the "json" feature is enabled, transactions support JSON serialization:

#### `serde::Serialize` and `serde::Deserialize`
The `Transaction` struct implements Serde traits for JSON serialization.

**JSON Structure:**
- `id`: Transaction ID as hex string
- `inputs`: Array of input objects with boxId and extension
- `dataInputs`: Optional array of data input objects
- `outputs`: Array of output box objects

**Example JSON:**
```json
{
  "id": "d8a3d59c3e4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c",
  "inputs": [
    {
      "boxId": "a1b2c3d4e5f6...",
      "spendingProof": {
        "proofBytes": "...",
        "extension": {}
      }
    }
  ],
  "dataInputs": [...],
  "outputs": [...]
}
```

**Usage:**
```rust
#[cfg(feature = "json")]
{
    use serde_json;

    let tx_json = serde_json::to_string(&transaction).unwrap();
    let tx_from_json: Transaction = serde_json::from_str(&tx_json).unwrap();
}
```

## Validation Checks

### Pre-signing Validation
1. **Balance Check**: Total input value >= total output value
2. **Token Balance**: Token inputs >= token outputs for each token type
3. **Script Validity**: All guarding scripts are syntactically valid
4. **Box Format**: All boxes follow Ergo format

### Post-signing Validation
1. **Proof Verification**: All spending proofs are valid
2. **Context Consistency**: Scripts evaluate correctly in provided context
3. **Block Constraints**: Transaction fits within block limits
4. **Double Spend Prevention**: Inputs not already spent

## Size Considerations

### Transaction Size Limits
- Maximum transaction size: Typically limited by block size
- Input count: Limited by computational complexity
- Output count: Limited by block size and validation cost

### Typical Sizes
- Minimal transaction (1 input, 2 outputs): ~200-400 bytes
- Standard transaction: ~500-1500 bytes
- Complex transaction with many inputs/outputs: Several KB


## Usage Scenarios

### Wallet Transaction Creation
- Select appropriate inputs for payment amount
- Create outputs with recipient scripts
- Sign with wallet's private keys
- Broadcast to network

### Smart Contract Execution
- Prepare inputs that satisfy contract conditions
- Create outputs according to contract logic
- Include data inputs for oracle values
- Execute complex script evaluations

### Mining Pool Operations
- Aggregate transactions from multiple sources
- Verify transaction validity and fees
- Package transactions into blocks
- Optimize transaction ordering

## Error Handling

### Common Error Types
- `TxBuilderError`: Transaction construction failures
- `TxSigningError`: Signature generation failures
- `SelectionError`: Input selection failures
- `SigmaParsingError`: Serialization/deserialization failures
- `ScriptEvaluationError`: Script execution failures

### Error Recovery Strategies
1. **Input Re-selection**: Try different input combinations
2. **Fee Adjustment**: Modify fee amounts if needed
3. **Transaction Splitting**: Split large transactions
4. **Retry Logic**: Retry failed operations with backoff

## References

- Ergo blockchain protocol specification
- UTXO model documentation
- Sigma protocol specification
- ErgoTree serialization specification
- Cryptographic protocol standards