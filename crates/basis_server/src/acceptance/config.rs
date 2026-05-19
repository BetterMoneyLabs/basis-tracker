//! Configuration structures for acceptance predicates
//!
//! Provides TOML-based configuration for acceptance policies.

use serde::{Deserialize, Serialize};

/// Top-level acceptance configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcceptanceConfig {
    /// Default policy when no predicate matches
    #[serde(default)]
    pub default: DefaultPolicy,
    /// Name of the root predicate to use as the top-level policy
    /// If not specified, the last predicate in the list is used
    #[serde(default)]
    pub root: Option<String>,
    /// List of predicate definitions
    #[serde(default)]
    pub predicates: Vec<PredicateConfig>,
}

impl Default for AcceptanceConfig {
    fn default() -> Self {
        Self {
            default: DefaultPolicy::Reject,
            root: None,
            predicates: Vec::new(),
        }
    }
}

impl AcceptanceConfig {
    /// Create an empty configuration with reject-by-default policy
    pub fn empty() -> Self {
        Self::default()
    }
    
    /// Load configuration from TOML string
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }
    
    /// Convert to TOML string
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}

/// Default policy when no predicate matches
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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

impl Default for DefaultPolicy {
    fn default() -> Self {
        DefaultPolicy::Reject
    }
}

/// Individual predicate configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PredicateConfig {
    /// Whitelist predicate - accepts if issuer is in the list
    Whitelist {
        /// Predicate name
        name: String,
        /// List of hex-encoded 33-byte compressed public keys
        holders: Vec<String>,
        /// Optional maximum cumulative debt (nanoERG)
        max_debt: Option<u64>,
    },
    /// Blacklist predicate - rejects if issuer is in the list
    Blacklist {
        /// Predicate name
        name: String,
        /// List of hex-encoded 33-byte compressed public keys
        holders: Vec<String>,
    },
    /// Collateralization predicate - accepts if reserve meets ratio
    Collateralization {
        /// Predicate name
        name: String,
        /// Minimum collateralization ratio (e.g., 1.0 = 100%)
        min_ratio: f64,
    },
    /// All-of composite predicate - all sub-predicates must pass
    AllOf {
        /// Predicate name
        name: String,
        /// Names of sub-predicates
        predicates: Vec<String>,
    },
    /// Any-of composite predicate - at least one sub-predicate must pass
    AnyOf {
        /// Predicate name
        name: String,
        /// Names of sub-predicates
        predicates: Vec<String>,
    },
    /// Not predicate - inverts the result
    Not {
        /// Predicate name
        name: String,
        /// Name of the predicate to negate
        predicate: String,
    },
}

impl PredicateConfig {
    /// Get the predicate name
    pub fn name(&self) -> &str {
        match self {
            PredicateConfig::Whitelist { name, .. } => name,
            PredicateConfig::Blacklist { name, .. } => name,
            PredicateConfig::Collateralization { name, .. } => name,
            PredicateConfig::AllOf { name, .. } => name,
            PredicateConfig::AnyOf { name, .. } => name,
            PredicateConfig::Not { name, .. } => name,
        }
    }
    
    /// Check if this is a composite predicate (references other predicates)
    pub fn is_composite(&self) -> bool {
        matches!(self, PredicateConfig::AllOf { .. } | PredicateConfig::AnyOf { .. } | PredicateConfig::Not { .. })
    }
    
