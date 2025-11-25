# Ergo Node Scans in the Basis Tracker Server

## Overview

The Basis Tracker server implements multiple blockchain scanning systems to monitor the Ergo blockchain for different types of events. The server uses the Ergo node's `/scan` API to efficiently track relevant boxes without having to scan the entire blockchain. This system enables real-time monitoring of reserve-related events and tracker state commitments.

## Scanner Types

The system implements two main types of scanners:

1. **Reserve Scanner**: Monitors Basis reserve contracts for creation, top-ups, redemptions, and spending events
2. **Tracker Scanner**: Monitors Basis tracker state commitment boxes for cross-verification and state synchronization

## Reserve Scanner

### Architecture

The reserve scanner is implemented in the `basis_store::ergo_scanner` module and follows a synchronized design pattern with the following key components:

#### ServerState Structure

The main scanner state includes:
- Configuration for the Ergo node connection
- Synchronized inner state for tracking scan status
- HTTP client for communicating with the Ergo node
- Reserve tracker for maintaining in-memory state
- Persistence storage for metadata and reserve data

#### Configuration (NodeConfig)

The scanner configuration includes:
- `start_height`: Starting block height for scanning (optional)
- `reserve_contract_p2s`: P2S address of the Basis reserve contract to scan for
- `node_url`: URL of the Ergo node to connect to
- `scan_name`: Name for the registered scan (default: "Basis Reserve Scanner")
- `api_key`: Authentication key for the Ergo node

### Scan Registration Process

#### Registering a Reserve Scan

The scanner registers with the Ergo node using the `/scan/register` endpoint:

1. **Contract Preparation**: The reserve contract P2S address is serialized into the format expected by the Ergo node, wrapped in a ByteArrayConstant
2. **Registration Payload**: A JSON payload is created with:
   - `scanName`: Name for the scan
   - `walletInteraction`: Set to "off" for tracking-only scans
   - `trackingRule`: Specifies to track boxes that contain the contract in register R1
   - `removeOffchain`: Set to false to keep spent boxes in the scan
3. **API Request**: An HTTP POST request is sent to `/scan/register` with authentication headers if an API key is provided
4. **Response Handling**: The scanner processes the response to extract the scan ID
5. **Persistence**: The scan ID is stored in local metadata storage for future use

### Scan Verification

The system implements a verification mechanism to ensure registered scans are still active:
- Verification occurs every 4 hours to prevent unnecessary node requests
- The scanner queries the `/scan/listAll` endpoint to confirm the scan ID still exists
- If verification fails due to endpoint unavailability, the system assumes the scan exists to avoid unnecessary re-registration
- Invalid scan IDs are removed from local storage and the system re-registers

### Blockchain Monitoring Process

#### Background Scanner Loop

The scanner operates in a continuous background loop with the following steps:
1. **Height Check**: Retrieve the current blockchain height from the Ergo node
2. **Scan Validation**: Verify that the scanner has a valid registered scan
3. **Box Processing**: If the current height has advanced, retrieve and process unspent boxes from the registered scan
4. **Error Handling**: Implement retry logic with exponential backoff for failed operations

#### Retrieving Scan Boxes

The scanner fetches unspent boxes matching the registered scan using the `/scan/unspentBoxes/{scanId}` endpoint:
- Uses the stored scan ID to query for matching boxes
- Returns boxes that match the reserve contract tracking rule
- Processes each box according to the reserve contract structure

#### Processing Reserve Boxes

Each box retrieved from the scan is processed as follows:
1. **Box Parsing**: Extract box information including ID, value, creation height, and register data
2. **Reserve Information Extraction**: Parse key registers:
   - R4: Contains the owner's public key
   - R5: Contains an optional tracker NFT ID
   - Box value: The collateral amount
3. **State Updates**: Update both in-memory tracker and persistent storage with the new reserve information
4. **Spent Box Detection**: Compare current scan results with previously known reserves to identify spent boxes

## Tracker Scanner

### Architecture

The tracker scanner is implemented in the `basis_store::tracker_scanner` module and monitors tracker state commitment boxes with the following key components:

#### TrackerServerState Structure

The main tracker scanner state includes:
- Configuration for the Ergo node connection
- Synchronized inner state for tracking scan status
- HTTP client for communicating with the Ergo node
- Tracker state manager for maintaining state commitment information
- Persistence storage for metadata and tracker data

