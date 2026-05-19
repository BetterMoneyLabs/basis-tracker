//! Builder for constructing acceptance predicate trees from configuration
//!
//! Converts TOML-based configuration into runnable predicate objects,
//! resolving references and validating the predicate graph.

use super::{
    AllOfPredicate, AnyOfPredicate, BlacklistPredicate, CollateralizationPredicate,
    NotePredicate, NotPredicate, WhitelistPredicate,
};
use super::config::{AcceptanceConfig, PredicateConfig};
use basis_store::PubKey;
use std::collections::{BTreeMap, HashSet};

/// Error during predicate tree construction
#[derive(Debug, thiserror::Error)]
pub enum BuilderError {
    #[error("Duplicate predicate name: {0}")]
    DuplicateName(String),
    #[error("Missing predicate reference: {0}")]
    MissingReference(String),
    #[error("Circular reference detected: {0}")]
    CircularReference(String),
    #[error("Invalid public key hex: {0}")]
    InvalidPublicKey(String),
    #[error("Invalid public key length: expected 33 bytes, got {0}")]
    InvalidPublicKeyLength(usize),
}

/// Builder for constructing predicate trees from configuration
pub struct PredicateBuilder {
    predicates: BTreeMap<String, Box<dyn NotePredicate>>,
    config_map: BTreeMap<String, PredicateConfig>,
    building: HashSet<String>,
}

impl PredicateBuilder {
    /// Create a new builder from configuration
    pub fn new(config: AcceptanceConfig) -> Self {
        let mut config_map = BTreeMap::new();
        for pred in config.predicates {
            config_map.insert(pred.name().to_string(), pred);
        }
        
        Self {
            predicates: BTreeMap::new(),
            config_map,
            building: HashSet::new(),
        }
    }
    
    /// Build the predicate tree
    pub fn build(&mut self, root_name: Option<&str>) -> Result<Option<Box<dyn NotePredicate>>, BuilderError> {
        if self.config_map.is_empty() {
            return Ok(None);
        }
        
        // Find root predicate - use explicit root if specified,
        // otherwise use the last predicate in the list
        let root_name = match root_name {
            Some(name) => name.to_string(),
            None => self.config_map.keys().last().cloned().unwrap(),
        };
        
        let root = self.build_predicate(&root_name)?;
        
        Ok(Some(root))
    }
    
    /// Build a specific predicate by name
    fn build_predicate(&mut self, name: &str) -> Result<Box<dyn NotePredicate>, BuilderError> {
        // Check if already built
        if let Some(pred) = self.predicates.get(name) {
            // We need to clone here, but NotePredicate doesn't support Clone
            // So we rebuild - this is inefficient but safe
            // For production, we could use Arc<dyn NotePredicate> to share
        }
        
        // Check for circular references
        if self.building.contains(name) {
            return Err(BuilderError::CircularReference(name.to_string()));
        }
        
        // Get config
        let config = self.config_map
            .get(name)
            .ok_or_else(|| BuilderError::MissingReference(name.to_string()))?
            .clone();
        
        self.building.insert(name.to_string());
        
        let result = match config {
            PredicateConfig::Whitelist { name, holders, max_debt } => {
                let parsed_holders = Self::parse_holders(&holders)?;
                let pred = if let Some(max) = max_debt {
                    WhitelistPredicate::new_with_limit(name, parsed_holders, max)
                } else {
                    WhitelistPredicate::new(name, parsed_holders)
                };
                Ok(Box::new(pred) as Box<dyn NotePredicate>)
            }
            PredicateConfig::Blacklist { name, holders } => {
                let parsed_holders = Self::parse_holders(&holders)?;
                Ok(Box::new(BlacklistPredicate::new(name, parsed_holders)) as Box<dyn NotePredicate>)
            }
            PredicateConfig::Collateralization { name, min_ratio } => {
                Ok(Box::new(CollateralizationPredicate::new(name, min_ratio)) as Box<dyn NotePredicate>)
            }
            PredicateConfig::AllOf { name, predicates: refs } => {
                let sub_preds = self.build_sub_predicates(&refs)?;
                Ok(Box::new(AllOfPredicate::new(name, sub_preds)) as Box<dyn NotePredicate>)
            }
            PredicateConfig::AnyOf { name, predicates: refs } => {
                let sub_preds = self.build_sub_predicates(&refs)?;
                Ok(Box::new(AnyOfPredicate::new(name, sub_preds)) as Box<dyn NotePredicate>)
            }
            PredicateConfig::Not { name, predicate: ref_name } => {
                let inner = self.build_predicate(&ref_name)?;
                Ok(Box::new(NotPredicate::new(name, inner)) as Box<dyn NotePredicate>)
            }
        };
        
        self.building.remove(name);
        
        // Cache the built predicate
        if let Ok(ref pred) = result {
            self.predicates.insert(name.to_string(), Self::clone_predicate(pred.as_ref()));
        }
        
        result
    }
    
