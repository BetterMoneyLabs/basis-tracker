//! Offchain logic for Basis tracker.
//!
//! The v1 transaction-builder and signing surfaces are removed. Production
//! construction resumes only through the exact versioned v2 manifest boundary.
//!
//! ```compile_fail
//! use basis_offchain::transaction_builder::RedemptionTransactionBuilder;
//! ```

pub mod ergo_tx;
pub mod schnorr;

#[cfg(test)]
pub mod test_helpers;

// Placeholder for offchain functionality
pub struct OffchainProcessor;

impl OffchainProcessor {
    pub fn new() -> Self {
        Self
    }
}