#### Configuration (TrackerNodeConfig)

The tracker scanner configuration includes:
- `start_height`: Starting block height for scanning (optional)
- `tracker_nft_id`: Hex-encoded ID of the tracker NFT to scan for
- `node_url`: URL of the Ergo node to connect to
- `scan_name`: Name for the registered scan (default: "tracker_boxes")
- `api_key`: Authentication key for the Ergo node

### Tracker Scan Registration

The tracker scanner registers using the `containsAsset` predicate rather than register-based scanning:

1. **Asset Preparation**: Uses the tracker NFT ID to create a containsAsset tracking rule
2. **Registration Payload**: A JSON payload is created with:
   - `scanName`: Name for the scan
   - `walletInteraction`: Set to "off" for tracking-only scans
   - `trackingRule`: Specifies to track boxes that contain the tracker NFT in their assets
   - `removeOffchain`: Set to true to remove spent boxes from the scan
3. **API Request**: An HTTP POST request is sent to `/scan/register` with authentication headers if an API key is provided

### Processing Tracker Boxes

Tracker boxes contain different data than reserve boxes:
1. **Asset Validation**: Verify the box contains the tracker NFT
2. **Register Information Extraction**: Parse key registers:
   - R4: Contains the tracker's public key
   - R5: Contains the state commitment digest
   - R6: Contains the last verified height
3. **Tracker State Updates**: Update the tracker state manager with cross-verification information

## Data Models

### ScanBox Structure

The ScanBox struct represents boxes retrieved from the Ergo node scan:
- `box_id`: Unique identifier for the Ergo box
- `value`: Amount of nanoERG in the box
- `ergo_tree`: The script protecting the box
- `creation_height`: Block height when the box was created
- `transaction_id`: ID of the transaction that created the box
- `additional_registers`: Map of register values (R4, R5, etc.)
- `assets`: List of tokens held in the box

### ReserveEvent Types

The system tracks various reserve-related events:
- `ReserveCreated`: New reserve box creation
- `ReserveToppedUp`: Additional collateral added to existing reserve
- `ReserveRedeemed`: Redemption processed from a reserve
- `ReserveSpent`: Reserve box spent/closed

## Error Handling and Recovery

Both scanners implement robust error handling:
- **Connection Failures**: Automatic retry with backoff for network issues
- **Scan Registration Failures**: Re-registration attempts when scans become invalid
- **Parsing Errors**: Individual box failures don't stop processing of other boxes
- **Persistence Errors**: Warnings logged but processing continues
- **Node Unavailability**: Graceful degradation when Ergo node is temporarily unreachable

## Persistence and Storage

### Metadata Storage

- Stores scan IDs and associated names for persistence across server restarts
- Maintains last verification timestamps to optimize scan validation

### Reserve Storage

- Persists reserve information to disk using the ReserveStorage component
- Maintains a complete history of reserve states for historical queries
- Synchronized with in-memory tracker state

### Tracker Storage

- Persists tracker box information using the TrackerStorage component
- Maintains state commitment information for cross-verification
- Supports tracker state synchronization across nodes

## Configuration and Deployment

### Node Configuration

Both scanners can be configured with different Ergo node endpoints:
- Mainnet: For production deployment
- Testnet: For testing and development
- Local Node: For development and debugging

### Authentication

Supports API key authentication for Ergo node access:
- API key is included in HTTP headers for all requests
- Configurable per-node to support different access policies
- Secure handling to prevent credential exposure

## Performance Considerations

### Optimized Scanning

- Uses Ergo node's native scan functionality instead of polling the entire blockchain
- Efficient register-based filtering for reserve scanner and asset-based filtering for tracker scanner
- Asynchronous processing to minimize blocking operations

### Resource Management

- Memory-efficient in-memory tracking with database persistence
- Configurable polling intervals to balance between real-time updates and resource usage
- Thread-safe design to handle concurrent access from API requests

This dual scanning system provides the foundation for the Basis Tracker server to maintain real-time awareness of both reserve state and tracker state commitments on the Ergo blockchain, enabling accurate collateralization ratios, cross-verification, and comprehensive protocol monitoring.