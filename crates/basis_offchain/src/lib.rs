//! Offchain logic for Basis tracker

pub mod schnorr;
pub mod transaction_builder;

#[cfg(test)]
pub mod test_helpers;

// Placeholder for offchain functionality
pub struct OffchainProcessor;

impl OffchainProcessor {
    pub fn new() -> Self {
        Self
    }
}
