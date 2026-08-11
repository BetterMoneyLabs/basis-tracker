//! Redemption-time acceptance policy compliance check
//!
//! When a redemption request arrives, the tracker verifies that paying out does not
//! *newly* violate the acceptance policy of any **other** debt holder of the same
//! reserve (issuer). The only predicate a redemption can newly violate is
//! `Collateralization { min_ratio }`: paying out amount `A` moves the issuer's ratio
//! from `C/D` to `(C - A - fee)/(D - A)`, which is strictly worse when `C < D`.
//!
//! If **all** other holders' policies are already violated before the redemption
//! (deeply undercollateralized reserve), blocking redemption forever would deadlock,
//! so a FIFO fallback applies: only the holder of the issuer's oldest outstanding
//! note may redeem.

use std::collections::{BTreeMap, HashMap};

use basis_core::acceptance::{AcceptanceConfig, DefaultPolicy, PredicateConfig};
use basis_store::persistence::AcceptancePolicyStorage;
use basis_store::{IouNote, PubKey};

/// Error returned when a redemption is rejected by the acceptance policy check
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RedemptionPolicyError {
    /// Redemption would newly violate another holder's acceptance policy
    #[error(
        "redemption would newly violate acceptance policy of holder {holder_pubkey}: {detail}"
    )]
    WouldViolatePolicy {
        /// Hex-encoded public key of the holder whose policy would be violated
        holder_pubkey: String,
        /// Human-readable explanation (ratios before/after)
        detail: String,
    },
    /// Distressed reserve (all other holders already violated): only the oldest
    /// outstanding note may redeem (FIFO fallback)
    #[error(
        "reserve is distressed; only the oldest outstanding note (timestamp {oldest_timestamp}) may redeem, requested note has timestamp {requested_timestamp}"
    )]
    NotOldestNote {
        /// Timestamp of the issuer's oldest outstanding note
        oldest_timestamp: u64,
        /// Timestamp of the redeemer's note
        requested_timestamp: u64,
    },
}

/// Outstanding debt of a note
fn outstanding(note: &IouNote) -> u64 {
    note.amount_collected.saturating_sub(note.amount_redeemed)
}

/// Parse an acceptance policy JSON string into an `AcceptanceConfig`.
///
/// Same workaround as `api::parse_acceptance_policy_json`: with serde_json's
/// `arbitrary_precision` feature (pulled in by ergo-lib), internally tagged enums
/// with `f64` fields fail to deserialize directly from a stream, so we first parse
/// to `Value` and then to the target type.
fn parse_policy_json(policy_json: &str) -> Result<AcceptanceConfig, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(policy_json)?;
    serde_json::from_value(value)
}

/// Check whether a hex-encoded holder list contains the given public key
fn holders_contain(holders: &[String], pubkey: &PubKey) -> bool {
    let pubkey_hex = hex::encode(pubkey);
    holders.iter().any(|h| h == &pubkey_hex)
}

/// Evaluation context shared by all predicate evaluations in this check
struct EvalEnv<'a> {
    /// Issuer (reserve owner) public key
    issuer_pubkey: &'a PubKey,
    /// Refund initiation pending on the reserve (R7 height > 0)
    refund_pending: bool,
    /// Simulated collateral (nanoERG)
    collateral: u64,
    /// Simulated total outstanding debt (nanoERG)
    debt: u64,
    /// Per-holder cumulative total debt (for whitelist `max_debt` checks)
    holder_total_debt: u64,
}

