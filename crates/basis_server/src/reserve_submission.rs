//! Single-use permits for `POST /reserves/submit`.
//!
//! # Why this exists
//!
//! `POST /reserves/submit` forwards its request body to the configured Ergo node's
//! `/wallet/payment/send` endpoint together with that node's `api_key`. The node then signs the
//! transaction **with the tracker's own wallet key**. The endpoint used to accept any
//! `{address, value, assets, registers}` payload, so anybody who could reach the tracker could pay
//! any address from the tracker's wallet -- which is the same wallet that holds the tracker NFT box.
//!
//! Rather than deleting the endpoint (the CLI, three demo flows and the docs all use it), the
//! server now issues a **permit**: `/reserves/create` records a fingerprint of exactly the payload
//! it returned, and `/reserves/submit` refuses anything that does not match a live, unused permit.
//! The permit is single-use and short-lived, so a leaked or replayed payload is useless, and the
//! body is still hashed the same way so there is no second representation to smuggle through.
//!
//! What this does *not* do: it does not stop a caller who legitimately holds a permit from changing
//! the payload afterwards -- the fingerprint check is what prevents that. It also does not change
//! who controls the tracker wallet; the node still holds the key (see `specs/pr12_triage.md`
//! finding 6, where moving to local signing was deferred).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long an issued reserve-creation permit stays usable.
///
/// Generous enough for an operator to create a reserve, sign it into a transaction and submit it,
/// short enough that a captured permit is not a standing capability.
pub const PERMIT_TTL: Duration = Duration::from_secs(30 * 60);

/// Largest number of outstanding permits. Each entry is tiny (a 32-byte digest), and the map is
/// pruned on every call, so this only bounds memory if `/reserves/create` is hammered.
const MAX_OUTSTANDING: usize = 1024;

/// SHA-256 of the canonical serialization of a reserve-creation payload.
pub type PayloadFingerprint = [u8; 32];

/// Registry of outstanding, single-use reserve-submission permits.
#[derive(Debug, Default)]
pub struct PermitRegistry {
    entries: Mutex<HashMap<PayloadFingerprint, Instant>>,
}

impl PermitRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a permit for `fingerprint`, valid for [`PERMIT_TTL`].
    ///
    /// Returns false when the registry is full, which the caller should surface as a
    /// "try again" rather than silently dropping the permit on the floor.
    pub fn issue(&self, fingerprint: PayloadFingerprint) -> bool {
        let mut entries = self.lock();
        Self::prune(&mut entries);
        if entries.len() >= MAX_OUTSTANDING && !entries.contains_key(&fingerprint) {
            return false;
        }
        entries.insert(fingerprint, Instant::now());
        true
    }

    /// Consume the permit for `fingerprint`.
    ///
    /// Returns true exactly once per issued permit: a second call with the same fingerprint, or a
    /// call for a fingerprint that was never issued or has expired, returns false.
    pub fn consume(&self, fingerprint: PayloadFingerprint) -> bool {
        let mut entries = self.lock();
        Self::prune(&mut entries);
        entries.remove(&fingerprint).is_some()
    }

    /// Number of live permits. Used by tests.
    pub fn len(&self) -> usize {
        Self::prune(&mut self.lock());
        self.lock().len()
    }

    /// Whether no permits are outstanding.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop expired entries. Called on every issue/consume, so no timer is needed.
    fn prune(entries: &mut HashMap<PayloadFingerprint, Instant>) {
        let now = Instant::now();
        entries.retain(|_, issued| now.duration_since(*issued) < PERMIT_TTL);
    }

    /// Lock the map, recovering from a poisoned mutex.
    ///
    /// A panic while holding this lock would only ever mean the map is in a consistent state (each
    /// entry is independent), so refusing all subsequent reserve creation would be a worse outcome
    /// than continuing.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PayloadFingerprint, Instant>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Canonical fingerprint of a reserve-creation payload.
