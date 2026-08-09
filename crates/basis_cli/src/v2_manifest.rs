//! Dormant v2 client admission boundary.
//!
//! No proof generation or signing callback can be entered through this module
//! with a raw manifest. The callback receives only the opaque validation token
//! produced after the exact signer-side manifest checks have succeeded.
//! This is admission only: there is no v2 prover, signer, wallet integration,
//! submission, or broadcast implementation here. A future private integration
//! must consume only the validated token and bind reserve and funding proofs to
//! the same exact boxes carried by its manifest.
//!
//! ```compile_fail
//! use basis_store::basis_v2_builder::{
//!     V2RedemptionManifest, ValidatedV2RedemptionManifest,
//! };
//!
//! fn bypass(manifest: &V2RedemptionManifest) {
//!     let _ = ValidatedV2RedemptionManifest { manifest };
//! }
//! ```

use anyhow::Result;
use basis_store::basis_v2_builder::{
    with_validated_v2_redemption_manifest, with_validated_v2_redemption_manifest_bytes,
    V2RedemptionManifest, V2SigningIntent, ValidatedV2RedemptionManifest,
};

/// Validate the complete v2 manifest before entering a fallible proof/signing
/// callback seam. This does not implement the callback's proof or signing
/// operation. Production remains dormant because a `V2SigningIntent` cannot be
/// assembled until the independent header-ancestry authority supplies its
/// opaque verified tip.
pub fn with_validated_v2_manifest<T>(
    manifest: &V2RedemptionManifest,
    intent: &V2SigningIntent,
    callback: impl FnOnce(ValidatedV2RedemptionManifest<'_>) -> Result<T>,
) -> Result<T> {
    with_validated_v2_redemption_manifest(manifest, intent, callback).map_err(anyhow::Error::new)?
}

/// Bounded raw-byte admission seam for a future CLI transport. Raw JSON is
/// size-checked and parsed before the same opaque validation token can reach a
/// proof or signing callback.
pub fn with_validated_v2_manifest_bytes<T>(
    encoded: &[u8],
    intent: &V2SigningIntent,
    callback: impl FnOnce(ValidatedV2RedemptionManifest<'_>) -> Result<T>,
) -> Result<T> {
    with_validated_v2_redemption_manifest_bytes(encoded, intent, callback)
        .map_err(anyhow::Error::new)?
}
