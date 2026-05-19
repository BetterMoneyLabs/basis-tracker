//! Acceptance predicate system for Basis tracker
//!
//! Provides configurable policies for determining whether IOU notes are acceptable
//! as payment. Supports whitelist, blacklist, collateralization checks, and composite
//! predicates with TOML-based configuration.

pub mod builder;
pub mod config;

use basis_store::PubKey;
use std::collections::HashSet;

/// Context passed to predicate evaluation
#[derive(Clone)]
pub struct PredicateContext {
    /// Public key of the note issuer (owner)
    pub issuer_pubkey: PubKey,
    /// Public key of the note recipient (creditor)
    pub recipient_pubkey: PubKey,
    /// Total cumulative debt amount in the note
    pub total_debt: u64,
    /// Optional cloned reserve tracker for collateralization checks
    pub reserve_tracker: Option<basis_store::ReserveTracker>,
}

impl std::fmt::Debug for PredicateContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PredicateContext")
            .field("issuer_pubkey", &hex::encode(&self.issuer_pubkey))
            .field("recipient_pubkey", &hex::encode(&self.recipient_pubkey))
            .field("total_debt", &self.total_debt)
            .field("reserve_tracker", &self.reserve_tracker.is_some())
            .finish()
    }
}

/// Trait for note acceptance predicates
pub trait NotePredicate: Send + Sync + std::fmt::Debug {
    /// Evaluate whether a note is acceptable given the context
    fn acceptable(&self, ctx: &PredicateContext) -> bool;
    
    /// Get the predicate name
    fn name(&self) -> &str;
}

/// Whitelist predicate - accepts if issuer is in whitelist
#[derive(Debug, Clone)]
pub struct WhitelistPredicate {
    name: String,
    holders: HashSet<PubKey>,
    max_debt: Option<u64>,
}

impl WhitelistPredicate {
    /// Create a new whitelist predicate
    pub fn new(name: impl Into<String>, holders: HashSet<PubKey>) -> Self {
        Self {
            name: name.into(),
            holders,
            max_debt: None,
        }
    }
    
    /// Create a new whitelist predicate with debt limit
    pub fn new_with_limit(name: impl Into<String>, holders: HashSet<PubKey>, max_debt: u64) -> Self {
        Self {
            name: name.into(),
            holders,
            max_debt: Some(max_debt),
        }
    }
}