/// Evaluate a single predicate by name against the environment.
///
/// `Collateralization` leaves are evaluated against the simulated collateral/debt
/// values; all other leaf kinds are unaffected by someone else's redemption and are
/// evaluated against their current values. Composite predicates preserve the
/// AllOf/AnyOf/Not semantics of the runtime predicates.
fn eval_predicate(configs: &BTreeMap<String, &PredicateConfig>, name: &str, env: &EvalEnv) -> bool {
    let config = match configs.get(name) {
        Some(c) => c,
        None => {
            tracing::warn!(
                "Redemption policy check: missing predicate reference '{}'",
                name
            );
            return false;
        }
    };

    match config {
        PredicateConfig::Whitelist {
            holders, max_debt, ..
        } => {
            if !holders_contain(holders, env.issuer_pubkey) {
                return false;
            }
            if let Some(max) = max_debt {
                if env.holder_total_debt > *max {
                    return false;
                }
            }
            true
        }
        PredicateConfig::Blacklist { holders, .. } => !holders_contain(holders, env.issuer_pubkey),
        PredicateConfig::Collateralization { min_ratio, .. } => {
            if env.debt == 0 {
                // No debt means fully collateralized (same as CollateralizationPredicate)
                return true;
            }
            let ratio = env.collateral as f64 / env.debt as f64;
            ratio >= *min_ratio
        }
        PredicateConfig::AllOf { predicates, .. } => {
            predicates.iter().all(|p| eval_predicate(configs, p, env))
        }
        PredicateConfig::AnyOf { predicates, .. } => {
            !predicates.is_empty() && predicates.iter().any(|p| eval_predicate(configs, p, env))
        }
        PredicateConfig::Not { predicate, .. } => !eval_predicate(configs, predicate, env),
        PredicateConfig::NoPendingRefund { .. } => !env.refund_pending,
    }
}

/// Evaluate an `AcceptanceConfig` tree against the environment
fn eval_config(config: &AcceptanceConfig, env: &EvalEnv) -> bool {
    let configs: BTreeMap<String, &PredicateConfig> = config
        .predicates
        .iter()
        .map(|p| (p.name().to_string(), p))
        .collect();

    if configs.is_empty() {
        return config.default.acceptable();
    }

    // Root: explicit root if specified, otherwise the last predicate in the list
    // (same convention as PredicateBuilder)
    let root_name = match &config.root {
        Some(name) => name.clone(),
        None => config
            .predicates
            .last()
            .map(|p| p.name().to_string())
            .unwrap_or_default(),
    };

    eval_predicate(&configs, &root_name, env)
}

/// The effective policy resolved for a holder
enum EffectivePolicy {
    /// A predicate configuration tree to evaluate
    Config(AcceptanceConfig),
    /// A constant default policy
    Default(DefaultPolicy),
}

impl EffectivePolicy {
    /// Evaluate the policy against the environment
    fn acceptable(&self, env: &EvalEnv) -> bool {
        match self {
            EffectivePolicy::Config(config) => eval_config(config, env),
            EffectivePolicy::Default(policy) => policy.acceptable(),
        }
    }
}

/// Resolve the effective acceptance policy for a holder, using the same precedence
/// as `check_acceptance`: per-recipient stored policy → global predicate
/// configuration → config default. Corrupted or empty stored policies reject by
/// default, mirroring `check_acceptance` behavior.
fn resolve_effective_policy(
    policy_storage: &AcceptancePolicyStorage,
    holder_pubkey: &PubKey,
    global_config: &AcceptanceConfig,
) -> EffectivePolicy {
    let global = || {
        if global_config.predicates.is_empty() {
            EffectivePolicy::Default(global_config.default)
        } else {
            EffectivePolicy::Config(global_config.clone())
        }
    };

    match policy_storage.get_policy(holder_pubkey) {
        Ok(Some(stored)) => match parse_policy_json(&stored.policy_json) {
            Ok(config) => {
                if config.predicates.is_empty() {
                    // Empty per-recipient policy - rejecting by default
                    EffectivePolicy::Default(DefaultPolicy::Reject)
                } else {
                    EffectivePolicy::Config(config)
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Redemption policy check: failed to parse stored policy for {}: {}",
                    hex::encode(holder_pubkey),
                    e
                );
                EffectivePolicy::Default(DefaultPolicy::Reject)
            }
        },
        Ok(None) => global(),
        Err(e) => {
            tracing::warn!(
                "Redemption policy check: failed to read stored policy for {}: {:?}",
                hex::encode(holder_pubkey),
                e
            );
            global()
        }
    }
}

