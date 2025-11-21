//! Optimized Fjall-based storage for Basis Tracker trees

use crate::errors::TreeError;
use crate::storage::{TreeNode, TreeOperation, TreeCheckpoint};
use fjall::{Config, Keyspace, Partition, PartitionCreateOptions, CompressionType};
use std::path::Path;

/// Configuration for Fjall storage
#[derive(Debug, Clone)]
pub struct FjallStorageConfig {
    /// Maximum partition size in bytes
    pub max_partition_size: usize,
    /// Batch size for operations
    pub batch_size: usize,
    /// Enable compression
    pub compression: bool,
}

impl Default for FjallStorageConfig {
    fn default() -> Self {
        Self {
            max_partition_size: 1024 * 1024 * 1024, // 1GB
            batch_size: 1000,
            compression: true,
        }
    }
}

/// Optimized Fjall-based tree storage
pub struct FjallTreeStorage {
    /// Keyspace for all tree data
    keyspace: Keyspace,
    /// Node storage partition
    node_partition: Partition,
    /// Operation log partition
    operation_partition: Partition,
    /// Checkpoint storage partition
    checkpoint_partition: Partition,
    /// Metadata partition
    metadata_partition: Partition,
    /// Current operation sequence
    current_sequence: u64,
    /// Configuration
    config: FjallStorageConfig,
}

impl FjallTreeStorage {
    /// Open or create a new Fjall-based tree storage
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, TreeError> {
        Self::open_with_config(path, FjallStorageConfig::default())
    }