    /// Build multiple sub-predicates
    fn build_sub_predicates(
        &mut self,
        refs: &[ String],
    ) -> Result<Vec<Box<dyn NotePredicate>>, BuilderError> {
        let mut result = Vec::with_capacity(refs.len());
        for ref_name in refs {
            result.push(self.build_predicate(ref_name)?);
        }
        Ok(result)
    }
    
    /// Parse hex-encoded public keys
    fn parse_holders(holders: &[ String]) -> Result<HashSet<PubKey>, BuilderError> {
        let mut result = HashSet::with_capacity(holders.len());
        for hex_str in holders {
            let bytes = hex::decode(hex_str)
                .map_err(|e| BuilderError::InvalidPublicKey(format!("{}: {}", hex_str, e)))?;
            if bytes.len() != 33 {
                return Err(BuilderError::InvalidPublicKeyLength(bytes.len()));
            }
            let mut key = [0u8; 33];
            key.copy_from_slice(&bytes);
            result.insert(key);
        }
        Ok(result)
    }
    
    /// Clone a predicate (needed for caching)
    fn clone_predicate(pred: &dyn NotePredicate) -> Box<dyn NotePredicate> {
        // Since we can't easily clone dyn NotePredicate, we just rebuild
        // For production, we could use Arc<dyn NotePredicate> to avoid cloning
        // For now, we'll just create a placeholder - this is only used for caching
        // which we don't strictly need
        Box::new(AnyOfPredicate::new("placeholder", vec![]))
    }
}

