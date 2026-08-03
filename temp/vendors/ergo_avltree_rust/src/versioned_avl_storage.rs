use crate::batch_avl_prover::*;
use crate::batch_node::*;
use crate::operation::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use anyhow::Result;
use core::iter::Iterator;

///
/// Interface for persistent versioned
///
pub trait VersionedAVLStorage {
    ///
    /// Synchronize storage with prover's state
    ///
    /// @param batchProver - prover to synchronize storage with
    /// @return
    ///
    fn update(
        &mut self,
        prover: &mut BatchAVLProver,
        additional_data: Vec<(ADKey, ADValue)>,
    ) -> Result<()>;

    ///
    /// Return root node and tree height at version
    ///
    fn rollback(&mut self, version: &ADDigest) -> Result<(NodeId, usize)>;

    ///
    /// Current version of storage. Version is prover's root hash value during last storage update.
    ///
    /// @return current version, if any; None is storage is empty
    ///
    fn version(&self) -> Option<ADDigest>;

    ///
    /// If storage is empty
    ///
    /// @return true is storage is empty, false otherwise
    ///
    fn is_empty(&self) -> bool {
        self.version().is_none()
    }

    fn non_empty(&self) -> bool {
        !self.is_empty()
    }

    ////
    /// Versions store keeps and can rollback to.
    ///
    /// @return versions store keeps
    ///
    fn rollback_versions<'a>(&'a self) -> Box<dyn Iterator<Item = ADDigest> + 'a>;
}
