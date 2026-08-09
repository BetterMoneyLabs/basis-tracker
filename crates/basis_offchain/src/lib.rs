//! Offchain logic for Basis tracker.
//!
//! The placeholder v1 transaction-builder surface is retained only as a
//! historical unit-test fixture. Production construction resumes with the
//! versioned v2 builder.
//!
//! ```compile_fail
//! use basis_offchain::transaction_builder::RedemptionTransactionBuilder;
//! ```

pub mod ergo_tx;
pub mod schnorr;
pub mod signing;
#[cfg(test)]
mod transaction_builder;

#[cfg(test)]
pub mod test_helpers;

// Placeholder for offchain functionality
pub struct OffchainProcessor;

impl OffchainProcessor {
    pub fn new() -> Self {
        Self
    }
}