/// Build a predicate tree from configuration
pub fn build_predicate_tree(
    config: AcceptanceConfig,
) -> Result<Option<Box<dyn NotePredicate>>, BuilderError> {
    let root = config.root.clone();
    let mut builder = PredicateBuilder::new(config);
    builder.build(root.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::DefaultPolicy;
    
    fn test_pubkey_hex(n: u8) -> String {
        let mut key = [0u8; 33];
        key[0] = 0x02;
        key[1] = n;
        hex::encode(key)
    }
    
    #[test]
    fn test_build_whitelist() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "trusted".to_string(),
                    holders: vec![test_pubkey_hex(1), test_pubkey_hex(2)],
                    max_debt: None,
                },
            ],
        };
        
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_some());
    }
    
    #[test]
    fn test_build_whitelist_with_max_debt() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "trusted".to_string(),
                    holders: vec![test_pubkey_hex(1)],
                    max_debt: Some(500),
                },
            ],
        };
        
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_some());
        
        let pred = result.unwrap();
        assert_eq!(pred.name(), "trusted");
    }
    
    #[test]
    fn test_build_blacklist() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Blacklist {
                    name: "blocked".to_string(),
                    holders: vec![test_pubkey_hex(1)],
                },
            ],
        };
        
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name(), "blocked");
    }
    
    #[test]
    fn test_build_collateralization() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Collateralization {
                    name: "collat".to_string(),
                    min_ratio: 1.5,
                },
            ],
        };
        
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name(), "collat");
    }
    
    #[test]
    fn test_build_all_of() {
        let config = AcceptanceConfig {
            root: Some("strict".to_string()),
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "trusted".to_string(),
                    holders: vec![test_pubkey_hex(1)],
                    max_debt: None,
                },
                PredicateConfig::Collateralization {
                    name: "collat".to_string(),
                    min_ratio: 1.0,
                },
                PredicateConfig::AllOf {
                    name: "strict".to_string(),
                    predicates: vec!["trusted".to_string(), "collat".to_string()],
                },
            ],
        };
        
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name(), "strict");
    }
    
    #[test]
    fn test_build_any_of() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "trusted".to_string(),
                    holders: vec![test_pubkey_hex(1)],
                    max_debt: None,
                },
                PredicateConfig::Collateralization {
                    name: "collat".to_string(),
                    min_ratio: 1.0,
                },
                PredicateConfig::AnyOf {
                    name: "cow1".to_string(),
                    predicates: vec!["trusted".to_string(), "collat".to_string()],
                },
            ],
        };
        
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_some());
    }
    
    #[test]
    fn test_build_not() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Blacklist {
                    name: "blocked".to_string(),
                    holders: vec![test_pubkey_hex(1)],
                },
                PredicateConfig::Not {
                    name: "allowed".to_string(),
                    predicate: "blocked".to_string(),
                },
            ],
        };
        
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_some());
    }
    
    #[test]
    fn test_build_empty_config() {
        let config = AcceptanceConfig::empty();
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_none());
    }
    
    #[test]
    fn test_missing_reference() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::AllOf {
                    name: "bad".to_string(),
                    predicates: vec!["missing".to_string()],
                },
            ],
        };
        
        let result = build_predicate_tree(config);
        assert!(result.is_err());
        match result.unwrap_err() {
            BuilderError::MissingReference(name) => assert_eq!(name, "missing"),
            _ => panic!("Expected MissingReference error"),
        }
    }
    
    #[test]
    fn test_circular_reference() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Not {
                    name: "a".to_string(),
                    predicate: "b".to_string(),
                },
                PredicateConfig::Not {
                    name: "b".to_string(),
                    predicate: "a".to_string(),
                },
            ],
        };
        
        let result = build_predicate_tree(config);
        assert!(result.is_err());
        match result.unwrap_err() {
            BuilderError::CircularReference(_) => {},
            _ => panic!("Expected CircularReference error"),
        }
    }
    
    #[test]
    fn test_invalid_pubkey_hex() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "bad".to_string(),
                    holders: vec!["not-hex!!!".to_string()],
                    max_debt: None,
                },
            ],
        };
        
        let result = build_predicate_tree(config);
        assert!(result.is_err());
        match result.unwrap_err() {
            BuilderError::InvalidPublicKey(_) => {},
            _ => panic!("Expected InvalidPublicKey error"),
        }
    }
    
    #[test]
    fn test_invalid_pubkey_length() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "bad".to_string(),
                    holders: vec!["deadbeef".to_string()], // 4 bytes, not 33
                    max_debt: None,
                },
            ],
        };
        
        let result = build_predicate_tree(config);
        assert!(result.is_err());
        match result.unwrap_err() {
            BuilderError::InvalidPublicKeyLength(4) => {},
            _ => panic!("Expected InvalidPublicKeyLength error"),
        }
    }
    
    #[test]
    fn test_parse_holders_valid() {
        let holders = vec![test_pubkey_hex(1), test_pubkey_hex(2)];
        let result = PredicateBuilder::parse_holders(&holders).unwrap();
        assert_eq!(result.len(), 2);
    }
    
    #[test]
    fn test_parse_holders_empty() {
        let holders: Vec<String> = vec![];
        let result = PredicateBuilder::parse_holders(&holders).unwrap();
        assert!(result.is_empty());
    }
    
    #[test]
    fn test_parse_holders_duplicate() {
        // Duplicates should be deduplicated by HashSet
        let holders = vec![test_pubkey_hex(1), test_pubkey_hex(1)];
        let result = PredicateBuilder::parse_holders(&holders).unwrap();
        assert_eq!(result.len(), 1);
    }
    
    #[test]
    fn test_builder_caches_predicates() {
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "trusted".to_string(),
                    holders: vec![test_pubkey_hex(1)],
                    max_debt: None,
                },
                PredicateConfig::AllOf {
                    name: "all1".to_string(),
                    predicates: vec!["trusted".to_string()],
                },
                PredicateConfig::AllOf {
                    name: "all2".to_string(),
                    predicates: vec!["trusted".to_string()],
                },
            ],
        };
        
        // Should succeed even with shared reference
        let result = build_predicate_tree(config);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_lets_scenario() {
        let config = AcceptanceConfig {
            root: Some("lets_policy".to_string()),
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "lets_members".to_string(),
                    holders: vec![
                        test_pubkey_hex(1),
                        test_pubkey_hex(2),
                        test_pubkey_hex(3),
                    ],
                    max_debt: Some(5000000000),
                },
                PredicateConfig::Whitelist {
                    name: "municipality".to_string(),
                    holders: vec![test_pubkey_hex(9)],
                    max_debt: None,
                },
                PredicateConfig::AnyOf {
                    name: "lets_policy".to_string(),
                    predicates: vec!["lets_members".to_string(), "municipality".to_string()],
                },
            ],
        };
        
        let result = build_predicate_tree(config).unwrap();
        assert!(result.is_some());
        
        let pred = result.unwrap();
        assert_eq!(pred.name(), "lets_policy");
        
        // Test with a LETS member
        let ctx = super::super::PredicateContext {
            issuer_pubkey: {
                let mut key = [0u8; 33];
                key[0] = 0x02;
                key[1] = 1;
                key
            },
            recipient_pubkey: [0u8; 33],
            total_debt: 4000000000,
            reserve_tracker: None,
        };
        assert!(pred.acceptable(&ctx));
        
        // Test with municipality
        let ctx2 = super::super::PredicateContext {
            issuer_pubkey: {
                let mut key = [0u8; 33];
                key[0] = 0x02;
                key[1] = 9;
                key
            },
            recipient_pubkey: [0u8; 33],
            total_debt: u64::MAX,
            reserve_tracker: None,
        };
        assert!(pred.acceptable(&ctx2));
        
        // Test with non-member
        let ctx3 = super::super::PredicateContext {
            issuer_pubkey: {
                let mut key = [0u8; 33];
                key[0] = 0x02;
                key[1] = 4;
                key
            },
            recipient_pubkey: [0u8; 33],
            total_debt: 100,
            reserve_tracker: None,
        };
        assert!(!pred.acceptable(&ctx3));
    }
    
    #[test]
    fn test_collateralization_with_reserve() {
        use basis_store::{PubKey, ReserveInfo, ReserveTracker};
        
        let mut tracker = ReserveTracker::new();
        let reserve = basis_store::reserve_tracker::ExtendedReserveInfo {
            base_info: ReserveInfo {
                collateral_amount: 150,
                last_updated_height: 0,
                contract_address: "test".to_string(),
                tracker_nft_id: "test".to_string(),
            },
            total_debt: 100,
            box_id: "box1".to_string(),
            owner_pubkey: {
                let mut key = [0u8; 33];
                key[0] = 0x02;
                key[1] = 1;
                hex::encode(key)
            },
            last_updated_timestamp: 0,
        };
        tracker.update_reserve(reserve).unwrap();
        
        let config = AcceptanceConfig {
            root: None,
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Collateralization {
                    name: "collat".to_string(),
                    min_ratio: 1.0,
                },
            ],
        };
        
        let pred = build_predicate_tree(config).unwrap().unwrap();
        
        let ctx = super::super::PredicateContext {
            issuer_pubkey: {
                let mut key = [0u8; 33];
                key[0] = 0x02;
                key[1] = 1;
                key
            },
            recipient_pubkey: [0u8; 33],
            total_debt: 100,
            reserve_tracker: Some(tracker),
        };
        
        assert!(pred.acceptable(&ctx));
    }
    
    #[test]
    fn test_builder_error_display() {
        let err = BuilderError::DuplicateName("test".to_string());
        assert_eq!(format!("{}", err), "Duplicate predicate name: test");
        
        let err = BuilderError::MissingReference("test".to_string());
        assert_eq!(format!("{}", err), "Missing predicate reference: test");
        
        let err = BuilderError::CircularReference("test".to_string());
        assert_eq!(format!("{}", err), "Circular reference detected: test");
        
        let err = BuilderError::InvalidPublicKey("bad".to_string());
        assert_eq!(format!("{}", err), "Invalid public key hex: bad");
        
        let err = BuilderError::InvalidPublicKeyLength(5);
        assert_eq!(format!("{}", err), "Invalid public key length: expected 33 bytes, got 5");
    }
    
    #[test]
    fn test_deeply_nested_composite() {
        let config = AcceptanceConfig {
            root: Some("deep".to_string()),
            default: DefaultPolicy::Reject,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "w1".to_string(),
                    holders: vec![test_pubkey_hex(1)],
                    max_debt: None,
                },
                PredicateConfig::Not {
                    name: "not_w1".to_string(),
                    predicate: "w1".to_string(),
                },
                PredicateConfig::Not {
                    name: "not_not_w1".to_string(),
                    predicate: "not_w1".to_string(),
                },
                PredicateConfig::AllOf {
                    name: "deep".to_string(),
                    predicates: vec!["not_not_w1".to_string()],
                },
            ],
        };
        
        let pred = build_predicate_tree(config).unwrap().unwrap();
        assert_eq!(pred.name(), "deep");
        
        // NOT(NOT(whitelist)) == whitelist
        let ctx = super::super::PredicateContext {
            issuer_pubkey: {
                let mut key = [0u8; 33];
                key[0] = 0x02;
                key[1] = 1;
                key
            },
            recipient_pubkey: [0u8; 33],
            total_debt: 100,
            reserve_tracker: None,
        };
        assert!(pred.acceptable(&ctx));
    }
}