    /// Open storage with custom configuration
    pub fn open_with_config<P: AsRef<Path>>(
        path: P,
        config: FjallStorageConfig,
    ) -> Result<Self, TreeError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| TreeError::StorageError(format!("Failed to open database: {}", e)))?;

        // Configure partition options
        let compression_type = if config.compression {
            CompressionType::Lz4
        } else {
            CompressionType::None
        };
        
        let partition_options = PartitionCreateOptions::default()
            .compression(compression_type);

        // Open partitions
        let node_partition = keyspace
            .open_partition("tree_nodes", partition_options.clone())
            .map_err(|e| TreeError::StorageError(format!("Failed to open node partition: {}", e)))?;

        let operation_partition = keyspace
            .open_partition("tree_operations", partition_options.clone())
            .map_err(|e| TreeError::StorageError(format!("Failed to open operation partition: {}", e)))?;

        let checkpoint_partition = keyspace
            .open_partition("tree_checkpoints", partition_options.clone())
            .map_err(|e| TreeError::StorageError(format!("Failed to open checkpoint partition: {}", e)))?;

        let metadata_partition = keyspace
            .open_partition("tree_metadata", partition_options)
            .map_err(|e| TreeError::StorageError(format!("Failed to open metadata partition: {}", e)))?;

        // Load current sequence
        let current_sequence = Self::load_current_sequence(&metadata_partition)?;

        Ok(Self {
            keyspace,
            node_partition,
            operation_partition,
            checkpoint_partition,
            metadata_partition,
            current_sequence,
            config,
        })
    }

    /// Store a tree node
    pub fn store_node(&self, node: &TreeNode) -> Result<(), TreeError> {
        let key = Self::node_key(&node.digest);
        let value = bincode::serialize(node)
            .map_err(|e| TreeError::StorageError(format!("Failed to serialize node: {}", e)))?;

        self.node_partition
            .insert(&key, &value)
            .map_err(|e| TreeError::StorageError(format!("Failed to store node: {}", e)))?;

        Ok(())
    }

    /// Batch store multiple nodes
    pub fn batch_store_nodes(&self, nodes: &[TreeNode]) -> Result<(), TreeError> {
        for chunk in nodes.chunks(self.config.batch_size) {
            for node in chunk {
                let key = Self::node_key(&node.digest);
                let value = bincode::serialize(node)
                    .map_err(|e| TreeError::StorageError(format!("Failed to serialize node: {}", e)))?;
                
                self.node_partition.insert(&key, &value)
                    .map_err(|e| TreeError::StorageError(format!("Failed to store node: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Retrieve a tree node
    pub fn get_node(&self, digest: &[u8]) -> Result<Option<TreeNode>, TreeError> {
        let key = Self::node_key(digest);

        match self.node_partition.get(&key) {
            Ok(Some(value_bytes)) => {
                let node = bincode::deserialize(&value_bytes)
                    .map_err(|e| TreeError::StorageError(format!("Failed to deserialize node: {}", e)))?;
                Ok(Some(node))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(TreeError::StorageError(format!("Failed to get node: {}", e))),
        }
    }

    /// Batch retrieve multiple nodes
    pub fn batch_get_nodes(&self, digests: &[Vec<u8>]) -> Result<Vec<Option<TreeNode>>, TreeError> {
        let mut results = Vec::with_capacity(digests.len());

        for digest in digests {
            let node = self.get_node(digest)?;
            results.push(node);
        }

        Ok(results)
    }

    /// Get nodes by digest range
    pub fn get_nodes_by_digest_range(
        &self,
        start_digest: &[u8],
        end_digest: &[u8],
    ) -> Result<Vec<TreeNode>, TreeError> {
        let start_key = Self::node_key(start_digest);
        let end_key = Self::node_key(end_digest);

        let mut nodes = Vec::new();

        for item in self.node_partition.range(start_key..=end_key) {
            let (_key_bytes, value_bytes) = item
                .map_err(|e| TreeError::StorageError(format!("Failed to iterate nodes: {}", e)))?;

            let node: TreeNode = bincode::deserialize(&value_bytes)
                .map_err(|e| TreeError::StorageError(format!("Failed to deserialize node: {}", e)))?;

            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Log a tree operation
    pub fn log_operation(&mut self, operation: TreeOperation) -> Result<(), TreeError> {
        let key = Self::operation_key(operation.sequence_number);
        let value = bincode::serialize(&operation)
            .map_err(|e| TreeError::StorageError(format!("Failed to serialize operation: {}", e)))?;

        self.operation_partition
            .insert(&key, &value)
            .map_err(|e| TreeError::StorageError(format!("Failed to store operation: {}", e)))?;

        // Update current sequence
        self.current_sequence = operation.sequence_number;
        self.store_current_sequence()?;

        Ok(())
    }

    /// Batch log operations
    pub fn batch_log_operations(&mut self, operations: &[TreeOperation]) -> Result<(), TreeError> {
        for chunk in operations.chunks(self.config.batch_size) {
            for operation in chunk {
                let key = Self::operation_key(operation.sequence_number);
                let value = bincode::serialize(operation)
                    .map_err(|e| TreeError::StorageError(format!("Failed to serialize operation: {}", e)))?;
                
                self.operation_partition.insert(&key, &value)
                    .map_err(|e| TreeError::StorageError(format!("Failed to store operation: {}", e)))?;
            }
            
            // Update current sequence
            if let Some(last_op) = chunk.last() {
                self.current_sequence = last_op.sequence_number;
            }
        }

        self.store_current_sequence()?;
        Ok(())
    }

    /// Get next operation sequence number
    pub fn next_sequence_number(&mut self) -> u64 {
        self.current_sequence += 1;
        self.current_sequence
    }

    /// Get operations in sequence range
    pub fn get_operations(&self, start: u64, end: u64) -> Result<Vec<TreeOperation>, TreeError> {
        let mut operations = Vec::new();

        for seq in start..=end {
            let key = Self::operation_key(seq);
            if let Some(value_bytes) = self.operation_partition.get(&key)
                .map_err(|e| TreeError::StorageError(format!("Failed to get operation: {}", e)))? {
                let operation = bincode::deserialize(&value_bytes)
                    .map_err(|e| TreeError::StorageError(format!("Failed to deserialize operation: {}", e)))?;
                operations.push(operation);
            }
        }

        Ok(operations)
    }

    /// Store a checkpoint
    pub fn store_checkpoint(&self, checkpoint: &TreeCheckpoint) -> Result<(), TreeError> {
        let key = Self::checkpoint_key(checkpoint.checkpoint_id);
        let value = bincode::serialize(checkpoint)
            .map_err(|e| TreeError::StorageError(format!("Failed to serialize checkpoint: {}", e)))?;

        self.checkpoint_partition
            .insert(&key, &value)
            .map_err(|e| TreeError::StorageError(format!("Failed to store checkpoint: {}", e)))?;

        Ok(())
    }

    /// Get latest checkpoint
    pub fn get_latest_checkpoint(&self) -> Result<Option<TreeCheckpoint>, TreeError> {
        let mut latest_checkpoint: Option<TreeCheckpoint> = None;

        for item in self.checkpoint_partition.iter() {
            let (_key_bytes, value_bytes) = item
                .map_err(|e| TreeError::StorageError(format!("Failed to iterate checkpoints: {}", e)))?;

            let checkpoint: TreeCheckpoint = bincode::deserialize(&value_bytes)
                .map_err(|e| TreeError::StorageError(format!("Failed to deserialize checkpoint: {}", e)))?;

            if latest_checkpoint.as_ref().map_or(true, |c| checkpoint.checkpoint_id > c.checkpoint_id) {
                latest_checkpoint = Some(checkpoint);
            }
        }

        Ok(latest_checkpoint)
    }

    /// Get all nodes in storage
    pub fn get_all_nodes(&self) -> Result<Vec<TreeNode>, TreeError> {
        let mut nodes = Vec::new();

        for item in self.node_partition.iter() {
            let (_key_bytes, value_bytes) = item
                .map_err(|e| TreeError::StorageError(format!("Failed to iterate nodes: {}", e)))?;

            let node: TreeNode = bincode::deserialize(&value_bytes)
                .map_err(|e| TreeError::StorageError(format!("Failed to deserialize node: {}", e)))?;

            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Get current sequence number
    pub fn current_sequence(&self) -> u64 {
        self.current_sequence
    }

    /// Helper: Generate node storage key
    fn node_key(digest: &[u8]) -> Vec<u8> {
        let mut key = b"nodes/".to_vec();
        key.extend_from_slice(digest);
        key
    }

    /// Helper: Generate operation storage key
    fn operation_key(sequence_number: u64) -> Vec<u8> {
        let mut key = b"operations/".to_vec();
        key.extend_from_slice(&sequence_number.to_be_bytes());
        key
    }

    /// Helper: Generate checkpoint storage key
    fn checkpoint_key(checkpoint_id: u64) -> Vec<u8> {
        let mut key = b"checkpoints/".to_vec();
        key.extend_from_slice(&checkpoint_id.to_be_bytes());
        key
    }

    /// Load current sequence number from metadata
    fn load_current_sequence(metadata_partition: &Partition) -> Result<u64, TreeError> {
        match metadata_partition.get(b"current_sequence") {
            Ok(Some(value_bytes)) => {
                if value_bytes.len() == 8 {
                    Ok(u64::from_be_bytes(value_bytes[0..8].try_into().unwrap()))
                } else {
                    Ok(0) // Default to 0 if invalid
                }
            }
            Ok(None) => Ok(0), // Start from 0 if not found
            Err(e) => Err(TreeError::StorageError(format!("Failed to get current sequence: {}", e))),
        }
    }

    /// Store current sequence number to metadata
    fn store_current_sequence(&self) -> Result<(), TreeError> {
        let value_bytes = self.current_sequence.to_be_bytes().to_vec();
        self.metadata_partition
            .insert(b"current_sequence", &value_bytes)
            .map_err(|e| TreeError::StorageError(format!("Failed to store current sequence: {}", e)))?;
        Ok(())
    }
}