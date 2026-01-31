# Real Redemption with File Output Specification

## Overview
This document specifies the process for performing real redemptions using the basis tracker system with actual blockchain interaction against the Ergo node at `159.89.116.15:11088`. The system will write a redemption request to a file at the end of the process for external processing.

## System Architecture

### Components
1. **Basis Tracker**: Maintains state of IOU notes and AVL tree commitments
2. **Ergo Node Interface**: Communicates with node at `159.89.116.15:11088`
3. **Reserve Scanner**: Monitors reserve boxes on the blockchain
4. **Redemption Processor**: Handles redemption requests
5. **File Output Module**: Writes redemption requests to file for external processing

### Target Ergo Node
- **URL**: `http://159.89.116.15:11088`
- **API Key**: `hello` (default for public access)
- **Network**: Mainnet
- **Capabilities**: 
  - Blockchain scanning via `/scan` endpoints
  - Schnorr signature generation via `/utils/schnorrSign`
  - Transaction submission and validation

## Redemption Process Flow

### Phase 1: Setup and Initialization
1. **Initialize Basis Tracker**
   - Load existing notes and AVL tree state
   - Initialize connection to Ergo node at `159.89.116.15:11088`
   - Verify node connectivity and capabilities

2. **Start Reserve Monitoring**
   - Register scan for reserve boxes matching contract template
   - Begin polling for unspent reserve boxes
   - Update local reserve tracker with current blockchain state

3. **Validate System State**
   - Confirm node connectivity
   - Verify Schnorr signing API availability
   - Check that required reserves exist on blockchain

### Phase 2: Redemption Request Processing
1. **Receive Redemption Request**
   - Parse redemption parameters (issuer pubkey, recipient pubkey, amount)
   - Validate request format and signatures

2. **Locate Associated Reserve**
   - Query blockchain for reserves associated with issuer
   - Verify reserve ownership using public key matching
   - Check collateralization ratio of the reserve

3. **Validate Redemption Eligibility**
   - Verify note exists and is properly signed
   - Check time lock requirements (minimum 1 week from note creation)
   - Confirm sufficient collateral in reserve for redemption amount
   - Validate outstanding debt against requested redemption amount

4. **Generate Redemption Proof**
   - Create AVL tree proof for the note
   - Include current tracker state digest
   - Prepare required Schnorr signatures using node API

### Phase 3: Transaction Preparation
1. **Build Redemption Transaction**
   - Create transaction spending the reserve box
   - Include tracker box as data input for proof verification
   - Generate updated reserve box with reduced collateral
   - Create redemption output to recipient address

2. **Obtain Required Signatures**
   - Generate issuer signature for redemption transaction
   - Request tracker signature from Ergo node via `/utils/schnorrSign`
   - Verify all signatures are valid

3. **Validate Transaction**
   - Check transaction cost and fee requirements
   - Verify all inputs and outputs are properly formed
   - Confirm transaction meets contract requirements

### Phase 4: File Output Generation
1. **Prepare Redemption Request File**
   - Format transaction data in JSON structure
   - Include all required signatures
   - Add metadata (timestamp, request ID, node information)

2. **Write to Output File**
   - Save redemption request to designated file location
   - Use standardized filename format: `redemption_request_{timestamp}_{hash}.json`
   - Ensure file is properly formatted and readable

3. **Log Transaction Details**
   - Record redemption details in system logs
   - Include file path and request ID for tracking
   - Update internal state to reflect pending redemption

## Data Structures

### RedemptionRequestFile
The output file structure containing the complete redemption request:

```rust
pub struct RedemptionRequestFile {
    /// Unique identifier for this redemption request
    pub request_id: String,
    /// Timestamp of request creation
    pub timestamp: u64,
    /// Target Ergo node information
    pub node_info: NodeInfo,
    /// Original redemption parameters
    pub redemption_params: RedemptionParams,
    /// Blockchain transaction data
    pub transaction_data: TransactionData,
    /// Required signatures
    pub signatures: Vec<SignatureData>,
    /// Proof data for verification
    pub proof_data: ProofData,
    /// Estimated transaction fee
    pub fee: u64,
    /// Status of the request
    pub status: RedemptionStatus,
}

pub struct NodeInfo {
    /// Node URL used for this redemption
    pub node_url: String,
    /// Node height at time of request
    pub node_height: u64,
    /// Node version
    pub node_version: String,
}

pub struct RedemptionParams {
    /// Issuer's public key (hex encoded)
    pub issuer_pubkey: String,
    /// Recipient's public key (hex encoded)
    pub recipient_pubkey: String,
    /// Amount to redeem
    pub amount: u64,
    /// Recipient's Ergo address
    pub recipient_address: String,
    /// Note timestamp
    pub note_timestamp: u64,
}

pub struct TransactionData {
    /// Raw transaction bytes (hex encoded)
    pub transaction_bytes: String,
    /// Input boxes involved
    pub inputs: Vec<InputBox>,
    /// Output boxes created
    pub outputs: Vec<OutputBox>,
    /// Data inputs used
    pub data_inputs: Vec<DataInput>,
    /// Script constants
    pub constants: HashMap<String, String>,
}

pub struct SignatureData {
    /// Public key associated with signature
    pub pubkey: String,
    /// Signature bytes (hex encoded)
    pub signature: String,
    /// Signature type
    pub signature_type: String,
}

pub struct ProofData {
    /// AVL tree proof (hex encoded)
    pub avl_proof: String,
    /// Tracker state digest
    pub tracker_digest: String,
    /// Proof validity flag
    pub proof_valid: bool,
}

pub enum RedemptionStatus {
    /// Request prepared but not yet submitted
    Prepared,
    /// Submitted to blockchain
    Submitted,
    /// Confirmed on blockchain
    Confirmed,
    /// Failed validation
    Failed,
}
```