/// Check that a redemption does not newly violate the acceptance policy of any
/// other debt holder of the same reserve.
///
/// - `issuer_pubkey`: reserve owner (note issuer)
/// - `redeemer_pubkey`: recipient requesting the redemption
/// - `redeem_amount`: amount being redeemed (nanoERG)
/// - `reserve_box_value`: current reserve collateral `C` (nanoERG)
/// - `fee`: transaction fee paid from the reserve output (nanoERG)
/// - `refund_pending`: whether the reserve has a pending refund (R7 height > 0)
/// - `notes`: all of the issuer's notes (outstanding debt is derived per note)
/// - `policy_storage`: per-recipient acceptance policy storage
/// - `global_config`: the server's global acceptance configuration
///
/// Returns `Ok(())` when the redemption is allowed, or a `RedemptionPolicyError`
/// describing why it was rejected.
// The parameters mirror the redemption context; grouping them into a struct would
// add indirection without reducing the count at the call sites.
#[allow(clippy::too_many_arguments)]
pub fn check_redemption_policy_compliance(
    issuer_pubkey: &PubKey,
    redeemer_pubkey: &PubKey,
    redeem_amount: u64,
    reserve_box_value: u64,
    fee: u64,
    refund_pending: bool,
    notes: &[IouNote],
    policy_storage: &AcceptancePolicyStorage,
    global_config: &AcceptanceConfig,
) -> Result<(), RedemptionPolicyError> {
    // Total outstanding debt D over all issuer notes (including the redeemer's)
    let total_debt = notes
        .iter()
        .fold(0u64, |acc, n| acc.saturating_add(outstanding(n)));

    // Simulate post-redemption state: C' = C - amount - fee, D' = D - amount
    let collateral_post = reserve_box_value
        .saturating_sub(redeem_amount)
        .saturating_sub(fee);
    let debt_post = total_debt.saturating_sub(redeem_amount);

    // Other holders: recipients with outstanding debt, excluding the redeemer.
    // Value: cumulative total debt (max amount_collected across their notes).
    let mut other_holders: HashMap<PubKey, u64> = HashMap::new();
    for note in notes {
        if note.recipient_pubkey == *redeemer_pubkey || outstanding(note) == 0 {
            continue;
        }
        let entry = other_holders.entry(note.recipient_pubkey).or_insert(0);
        *entry = (*entry).max(note.amount_collected);
    }

    if other_holders.is_empty() {
        // No other holders: nobody else's policy can be affected
        return Ok(());
    }

    let mut first_newly_violated: Option<(PubKey, String)> = None;
    let mut all_pre_violated = true;

    for (holder_pubkey, holder_total_debt) in &other_holders {
        let policy = resolve_effective_policy(policy_storage, holder_pubkey, global_config);

        let pre_env = EvalEnv {
            issuer_pubkey,
            refund_pending,
            collateral: reserve_box_value,
            debt: total_debt,
            holder_total_debt: *holder_total_debt,
        };
        let post_env = EvalEnv {
            issuer_pubkey,
            refund_pending,
            collateral: collateral_post,
            debt: debt_post,
            holder_total_debt: *holder_total_debt,
        };

        let pre_ok = policy.acceptable(&pre_env);
        let post_ok = policy.acceptable(&post_env);

        if pre_ok {
            all_pre_violated = false;
        }

        if pre_ok && !post_ok && first_newly_violated.is_none() {
            let ratio = |c: u64, d: u64| {
                if d == 0 {
                    f64::INFINITY
                } else {
                    c as f64 / d as f64
                }
            };
            first_newly_violated = Some((
                *holder_pubkey,
                format!(
                    "collateralization ratio would drop from {:.4} ({}/{}) to {:.4} ({}/{})",
                    ratio(reserve_box_value, total_debt),
                    reserve_box_value,
                    total_debt,
                    ratio(collateral_post, debt_post),
                    collateral_post,
                    debt_post,
                ),
            ));
        }
    }

    if let Some((holder_pubkey, detail)) = first_newly_violated {
        return Err(RedemptionPolicyError::WouldViolatePolicy {
            holder_pubkey: hex::encode(holder_pubkey),
            detail,
        });
    }

    if all_pre_violated {
        // Distressed reserve: every other holder's policy is already violated, so
        // the check above can never allow anyone. FIFO fallback: only the holder of
        // the issuer's oldest outstanding note may redeem.
        let oldest = notes
            .iter()
            .filter(|n| outstanding(n) > 0)
            .min_by_key(|n| n.timestamp);

        match oldest {
            Some(oldest_note) if oldest_note.recipient_pubkey == *redeemer_pubkey => Ok(()),
            Some(oldest_note) => {
                let requested_timestamp = notes
                    .iter()
                    .filter(|n| n.recipient_pubkey == *redeemer_pubkey && outstanding(n) > 0)
                    .map(|n| n.timestamp)
                    .min()
                    .unwrap_or(0);
                Err(RedemptionPolicyError::NotOldestNote {
                    oldest_timestamp: oldest_note.timestamp,
                    requested_timestamp,
                })
            }
            // No outstanding notes at all - nothing to enforce
            None => Ok(()),
        }
    } else {
        // Mixed case: some holders already violated, none newly violated
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pubkey(n: u8) -> PubKey {
        let mut key = [0u8; 33];
        key[0] = 0x02;
        key[1] = n;
        key
    }

    fn test_note(recipient_n: u8, collected: u64, redeemed: u64, timestamp: u64) -> IouNote {
        IouNote {
            recipient_pubkey: test_pubkey(recipient_n),
            amount_collected: collected,
            amount_redeemed: redeemed,
            timestamp,
            signature: [0u8; 65],
        }
    }

    /// Empty in-memory-ish policy storage (no stored policies) backed by a temp dir
    struct TestStorage {
        storage: AcceptancePolicyStorage,
        _dir: tempfile::TempDir,
    }

    fn test_storage() -> TestStorage {
        let dir = tempfile::tempdir().expect("temp dir");
        let storage = AcceptancePolicyStorage::open(dir.path()).expect("policy storage");
        TestStorage { storage, _dir: dir }
    }

    fn collateral_config(min_ratio: f64) -> AcceptanceConfig {
        AcceptanceConfig {
            default: DefaultPolicy::Reject,
            root: None,
            predicates: vec![PredicateConfig::Collateralization {
                name: "collat".to_string(),
                min_ratio,
            }],
        }
    }

    const ISSUER: u8 = 1;
    const REDEEMER: u8 = 2;
    const HOLDER_A: u8 = 3;
    const HOLDER_B: u8 = 4;

    #[test]
    fn test_well_collateralized_passes() {
        // C=1000, D=500 (ratio 2.0). Redeem 100 (+fee 10):
        // C'=890, D'=400 -> ratio 2.225. Stays above min_ratio 1.0.
        let store = test_storage();
        let notes = vec![
            test_note(REDEEMER, 100, 0, 1000),
            test_note(HOLDER_A, 400, 0, 2000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            1000,
            10,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_newly_violated_holder_rejected() {
        // Paying out A moves the ratio from C/D to (C-A-fee)/(D-A), which worsens
        // only when C < D. With C < D, min_ratio must be below the current ratio
        // for the holder to pass pre-redemption:
        // C=900, D=1000 (0.9), min_ratio 0.85. Redeem 400, fee 0:
        // C'=500, D'=600 -> 0.833 < 0.85: newly violated.
        let store = test_storage();
        let notes = vec![
            test_note(REDEEMER, 400, 0, 1000),
            test_note(HOLDER_A, 600, 0, 2000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            400,
            900,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(0.85),
        );
        match result {
            Err(RedemptionPolicyError::WouldViolatePolicy { holder_pubkey, .. }) => {
                assert_eq!(holder_pubkey, hex::encode(test_pubkey(HOLDER_A)));
            }
            other => panic!("expected WouldViolatePolicy, got {:?}", other),
        }
    }

    #[test]
    fn test_all_violated_non_oldest_rejected() {
        // Deeply undercollateralized: C=100, D=1000, min_ratio 1.0.
        // Both holders already violated pre-redemption. Redeemer holds the newer note.
        let store = test_storage();
        let notes = vec![
            test_note(REDEEMER, 400, 0, 2000), // newer
            test_note(HOLDER_A, 600, 0, 1000), // oldest
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            100,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        match result {
            Err(RedemptionPolicyError::NotOldestNote {
                oldest_timestamp,
                requested_timestamp,
            }) => {
                assert_eq!(oldest_timestamp, 1000);
                assert_eq!(requested_timestamp, 2000);
            }
            other => panic!("expected NotOldestNote, got {:?}", other),
        }
    }

    #[test]
    fn test_all_violated_oldest_allowed() {
        // Same distressed reserve, but the redeemer holds the oldest note.
        let store = test_storage();
        let notes = vec![
            test_note(REDEEMER, 400, 0, 1000), // oldest
            test_note(HOLDER_A, 600, 0, 2000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            100,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_mixed_case_allowed() {
        // HOLDER_A has a stored whitelist policy naming the issuer (no
        // collateralization requirement) -> always satisfied. HOLDER_B falls back
        // to the global collateralization policy and is already violated. None
        // newly violated -> allowed.
        let store = test_storage();
        let issuer_hex = hex::encode(test_pubkey(ISSUER));
        let holder_a_policy = AcceptanceConfig {
            default: DefaultPolicy::Accept,
            root: None,
            predicates: vec![PredicateConfig::Whitelist {
                name: "issuer_ok".to_string(),
                holders: vec![issuer_hex],
                max_debt: None,
            }],
        };
        store
            .storage
            .store_policy(
                &test_pubkey(HOLDER_A),
                &serde_json::to_string(&holder_a_policy).unwrap(),
                "00",
            )
            .unwrap();

        let global = collateral_config(1.0);
        let notes = vec![
            test_note(REDEEMER, 100, 0, 1000),
            test_note(HOLDER_A, 400, 0, 2000),
            test_note(HOLDER_B, 500, 0, 3000),
        ];
        // C=100, D=1000: HOLDER_B (global collat 1.0) already violated.
        // HOLDER_A (whitelist) satisfied. None newly violated -> allowed.
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            50,
            100,
            0,
            false,
            &notes,
            &store.storage,
            &global,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_other_holders_allowed() {
        // Only the redeemer has outstanding debt; other notes fully redeemed.
        let store = test_storage();
        let notes = vec![
            test_note(REDEEMER, 500, 0, 1000),
            test_note(HOLDER_A, 400, 400, 2000), // fully redeemed
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            500,
            100,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_zero_post_debt_edge() {
        // Redeemer is the only outstanding holder and redeems everything: D' = 0.
        let store = test_storage();
        let notes = vec![test_note(REDEEMER, 500, 0, 1000)];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            500,
            400,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_zero_debt_collateralization_leaf() {
        // Collateralization leaf with zero debt is satisfied (no violation possible)
        let env = EvalEnv {
            issuer_pubkey: &test_pubkey(ISSUER),
            refund_pending: false,
            collateral: 0,
            debt: 0,
            holder_total_debt: 0,
        };
        assert!(eval_config(&collateral_config(1.0), &env));
    }

    #[test]
    fn test_stored_policy_takes_precedence_over_global() {
        // Global requires 1.0 (would be violated), but HOLDER_A stored a lenient
        // 0.5 policy which stays satisfied -> allowed.
        let store = test_storage();
        let holder_policy = collateral_config(0.5);
        store
            .storage
            .store_policy(
                &test_pubkey(HOLDER_A),
                &serde_json::to_string(&holder_policy).unwrap(),
                "00",
            )
            .unwrap();

        let notes = vec![
            test_note(REDEEMER, 100, 0, 1000),
            test_note(HOLDER_A, 400, 0, 2000),
        ];
        // C=300, D=500 -> 0.6 pre; redeem 100: C'=200, D'=400 -> 0.5 post.
        // Stored min_ratio 0.5: satisfied both pre and post.
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            300,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_corrupted_stored_policy_counts_as_violated() {
        // Corrupted stored policy -> reject by default, both pre and post, so it
        // counts as "already violated" (FIFO territory), never newly violated.
        let store = test_storage();
        store
            .storage
            .store_policy(&test_pubkey(HOLDER_A), "{not valid json", "00")
            .unwrap();

        let notes = vec![
            test_note(REDEEMER, 100, 0, 2000),
            test_note(HOLDER_A, 400, 0, 1000), // oldest
        ];
        // Well-collateralized reserve; only holder is "already violated" due to
        // corrupted policy -> all violated -> FIFO: redeemer is not oldest.
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            10000,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        assert!(matches!(
            result,
            Err(RedemptionPolicyError::NotOldestNote { .. })
        ));
    }

    #[test]
    fn test_composite_not_semantics_preserved() {
        // Policy: Not(Collateralization 2.0) - satisfied while the ratio is below
        // 2.0. C=1500, D=1000 (1.5): satisfied pre. Redeem 100, fee 0:
        // C'=1400, D'=900 -> 1.555: still satisfied post -> allowed.
        let store = test_storage();
        let config = AcceptanceConfig {
            default: DefaultPolicy::Reject,
            root: None,
            predicates: vec![
                PredicateConfig::Collateralization {
                    name: "collat".to_string(),
                    min_ratio: 2.0,
                },
                PredicateConfig::Not {
                    name: "not_over".to_string(),
                    predicate: "collat".to_string(),
                },
            ],
        };
        let notes = vec![
            test_note(REDEEMER, 100, 0, 1000),
            test_note(HOLDER_A, 900, 0, 2000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            1500,
            0,
            false,
            &notes,
            &store.storage,
            &config,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_display() {
        let err = RedemptionPolicyError::WouldViolatePolicy {
            holder_pubkey: "abcd".to_string(),
            detail: "ratio drop".to_string(),
        };
        assert!(format!("{}", err).contains("abcd"));

        let err = RedemptionPolicyError::NotOldestNote {
            oldest_timestamp: 1000,
            requested_timestamp: 2000,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("1000"));
        assert!(msg.contains("2000"));
    }

    #[test]
    fn test_fifo_timestamp_tie_resolved_by_note_order() {
        // Distressed reserve where the redeemer's and the other holder's notes
        // share the same minimum timestamp. `min_by_key` returns the FIRST
        // element among equal minima, so the queue head depends on the order
        // notes appear in the issuer's note list.
        let store = test_storage();
        let config = collateral_config(1.0);

        // Redeemer's note first in the list: redeemer is the queue head -> allowed.
        let notes = vec![
            test_note(REDEEMER, 400, 0, 1000),
            test_note(HOLDER_A, 600, 0, 1000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            100,
            0,
            false,
            &notes,
            &store.storage,
            &config,
        );
        assert!(result.is_ok());

        // Holder's note first: holder is the queue head -> redeemer rejected,
        // with oldest_timestamp == requested_timestamp (a tie).
        let notes = vec![
            test_note(HOLDER_A, 600, 0, 1000),
            test_note(REDEEMER, 400, 0, 1000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            100,
            0,
            false,
            &notes,
            &store.storage,
            &config,
        );
        match result {
            Err(RedemptionPolicyError::NotOldestNote {
                oldest_timestamp,
                requested_timestamp,
            }) => {
                assert_eq!(oldest_timestamp, 1000);
                assert_eq!(requested_timestamp, 1000);
            }
            other => panic!("expected NotOldestNote, got {:?}", other),
        }
    }

    #[test]
    fn test_fully_redeemed_oldest_note_skipped_in_fifo_queue() {
        // Distressed reserve. HOLDER_A's note is the oldest by timestamp but
        // fully redeemed (outstanding 0), so the redeemer's note becomes the
        // queue head -> allowed.
        let store = test_storage();
        let notes = vec![
            test_note(REDEEMER, 400, 0, 2000),
            test_note(HOLDER_A, 500, 500, 1000), // oldest, fully redeemed
            test_note(HOLDER_B, 100, 0, 3000),
        ];
        // D = 400 + 100 = 500 outstanding; C=100, min_ratio 1.0: HOLDER_B already
        // violated, HOLDER_A excluded -> all other holders violated -> FIFO.
        // Oldest outstanding note is the redeemer's (ts 2000).
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            100,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_min_ratio_boundary_equal_passes() {
        // The collateralization comparison is inclusive (`ratio >= min_ratio`):
        // a post-redemption ratio exactly equal to min_ratio is acceptable.
        // C=300, D=500 (0.6 pre); redeem 100 -> C'=200, D'=400 -> exactly 0.5.
        let store = test_storage();
        let notes = vec![
            test_note(REDEEMER, 100, 0, 1000),
            test_note(HOLDER_A, 400, 0, 2000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            300,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(0.5),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_min_ratio_boundary_just_below_fails() {
        // Just below the boundary: C=299, D=500 (0.598 pre, >= 0.5 ok);
        // redeem 100 -> C'=199, D'=400 -> 0.4975 < 0.5: newly violated.
        let store = test_storage();
        let notes = vec![
            test_note(REDEEMER, 100, 0, 1000),
            test_note(HOLDER_A, 400, 0, 2000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            299,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(0.5),
        );
        assert!(matches!(
            result,
            Err(RedemptionPolicyError::WouldViolatePolicy { .. })
        ));
    }

    #[test]
    fn test_compound_allof_failing_non_collat_leaf_counts_as_already_violated() {
        // Policy: AllOf[Whitelist (issuer NOT listed), Collateralization 0.85].
        // The whitelist leaf fails both pre and post, so the holder is *already*
        // violated even though the collateralization leaf alone would be *newly*
        // violated by the ratio drop (0.9 -> 0.833). The redemption must hit the
        // FIFO fallback, never WouldViolatePolicy.
        let store = test_storage();
        let config = AcceptanceConfig {
            default: DefaultPolicy::Reject,
            root: None,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "wl".to_string(),
                    holders: vec![hex::encode(test_pubkey(99))], // issuer not listed
                    max_debt: None,
                },
                PredicateConfig::Collateralization {
                    name: "collat".to_string(),
                    min_ratio: 0.85,
                },
                PredicateConfig::AllOf {
                    name: "all".to_string(),
                    predicates: vec!["wl".to_string(), "collat".to_string()],
                },
            ],
        };
        // C=900, D=1000, redeem 400: collat leaf alone goes 0.9 -> 500/600 = 0.833.
        let amount = 400;
        let collateral = 900;

        // Non-oldest redeemer -> NotOldestNote (NOT WouldViolatePolicy).
        let notes = vec![
            test_note(REDEEMER, 400, 0, 2000), // newer
            test_note(HOLDER_A, 600, 0, 1000), // oldest
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            amount,
            collateral,
            0,
            false,
            &notes,
            &store.storage,
            &config,
        );
        assert!(
            matches!(result, Err(RedemptionPolicyError::NotOldestNote { .. })),
            "expected NotOldestNote, got {:?}",
            result
        );

        // Oldest redeemer -> allowed (FIFO fallback), again not WouldViolatePolicy.
        let notes = vec![
            test_note(REDEEMER, 400, 0, 1000), // oldest
            test_note(HOLDER_A, 600, 0, 2000),
        ];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            amount,
            collateral,
            0,
            false,
            &notes,
            &store.storage,
            &config,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_pending_refund_leaf_with_pending_refund() {
        // NoPendingRefund fails while the reserve has a pending refund, both pre
        // and post (it is unaffected by the redemption), so the holder counts as
        // already violated -> FIFO fallback even on a well-collateralized reserve.
        let store = test_storage();
        let config = AcceptanceConfig {
            default: DefaultPolicy::Reject,
            root: None,
            predicates: vec![PredicateConfig::NoPendingRefund {
                name: "no_refund".to_string(),
            }],
        };
        let notes = vec![
            test_note(REDEEMER, 400, 0, 2000), // newer
            test_note(HOLDER_A, 600, 0, 1000), // oldest
        ];

        // Refund pending: holder already violated -> non-oldest redeemer rejected.
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            10000,
            0,
            true,
            &notes,
            &store.storage,
            &config,
        );
        assert!(matches!(
            result,
            Err(RedemptionPolicyError::NotOldestNote { .. })
        ));

        // No refund pending: policy satisfied -> allowed.
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            10000,
            0,
            false,
            &notes,
            &store.storage,
            &config,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_stored_stricter_policy_overrides_lenient_global() {
        // Global min_ratio 0.5 would allow the redemption (post ratio exactly
        // 0.5), but HOLDER_A stored a stricter 0.55 policy: 0.6 pre ok,
        // 0.5 post fail -> newly violated.
        let store = test_storage();
        let holder_policy = collateral_config(0.55);
        store
            .storage
            .store_policy(
                &test_pubkey(HOLDER_A),
                &serde_json::to_string(&holder_policy).unwrap(),
                "00",
            )
            .unwrap();

        let notes = vec![
            test_note(REDEEMER, 100, 0, 1000),
            test_note(HOLDER_A, 400, 0, 2000),
        ];
        // C=300, D=500 -> 0.6 pre; redeem 100: C'=200, D'=400 -> 0.5 post.
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            300,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(0.5),
        );
        match result {
            Err(RedemptionPolicyError::WouldViolatePolicy { holder_pubkey, .. }) => {
                assert_eq!(holder_pubkey, hex::encode(test_pubkey(HOLDER_A)));
            }
            other => panic!("expected WouldViolatePolicy, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_stored_policy_counts_as_violated() {
        // A stored policy that parses but contains no predicates rejects by
        // default (mirroring check_acceptance), counting as already violated.
        let store = test_storage();
        let empty_policy = AcceptanceConfig {
            default: DefaultPolicy::Accept,
            root: None,
            predicates: vec![],
        };
        store
            .storage
            .store_policy(
                &test_pubkey(HOLDER_A),
                &serde_json::to_string(&empty_policy).unwrap(),
                "00",
            )
            .unwrap();

        let notes = vec![
            test_note(REDEEMER, 100, 0, 2000),
            test_note(HOLDER_A, 400, 0, 1000), // oldest
        ];
        // Well-collateralized reserve; the only other holder is already violated
        // due to the empty stored policy -> FIFO: redeemer is not oldest.
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            10000,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        assert!(matches!(
            result,
            Err(RedemptionPolicyError::NotOldestNote { .. })
        ));
    }

    #[test]
    fn test_redeemer_without_outstanding_note_rejected_in_distressed_reserve() {
        // The redeemer has no outstanding note in the issuer's list. In a
        // distressed reserve the oldest outstanding note belongs to someone
        // else, and the requested timestamp is reported as 0.
        let store = test_storage();
        let notes = vec![test_note(HOLDER_A, 600, 0, 1000)];
        let result = check_redemption_policy_compliance(
            &test_pubkey(ISSUER),
            &test_pubkey(REDEEMER),
            100,
            100,
            0,
            false,
            &notes,
            &store.storage,
            &collateral_config(1.0),
        );
        match result {
            Err(RedemptionPolicyError::NotOldestNote {
                oldest_timestamp,
                requested_timestamp,
            }) => {
                assert_eq!(oldest_timestamp, 1000);
                assert_eq!(requested_timestamp, 0);
            }
            other => panic!("expected NotOldestNote, got {:?}", other),
        }
    }
}