    /// Check if this is a leaf predicate (no references)
    pub fn is_leaf(&self) -> bool {
        !self.is_composite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_acceptance_config_default() {
        let config = AcceptanceConfig::default();
        assert_eq!(config.default, DefaultPolicy::Reject);
        assert!(config.predicates.is_empty());
    }
    
    #[test]
    fn test_acceptance_config_empty() {
        let config = AcceptanceConfig::empty();
        assert_eq!(config.default, DefaultPolicy::Reject);
        assert!(config.predicates.is_empty());
    }
    
    #[test]
    fn test_parse_whitelist_config() {
        let toml = r#"
            default = "reject"
            
            [[predicates]]
            name = "trusted"
            type = "whitelist"
            holders = ["02a1b2c3d4e5f6..."]
            max_debt = 5000000000
        "#;
        
        let config = AcceptanceConfig::from_toml(toml).unwrap();
        assert_eq!(config.default, DefaultPolicy::Reject);
        assert_eq!(config.predicates.len(), 1);
        
        match &config.predicates[0] {
            PredicateConfig::Whitelist { name, holders, max_debt } => {
                assert_eq!(name, "trusted");
                assert_eq!(holders.len(), 1);
                assert_eq!(*max_debt, Some(5000000000));
            }
            _ => panic!("Expected Whitelist config"),
        }
    }
    
    #[test]
    fn test_parse_blacklist_config() {
        let toml = r#"
            default = "accept"
            
            [[predicates]]
            name = "sanctions"
            type = "blacklist"
            holders = ["02bad1...", "03bad2..."]
        "#;
        
        let config = AcceptanceConfig::from_toml(toml).unwrap();
        assert_eq!(config.default, DefaultPolicy::Accept);
        
        match &config.predicates[0] {
            PredicateConfig::Blacklist { name, holders } => {
                assert_eq!(name, "sanctions");
                assert_eq!(holders.len(), 2);
            }
            _ => panic!("Expected Blacklist config"),
        }
    }
    
    #[test]
    fn test_parse_collateralization_config() {
        let toml = r#"
            default = "reject"
            
            [[predicates]]
            name = "full_collat"
            type = "collateralization"
            min_ratio = 1.5
        "#;
        
        let config = AcceptanceConfig::from_toml(toml).unwrap();
        
        match &config.predicates[0] {
            PredicateConfig::Collateralization { name, min_ratio } => {
                assert_eq!(name, "full_collat");
                assert!((min_ratio - 1.5).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Collateralization config"),
        }
    }
    
    #[test]
    fn test_parse_composite_config() {
        let toml = r#"
            default = "reject"
            
            [[predicates]]
            name = "cow1"
            type = "any_of"
            predicates = ["whitelist", "collateralization"]
            
            [[predicates]]
            name = "strict"
            type = "all_of"
            predicates = ["whitelist", "collateralization"]
            
            [[predicates]]
            name = "not_blocked"
            type = "not"
            predicate = "blocked"
        "#;
        
        let config = AcceptanceConfig::from_toml(toml).unwrap();
        assert_eq!(config.predicates.len(), 3);
        
        match &config.predicates[0] {
            PredicateConfig::AnyOf { name, predicates } => {
                assert_eq!(name, "cow1");
                assert_eq!(predicates, &vec!["whitelist".to_string(), "collateralization".to_string()]);
            }
            _ => panic!("Expected AnyOf config"),
        }
        
        match &config.predicates[1] {
            PredicateConfig::AllOf { name, predicates } => {
                assert_eq!(name, "strict");
                assert_eq!(predicates.len(), 2);
            }
            _ => panic!("Expected AllOf config"),
        }
        
        match &config.predicates[2] {
            PredicateConfig::Not { name, predicate } => {
                assert_eq!(name, "not_blocked");
                assert_eq!(predicate, "blocked");
            }
            _ => panic!("Expected Not config"),
        }
    }
    
    #[test]
    fn test_whitelist_without_max_debt() {
        let toml = r#"
            [[predicates]]
            name = "friends"
            type = "whitelist"
            holders = ["02abc..."]
        "#;
        
        let config = AcceptanceConfig::from_toml(toml).unwrap();
        
        match &config.predicates[0] {
            PredicateConfig::Whitelist { max_debt, .. } => {
                assert_eq!(*max_debt, None);
            }
            _ => panic!("Expected Whitelist config"),
        }
    }
    
    #[test]
    fn test_predicate_name() {
        let w = PredicateConfig::Whitelist {
            name: "test".to_string(),
            holders: vec![],
            max_debt: None,
        };
        assert_eq!(w.name(), "test");
        
        let b = PredicateConfig::Blacklist {
            name: "black".to_string(),
            holders: vec![],
        };
        assert_eq!(b.name(), "black");
        
        let c = PredicateConfig::Collateralization {
            name: "collat".to_string(),
            min_ratio: 1.0,
        };
        assert_eq!(c.name(), "collat");
        
        let a = PredicateConfig::AllOf {
            name: "all".to_string(),
            predicates: vec![],
        };
        assert_eq!(a.name(), "all");
        
        let o = PredicateConfig::AnyOf {
            name: "any".to_string(),
            predicates: vec![],
        };
        assert_eq!(o.name(), "any");
        
        let n = PredicateConfig::Not {
            name: "not".to_string(),
            predicate: "inner".to_string(),
        };
        assert_eq!(n.name(), "not");
    }
    
    #[test]
    fn test_predicate_is_composite() {
        let w = PredicateConfig::Whitelist {
            name: "test".to_string(),
            holders: vec![],
            max_debt: None,
        };
        assert!(!w.is_composite());
        assert!(w.is_leaf());
        
        let a = PredicateConfig::AllOf {
            name: "all".to_string(),
            predicates: vec![],
        };
        assert!(a.is_composite());
        assert!(!a.is_leaf());
        
        let n = PredicateConfig::Not {
            name: "not".to_string(),
            predicate: "inner".to_string(),
        };
        assert!(n.is_composite());
    }
    
    #[test]
    fn test_default_policy_accept() {
        let policy = DefaultPolicy::Accept;
        assert_eq!(policy, DefaultPolicy::Accept);
    }
    
    #[test]
    fn test_default_policy_reject() {
        let policy = DefaultPolicy::Reject;
        assert_eq!(policy, DefaultPolicy::Reject);
    }
    
    #[test]
    fn test_roundtrip_toml() {
        let config = AcceptanceConfig {
            default: DefaultPolicy::Reject,
            root: None,
            predicates: vec![
                PredicateConfig::Whitelist {
                    name: "trusted".to_string(),
                    holders: vec!["02abc...".to_string()],
                    max_debt: Some(1000),
                },
                PredicateConfig::Blacklist {
                    name: "blocked".to_string(),
                    holders: vec!["03def...".to_string()],
                },
            ],
        };
        
        let toml = config.to_toml().unwrap();
        let parsed = AcceptanceConfig::from_toml(&toml).unwrap();
        
        assert_eq!(config, parsed);
    }
    
    #[test]
    fn test_parse_lets_config() {
        let toml = r#"
            default = "reject"
            
            [[predicates]]
            name = "lets_members"
            type = "whitelist"
            holders = [
                "02alice...",
                "02bob...",
                "02charlie..."
            ]
            max_debt = 5000000000
            
            [[predicates]]
            name = "municipality"
            type = "whitelist"
            holders = ["02municipality_key..."]
            
            [[predicates]]
            name = "endorsed_note"
            type = "any_of"
            predicates = ["lets_members", "municipality"]
        "#;
        
        let config = AcceptanceConfig::from_toml(toml).unwrap();
        assert_eq!(config.predicates.len(), 3);
        assert_eq!(config.default, DefaultPolicy::Reject);
    }
    
    #[test]
    fn test_parse_empty_config() {
        let toml = "";
        let config = AcceptanceConfig::from_toml(toml).unwrap();
        assert_eq!(config.default, DefaultPolicy::Reject);
        assert!(config.predicates.is_empty());
    }
    
    #[test]
    fn test_parse_config_with_only_default() {
        let toml = r#"default = "accept""#;
        let config = AcceptanceConfig::from_toml(toml).unwrap();
        assert_eq!(config.default, DefaultPolicy::Accept);
        assert!(config.predicates.is_empty());
    }
    
    #[test]
    fn test_invalid_toml_returns_error() {
        let toml = r#"invalid toml content {{"#;
        let result = AcceptanceConfig::from_toml(toml);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_serialize_deserialize_default_policy() {
        let policies = vec![DefaultPolicy::Accept, DefaultPolicy::Reject];
        
        for policy in policies {
            let serialized = serde_json::to_string(&policy).unwrap();
            let deserialized: DefaultPolicy = serde_json::from_str(&serialized).unwrap();
            assert_eq!(policy, deserialized);
        }
    }
    
    #[test]
    fn test_predicate_config_equality() {
        let w1 = PredicateConfig::Whitelist {
            name: "test".to_string(),
            holders: vec!["02abc".to_string()],
            max_debt: Some(100),
        };
        let w2 = PredicateConfig::Whitelist {
            name: "test".to_string(),
            holders: vec!["02abc".to_string()],
            max_debt: Some(100),
        };
        let w3 = PredicateConfig::Whitelist {
            name: "other".to_string(),
            holders: vec!["02abc".to_string()],
            max_debt: Some(100),
        };
        
        assert_eq!(w1, w2);
        assert_ne!(w1, w3);
    }
}