### Example Output File
```json
{
  "request_id": "redemption_1709594_abc123def456",
  "timestamp": 1709594000,
  "node_info": {
    "node_url": "http://159.89.116.15:11088",
    "node_height": 1709594,
    "node_version": "6.0.2"
  },
  "redemption_params": {
    "issuer_pubkey": "02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
    "recipient_pubkey": "03e1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328c",
    "amount": 500000000,
    "recipient_address": "9iJrR3pjgfAp7uVzmY54MSqFh6BEZG8XswWR8qMYj4Mx5e7yv",
    "note_timestamp": 1709000000
  },
  "transaction_data": {
    "transaction_bytes": "040001...",
    "inputs": [
      {
        "box_id": "abcdef1234567890...",
        "value": 1000000000
      }
    ],
    "outputs": [
      {
        "value": 500000000,
        "address": "9iJrR3pjgfAp7uVzmY54MSqFh6BEZG8XswWR8qMYj4Mx5e7yv"
      },
      {
        "value": 499000000,
        "address": "reserve_return_address..."
      }
    ],
    "data_inputs": [
      {
        "box_id": "tracker_box_123..."
      }
    ]
  },
  "signatures": [
    {
      "pubkey": "02d1b60084a5af8dc3e006802a36dddfd09684eaf90164a5ad978b6e9b97eb328b",
      "signature": "02f40cf9d43542868b3e97a790872812574a8be92fd02ce229908d578724c28b925fa689420f9be9f5ddb3d22a6b2a317351008ad38fe222f66aae251f04daae03",
      "signature_type": "schnorr"
    }
  ],
  "proof_data": {
    "avl_proof": "hex_encoded_avl_proof...",
    "tracker_digest": "hex_encoded_digest...",
    "proof_valid": true
  },
  "fee": 1000000,
  "status": "Prepared"
}
```

## API Integration Points

### Ergo Node Endpoints Used
1. **`GET /info`** - Verify node connectivity and get current height
2. **`POST /utils/schnorrSign`** - Generate Schnorr signatures for transactions
3. **`GET /scan/unspentBoxes/{scanId}`** - Get current unspent reserve boxes
4. **`POST /transactions`** - Submit completed redemption transactions (future use)

### Schnorr Signature Integration
The system uses the Ergo node's Schnorr signing API to generate tracker signatures securely:
- Calls `POST /utils/schnorrSign` with address and message
- Uses the node's wallet to sign without exposing private keys
- Integrates signatures into redemption transactions

## File Output Requirements

### Location and Naming
- **Default Directory**: `./redemption_requests/` (relative to tracker execution)
- **Filename Format**: `redemption_request_{timestamp}_{unique_hash}.json`
- **Permissions**: Readable by external processes
- **Backup**: Optionally maintain backup copies

### Content Validation
- **JSON Format**: Valid, properly formatted JSON
- **Schema Compliance**: Matches defined schema structure
- **Signature Verification**: All included signatures are valid
- **Transaction Validity**: Transaction data is well-formed

### Error Handling
- **Write Failures**: Log error and attempt alternative location
- **Disk Space**: Check available space before writing
- **Permissions**: Verify write permissions before processing
- **Corruption**: Validate file integrity after write

## Security Considerations

### Signature Security
- Private keys remain in Ergo node wallet
- All signing performed via node API
- No exposure of private keys to tracker application

### Transaction Security
- All transactions validated before file output
- Fee calculations verified against network requirements
- Input/output amounts balanced correctly

### File Security
- Output files should be secured against tampering
- Access controls for redemption request files
- Audit trail for all file operations

## Error Handling and Recovery

### Node Connection Issues
- Retry mechanism for temporary connection failures
- Fallback to alternative nodes if primary fails
- Graceful degradation when node unavailable

### Transaction Validation Failures
- Detailed error reporting for invalid transactions
- Rollback of internal state changes
- Logging for debugging and analysis

### File Output Failures
- Alternative storage locations
- Notification of file write failures
- Recovery procedures for incomplete writes

## Monitoring and Logging

### System Metrics
- Node connectivity status
- Redemption request volume
- Transaction success rates
- File output statistics

### Log Entries
- Node connection establishment
- Redemption request processing
- File output completion
- Error conditions and recovery

## Testing Requirements

### Integration Tests
- End-to-end redemption flow with file output
- Node connectivity verification
- Schnorr signature generation and validation
- File output format validation

### Edge Cases
- Insufficient collateral scenarios
- Invalid signature handling
- Node unavailability during processing
- File system error conditions

### Performance Tests
- Transaction processing throughput
- File output performance
- Node API response times
- Memory usage during processing

## Deployment Considerations

### Configuration
- Node URL and API key in configuration
- Output directory specification
- Retry and timeout parameters
- Logging level configuration

### Resource Requirements
- Sufficient disk space for output files
- Network connectivity to Ergo node
- Adequate memory for transaction processing
- CPU resources for cryptographic operations

## Transaction Format Reference

The `transaction_bytes` field in the `TransactionData` structure contains the serialized transaction in the format required by the Ergo node's `/wallet/transaction/send` API. For the complete specification of this format, see the [Redemption Transaction Format Specification](./redemption_transaction_format_spec.md).

This specification ensures that the basis tracker can perform real redemptions with actual blockchain interaction against the specified Ergo node and output properly formatted redemption requests to files for external processing.