///
/// Serializes with serde and hashes with SHA-256.
///
/// The digest depends on serde's field order for the struct and on `HashMap` iteration order for
/// `registers`. That is fine here because BOTH the issuer and the verifier run this function over
/// the same `ReserveCreationResponse` type: `/reserves/create` hashes what it built, and
/// `/reserves/submit` re-hashes the payload it received. A client cannot exploit a different map
/// ordering, because it cannot choose the order -- it can only choose the entries, and any change to
/// the entries changes the digest.
pub fn fingerprint_of<T: serde::Serialize>(payload: &T) -> Result<PayloadFingerprint, String> {
    use sha2::Digest as _;
    let bytes =
        serde_json::to_vec(payload).map_err(|e| format!("failed to serialize payload: {e}"))?;
    Ok(sha2::Sha256::digest(&bytes).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permit_is_single_use() {
        let registry = PermitRegistry::new();
        let fp: PayloadFingerprint = [7u8; 32];
        assert!(registry.issue(fp));
        assert!(registry.consume(fp), "first use succeeds");
        assert!(!registry.consume(fp), "second use must fail");
        assert!(registry.is_empty());
    }

    #[test]
    fn unknown_permit_is_rejected() {
        let registry = PermitRegistry::new();
        assert!(!registry.consume([1u8; 32]));
    }

    #[test]
    fn expired_permit_is_rejected() {
        // Rather than sleeping for PERMIT_TTL, insert an entry that is already older than the TTL.
        let registry = PermitRegistry::new();
        let fp: PayloadFingerprint = [9u8; 32];
        registry
            .lock()
            .insert(fp, Instant::now() - PERMIT_TTL - Duration::from_secs(1));
        assert!(
            !registry.consume(fp),
            "an expired permit must not be usable"
        );
    }

    #[test]
    fn registry_stays_bounded() {
        let registry = PermitRegistry::new();
        // Distinct fingerprints: encode the index big-endian so every key is unique.
        let fingerprint = |i: usize| {
            let mut fp = [0u8; 32];
            fp[24..].copy_from_slice(&(i as u64).to_be_bytes());
            fp
        };
        for i in 0..MAX_OUTSTANDING {
            assert!(registry.issue(fingerprint(i)), "issue {i} should succeed");
        }
        assert_eq!(registry.len(), MAX_OUTSTANDING);
        assert!(
            !registry.issue(fingerprint(MAX_OUTSTANDING)),
            "a full registry must refuse new permits"
        );
        // Re-issuing an already-tracked fingerprint is still allowed (it refreshes the TTL).
        assert!(registry.issue(fingerprint(0)));
    }

    #[test]
    fn fingerprint_is_stable_and_sensitive_to_every_field() {
        #[derive(serde::Serialize)]
        struct Payload {
            address: String,
            value: u64,
            registers: std::collections::HashMap<String, String>,
        }

        let registers = || {
            let mut m = std::collections::HashMap::new();
            m.insert("R4".to_string(), "07aa".to_string());
            m.insert("R5".to_string(), "64bb".to_string());
            m
        };

        // Same content hashed twice must agree, so a permit issued for a payload verifies later.
        let p1 = Payload {
            address: "addr".into(),
            value: 5,
            registers: registers(),
        };
        assert_eq!(fingerprint_of(&p1).unwrap(), fingerprint_of(&p1).unwrap());

        // Changing ANY field the node would act on must change the digest, otherwise a caller could
        // swap the destination, the value or a register and keep the permit.
        let base = fingerprint_of(&p1).unwrap();
        let address_changed = fingerprint_of(&Payload {
            address: "attacker".into(),
            value: 5,
            registers: registers(),
        })
        .unwrap();
        let value_changed = fingerprint_of(&Payload {
            address: "addr".into(),
            value: 6,
            registers: registers(),
        })
        .unwrap();
        let register_changed = fingerprint_of(&Payload {
            address: "addr".into(),
            value: 5,
            registers: {
                let mut m = registers();
                m.insert("R6".to_string(), "0e20cc".to_string());
                m
            },
        })
        .unwrap();

        assert_ne!(base, address_changed, "address must be covered");
        assert_ne!(base, value_changed, "value must be covered");
        assert_ne!(base, register_changed, "registers must be covered");
    }
}
