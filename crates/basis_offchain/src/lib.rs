//! Offchain logic for Basis tracker

pub mod ergo_tx;
pub mod schnorr;
pub mod signing;
pub mod transaction_builder;

#[cfg(test)]
pub mod test_helpers;

// Placeholder for offchain functionality
pub struct OffchainProcessor;

impl Default for OffchainProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl OffchainProcessor {
    pub fn new() -> Self {
        Self
    }
}