impl NotePredicate for WhitelistPredicate {
    fn acceptable(&self, ctx: &PredicateContext) -> bool {
        if !self.holders.contains(&ctx.issuer_pubkey) {
            return false;
        }
        
        if let Some(max) = self.max_debt {
            if ctx.total_debt > max {
                return false;
            }
        }
        
        true
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Blacklist predicate - rejects if issuer is in blacklist
#[derive(Debug, Clone)]
pub struct BlacklistPredicate {
    name: String,
    holders: HashSet<PubKey>,
}

impl BlacklistPredicate {
    /// Create a new blacklist predicate
    pub fn new(name: impl Into<String>, holders: HashSet<PubKey>) -> Self {
        Self {
            name: name.into(),
            holders,
        }
    }
}

impl NotePredicate for BlacklistPredicate {
    fn acceptable(&self, ctx: &PredicateContext) -> bool {
        !self.holders.contains(&ctx.issuer_pubkey)
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Collateralization predicate - accepts if reserve meets minimum ratio
#[derive(Debug, Clone)]
pub struct CollateralizationPredicate {
    name: String,
    min_ratio: f64,
}

impl CollateralizationPredicate {
    /// Create a new collateralization predicate
    pub fn new(name: impl Into<String>, min_ratio: f64) -> Self {
        Self {
            name: name.into(),
            min_ratio,
        }
    }
}

impl NotePredicate for CollateralizationPredicate {
    fn acceptable(&self, ctx: &PredicateContext) -> bool {
        let tracker = match &ctx.reserve_tracker {
            Some(t) => t,
            None => return false,
        };
        
        let reserve = match tracker.get_reserve_by_owner(&hex::encode(&ctx.issuer_pubkey)) {
            Ok(r) => r,
            Err(_) => return false,
        };
        
        let assets = reserve.base_info.collateral_amount;
        let liabilities = reserve.total_debt;
        
        if liabilities == 0 {
            // No debt means fully collateralized (or no reserve needed)
            return true;
        }
        
        let ratio = assets as f64 / liabilities as f64;
        ratio >= self.min_ratio
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// All-of (AND) composite predicate
#[derive(Debug)]
pub struct AllOfPredicate {
    name: String,
    predicates: Vec<Box<dyn NotePredicate>>,
}

impl AllOfPredicate {
    /// Create a new all-of predicate
    pub fn new(name: impl Into<String>, predicates: Vec<Box<dyn NotePredicate>>) -> Self {
        Self {
            name: name.into(),
            predicates,
        }
    }
}

impl NotePredicate for AllOfPredicate {
    fn acceptable(&self, ctx: &PredicateContext) -> bool {
        if self.predicates.is_empty() {
            return true; // Empty AND is true
        }
        self.predicates.iter().all(|p| p.acceptable(ctx))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Any-of (OR) composite predicate
#[derive(Debug)]
pub struct AnyOfPredicate {
    name: String,
    predicates: Vec<Box<dyn NotePredicate>>,
}

impl AnyOfPredicate {
    /// Create a new any-of predicate
    pub fn new(name: impl Into<String>, predicates: Vec<Box<dyn NotePredicate>>) -> Self {
        Self {
            name: name.into(),
            predicates,
        }
    }
}

impl NotePredicate for AnyOfPredicate {
    fn acceptable(&self, ctx: &PredicateContext) -> bool {
        if self.predicates.is_empty() {
            return false; // Empty OR is false
        }
        self.predicates.iter().any(|p| p.acceptable(ctx))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Not (negation) predicate
#[derive(Debug)]
pub struct NotPredicate {
    name: String,
    predicate: Box<dyn NotePredicate>,
}

impl NotPredicate {
    /// Create a new not predicate
    pub fn new(name: impl Into<String>, predicate: Box<dyn NotePredicate>) -> Self {
        Self {
            name: name.into(),
            predicate,
        }
    }
}

impl NotePredicate for NotPredicate {
    fn acceptable(&self, ctx: &PredicateContext) -> bool {
        !self.predicate.acceptable(ctx)
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Default policy when no predicate matches
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultPolicy {
    Accept,
    Reject,
}

impl DefaultPolicy {
    /// Evaluate the default policy
    pub fn acceptable(&self) -> bool {
        match self {
            DefaultPolicy::Accept => true,
            DefaultPolicy::Reject => false,
        }
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
    
    fn test_context(issuer_n: u8, total_debt: u64) -> PredicateContext {
        PredicateContext {
            issuer_pubkey: test_pubkey(issuer_n),
            recipient_pubkey: test_pubkey(255),
            total_debt,
            reserve_tracker: None,
        }
    }
    
    #[test]
    fn test_whitelist_accepts_member() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        holders.insert(test_pubkey(2));
        
        let pred = WhitelistPredicate::new("test", holders);
        let ctx = test_context(1, 100);
        
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_whitelist_rejects_non_member() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        holders.insert(test_pubkey(2));
        
        let pred = WhitelistPredicate::new("test", holders);
        let ctx = test_context(3, 100);
        
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_whitelist_with_max_debt() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = WhitelistPredicate::new_with_limit("test", holders, 500);
        
        // Under limit
        let ctx1 = test_context(1, 400);
        assert!(pred.acceptable(&ctx1));
        
        // At limit
        let ctx2 = test_context(1, 500);
        assert!(pred.acceptable(&ctx2));
        
        // Over limit
        let ctx3 = test_context(1, 501);
        assert!(!pred.acceptable(&ctx3));
    }
    
    #[test]
    fn test_whitelist_exceeds_max_debt() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = WhitelistPredicate::new_with_limit("test", holders, 100);
        let ctx = test_context(1, 200);
        
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_whitelist_no_limit_allows_any_amount() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = WhitelistPredicate::new("test", holders);
        let ctx = test_context(1, u64::MAX);
        
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_blacklist_rejects_member() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        holders.insert(test_pubkey(2));
        
        let pred = BlacklistPredicate::new("test", holders);
        let ctx = test_context(1, 100);
        
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_blacklist_accepts_non_member() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        holders.insert(test_pubkey(2));
        
        let pred = BlacklistPredicate::new("test", holders);
        let ctx = test_context(3, 100);
        
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_blacklist_empty_allows_all() {
        let holders = HashSet::new();
        
        let pred = BlacklistPredicate::new("test", holders);
        let ctx = test_context(1, 100);
        
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_collateralization_no_tracker_rejects() {
        let pred = CollateralizationPredicate::new("test", 1.0);
        let ctx = test_context(1, 100);
        
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_collateralization_fully_collateralized() {
        let pred = CollateralizationPredicate::new("test", 1.0);
        let ctx = test_context(1, 100);
        
        // We can't easily test with tracker here, so we'll test in integration
        // For unit test, verify it returns false without tracker
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_allof_empty_returns_true() {
        let pred = AllOfPredicate::new("test", vec![]);
        let ctx = test_context(1, 100);
        
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_allof_all_true() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = AllOfPredicate::new("test", vec![
            Box::new(WhitelistPredicate::new("w", holders.clone())),
            Box::new(WhitelistPredicate::new("w2", holders.clone())),
        ]);
        let ctx = test_context(1, 100);
        
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_allof_one_false() {
        let mut holders1 = HashSet::new();
        holders1.insert(test_pubkey(1));
        let mut holders2 = HashSet::new();
        holders2.insert(test_pubkey(2));
        
        let pred = AllOfPredicate::new("test", vec![
            Box::new(WhitelistPredicate::new("w1", holders1)),
            Box::new(WhitelistPredicate::new("w2", holders2)),
        ]);
        let ctx = test_context(1, 100);
        
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_anyof_empty_returns_false() {
        let pred = AnyOfPredicate::new("test", vec![]);
        let ctx = test_context(1, 100);
        
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_anyof_one_true() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = AnyOfPredicate::new("test", vec![
            Box::new(WhitelistPredicate::new("w1", HashSet::new())),
            Box::new(WhitelistPredicate::new("w2", holders)),
        ]);
        let ctx = test_context(1, 100);
        
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_anyof_all_false() {
        let pred = AnyOfPredicate::new("test", vec![
            Box::new(WhitelistPredicate::new("w1", HashSet::new())),
            Box::new(WhitelistPredicate::new("w2", HashSet::new())),
        ]);
        let ctx = test_context(1, 100);
        
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_not_inverts() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = NotPredicate::new("test", Box::new(
            BlacklistPredicate::new("black", holders)
        ));
        let ctx = test_context(1, 100);
        
        // Blacklist rejects pubkey(1), NOT inverts it to accept
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_not_double_inversion() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = NotPredicate::new("test", Box::new(
            NotPredicate::new("inner", Box::new(
                WhitelistPredicate::new("white", holders)
            ))
        ));
        let ctx = test_context(1, 100);
        
        // Double negation: NOT(NOT(whitelist)) == whitelist
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_default_policy_accept() {
        assert!(DefaultPolicy::Accept.acceptable());
    }
    
    #[test]
    fn test_default_policy_reject() {
        assert!(!DefaultPolicy::Reject.acceptable());
    }
    
    #[test]
    fn test_predicate_names() {
        let pred = WhitelistPredicate::new("my_pred", HashSet::new());
        assert_eq!(pred.name(), "my_pred");
        
        let pred = BlacklistPredicate::new("block", HashSet::new());
        assert_eq!(pred.name(), "block");
        
        let pred = CollateralizationPredicate::new("collat", 1.0);
        assert_eq!(pred.name(), "collat");
    }
    
    #[test]
    fn test_complex_composite_letsscenario() {
        // LETS scenario: members whitelisted with no debt limit
        let mut lets_members = HashSet::new();
        lets_members.insert(test_pubkey(1));
        lets_members.insert(test_pubkey(2));
        lets_members.insert(test_pubkey(3));
        
        // Municipality endorsement
        let mut municipality = HashSet::new();
        municipality.insert(test_pubkey(9));
        
        // LETS policy: any member OR municipality
        let policy = AnyOfPredicate::new("lets", vec![
            Box::new(WhitelistPredicate::new("members", lets_members)),
            Box::new(WhitelistPredicate::new("municipality", municipality)),
        ]);
        
        // Member should be accepted
        let ctx1 = test_context(1, u64::MAX);
        assert!(policy.acceptable(&ctx1));
        
        // Municipality should be accepted
        let ctx2 = test_context(9, u64::MAX);
        assert!(policy.acceptable(&ctx2));
        
        // Non-member should be rejected
        let ctx3 = test_context(4, 100);
        assert!(!policy.acceptable(&ctx3));
    }
    
    #[test]
    fn test_context_clone() {
        let ctx = PredicateContext {
            issuer_pubkey: test_pubkey(1),
            recipient_pubkey: test_pubkey(2),
            total_debt: 100,
            reserve_tracker: None,
        };
        let cloned = ctx.clone();
        assert_eq!(ctx.issuer_pubkey, cloned.issuer_pubkey);
        assert_eq!(ctx.total_debt, cloned.total_debt);
    }
    
    #[test]
    fn test_whitelist_clone() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = WhitelistPredicate::new_with_limit("test", holders, 500);
        let cloned = pred.clone();
        
        let ctx = test_context(1, 400);
        assert!(cloned.acceptable(&ctx));
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_blacklist_clone() {
        let mut holders = HashSet::new();
        holders.insert(test_pubkey(1));
        
        let pred = BlacklistPredicate::new("test", holders);
        let cloned = pred.clone();
        
        let ctx = test_context(1, 100);
        assert!(!cloned.acceptable(&ctx));
        assert!(!pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_collateralization_clone() {
        let pred = CollateralizationPredicate::new("test", 1.5);
        let cloned = pred.clone();
        
        assert_eq!(pred.min_ratio, cloned.min_ratio);
        assert_eq!(pred.name(), cloned.name());
    }
}
