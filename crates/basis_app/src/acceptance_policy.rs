//! Acceptance policy helper functions for TUI
//!
//! These functions manage the acceptance policy configuration used by the TUI
//! to control which IOU notes are accepted based on issuer criteria.

use basis_core::acceptance::{AcceptanceConfig, PredicateConfig};
use std::collections::HashSet;

/// Extract policy summary: (collateral_percentage, whitelist_count, blacklist_count)
pub fn get_policy_summary(config: &AcceptanceConfig) -> (u16, usize, usize) {
    let collateral_pct = config.predicates.iter()
        .find_map(|p| match p {
            PredicateConfig::Collateralization { min_ratio, .. } => Some((*min_ratio * 100.0) as u16),
            _ => None,
        })
        .unwrap_or(100);
    
    let whitelist_count = get_whitelist_entries(config).len();
    let blacklist_count = get_blacklist_entries(config).len();
    
    (collateral_pct, whitelist_count, blacklist_count)
}

/// Get all whitelist entries as (name, pubkey) tuples
/// Note: Names are "Unknown" since config only stores pubkeys
pub fn get_whitelist_entries(config: &AcceptanceConfig) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for predicate in &config.predicates {
        if let PredicateConfig::Whitelist { holders, .. } = predicate {
            for pubkey in holders {
                entries.push(("Unknown".to_string(), pubkey.clone()));
            }
        }
    }
    entries
}

/// Get all blacklist entries as pubkeys
pub fn get_blacklist_entries(config: &AcceptanceConfig) -> Vec<String> {
    let mut entries = Vec::new();
    for predicate in &config.predicates {
        if let PredicateConfig::Blacklist { holders, .. } = predicate {
            for pubkey in holders {
                entries.push(pubkey.clone());
            }
        }
    }
    entries
}

/// Remove a pubkey from the whitelist
pub fn remove_from_whitelist(config: &AcceptanceConfig, pubkey: &str) -> AcceptanceConfig {
    let mut new_config = config.clone();
    for predicate in &mut new_config.predicates {
        if let PredicateConfig::Whitelist { holders, .. } = predicate {
            holders.retain(|p| p != pubkey);
        }
    }
    new_config
}

/// Remove a pubkey from the blacklist
pub fn remove_from_blacklist(config: &AcceptanceConfig, pubkey: &str) -> AcceptanceConfig {
    let mut new_config = config.clone();
    for predicate in &mut new_config.predicates {
        if let PredicateConfig::Blacklist { holders, .. } = predicate {
            holders.retain(|p| p != pubkey);
        }
    }
    new_config
}

/// Create or update a policy by adding whitelist/blacklist entries or updating collateral
pub fn create_policy(
    existing: &AcceptanceConfig,
    whitelist_add: Option<HashSet<String>>,
    blacklist_add: Option<HashSet<String>>,
    collateral_pct: Option<u16>,
) -> AcceptanceConfig {
    let mut config = existing.clone();
    
    // Add whitelist entries
    if let Some(new_holders) = whitelist_add {
        let mut found = false;
        for predicate in &mut config.predicates {
            if let PredicateConfig::Whitelist { holders, .. } = predicate {
                holders.extend(new_holders.iter().cloned());
                found = true;
                break;
            }
        }
        if !found {
            let holders: Vec<String> = new_holders.iter().cloned().collect();
            config.predicates.push(PredicateConfig::Whitelist {
                name: "whitelist".to_string(),
                holders,
                max_debt: None,
            });
        }
    }
    
    // Add blacklist entries
    if let Some(new_holders) = blacklist_add {
        let mut found = false;
        for predicate in &mut config.predicates {
            if let PredicateConfig::Blacklist { holders, .. } = predicate {
                holders.extend(new_holders.iter().cloned());
                found = true;
                break;
            }
        }
        if !found {
            let holders: Vec<String> = new_holders.iter().cloned().collect();
            config.predicates.push(PredicateConfig::Blacklist {
                name: "blacklist".to_string(),
                holders,
            });
        }
    }
    
    // Update collateral
    if let Some(pct) = collateral_pct {
        let ratio = (pct as f64) / 100.0;
        let mut found = false;
        for predicate in &mut config.predicates {
            if let PredicateConfig::Collateralization { min_ratio, .. } = predicate {
                *min_ratio = ratio;
                found = true;
                break;
            }
        }
        if !found {
            config.predicates.push(PredicateConfig::Collateralization {
                name: "collateral".to_string(),
                min_ratio: ratio,
            });
        }
    }
    
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_pubkey_1() -> String {
        "02993a1453401d3181fa56f4dd4807d26b02df5708ebdfc51ea89023362a9fd2a2".to_string()
    }
    
    fn test_pubkey_2() -> String {
        "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea".to_string()
    }
    
    fn test_pubkey_3() -> String {
        "02a3b5c7d9e1f3a5b7c9d1e3f5a7b9c1d3e5f7a9b1c3d5e7f9a1b3c5d7e9f1a3b5c".to_string()
    }

    // ==================== get_policy_summary tests ====================
    
    #[test]
    fn test_policy_summary_default() {
        let config = AcceptanceConfig::default_collateral();
        let (collateral, whitelist, blacklist) = get_policy_summary(&config);
        
        assert_eq!(collateral, 100);
        assert_eq!(whitelist, 0);
        assert_eq!(blacklist, 0);
    }
    
    #[test]
    fn test_policy_summary_with_whitelist() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1(), test_pubkey_2()],
            max_debt: None,
        });
        
        let (collateral, whitelist, blacklist) = get_policy_summary(&config);
        
        assert_eq!(collateral, 100);
        assert_eq!(whitelist, 2);
        assert_eq!(blacklist, 0);
    }
    
    #[test]
    fn test_policy_summary_with_blacklist() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Blacklist {
            name: "blacklist".to_string(),
            holders: vec![test_pubkey_1()],
        });
        
        let (collateral, whitelist, blacklist) = get_policy_summary(&config);
        
        assert_eq!(collateral, 100);
        assert_eq!(whitelist, 0);
        assert_eq!(blacklist, 1);
    }
    
    #[test]
    fn test_policy_summary_custom_collateral() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Collateralization {
            name: "collateral".to_string(),
            min_ratio: 2.5,
        });
        
        let (collateral, whitelist, blacklist) = get_policy_summary(&config);
        
        // Should find the first collateralization predicate
        assert_eq!(collateral, 100); // The default one has 1.0
        assert_eq!(whitelist, 0);
        assert_eq!(blacklist, 0);
    }
    
    #[test]
    fn test_policy_summary_multiple_predicates() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: None,
        });
        config.predicates.push(PredicateConfig::Blacklist {
            name: "blacklist".to_string(),
            holders: vec![test_pubkey_2(), test_pubkey_3()],
        });
        
        let (collateral, whitelist, blacklist) = get_policy_summary(&config);
        
        assert_eq!(collateral, 100);
        assert_eq!(whitelist, 1);
        assert_eq!(blacklist, 2);
    }
    
    // ==================== get_whitelist_entries tests ====================
    
    #[test]
    fn test_get_whitelist_entries_empty() {
        let config = AcceptanceConfig::default_collateral();
        let entries = get_whitelist_entries(&config);
        assert!(entries.is_empty());
    }
    
    #[test]
    fn test_get_whitelist_entries_single() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: None,
        });
        
        let entries = get_whitelist_entries(&config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "Unknown");
        assert_eq!(entries[0].1, test_pubkey_1());
    }
    
    #[test]
    fn test_get_whitelist_entries_multiple() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1(), test_pubkey_2()],
            max_debt: None,
        });
        
        let entries = get_whitelist_entries(&config);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1, test_pubkey_1());
        assert_eq!(entries[1].1, test_pubkey_2());
    }
    
    #[test]
    fn test_get_whitelist_entries_multiple_predicates() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist1".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: None,
        });
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist2".to_string(),
            holders: vec![test_pubkey_2()],
            max_debt: None,
        });
        
        let entries = get_whitelist_entries(&config);
        assert_eq!(entries.len(), 2);
    }
    
    // ==================== get_blacklist_entries tests ====================
    
    #[test]
    fn test_get_blacklist_entries_empty() {
        let config = AcceptanceConfig::default_collateral();
        let entries = get_blacklist_entries(&config);
        assert!(entries.is_empty());
    }
    
    #[test]
    fn test_get_blacklist_entries_single() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Blacklist {
            name: "blacklist".to_string(),
            holders: vec![test_pubkey_1()],
        });
        
        let entries = get_blacklist_entries(&config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], test_pubkey_1());
    }
    
    #[test]
    fn test_get_blacklist_entries_multiple_predicates() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Blacklist {
            name: "blacklist1".to_string(),
            holders: vec![test_pubkey_1()],
        });
        config.predicates.push(PredicateConfig::Blacklist {
            name: "blacklist2".to_string(),
            holders: vec![test_pubkey_2(), test_pubkey_3()],
        });
        
        let entries = get_blacklist_entries(&config);
        assert_eq!(entries.len(), 3);
    }
    
    // ==================== remove_from_whitelist tests ====================
    
    #[test]
    fn test_remove_from_whitelist_existing() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1(), test_pubkey_2()],
            max_debt: None,
        });
        
        let new_config = remove_from_whitelist(&config, &test_pubkey_1());
        let entries = get_whitelist_entries(&new_config);
        
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, test_pubkey_2());
    }
    
    #[test]
    fn test_remove_from_whitelist_nonexistent() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: None,
        });
        
        let new_config = remove_from_whitelist(&config, &test_pubkey_2());
        let entries = get_whitelist_entries(&new_config);
        
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, test_pubkey_1());
    }
    
    #[test]
    fn test_remove_from_whitelist_empty() {
        let config = AcceptanceConfig::default_collateral();
        
        let new_config = remove_from_whitelist(&config, &test_pubkey_1());
        let entries = get_whitelist_entries(&new_config);
        
        assert!(entries.is_empty());
    }
    
    #[test]
    fn test_remove_from_whitelist_last_entry() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: None,
        });
        
        let new_config = remove_from_whitelist(&config, &test_pubkey_1());
        let entries = get_whitelist_entries(&new_config);
        
        assert!(entries.is_empty());
    }
    
    // ==================== remove_from_blacklist tests ====================
    
    #[test]
    fn test_remove_from_blacklist_existing() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Blacklist {
            name: "blacklist".to_string(),
            holders: vec![test_pubkey_1(), test_pubkey_2()],
        });
        
        let new_config = remove_from_blacklist(&config, &test_pubkey_1());
        let entries = get_blacklist_entries(&new_config);
        
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], test_pubkey_2());
    }
    
    #[test]
    fn test_remove_from_blacklist_nonexistent() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Blacklist {
            name: "blacklist".to_string(),
            holders: vec![test_pubkey_1()],
        });
        
        let new_config = remove_from_blacklist(&config, &test_pubkey_2());
        let entries = get_blacklist_entries(&new_config);
        
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], test_pubkey_1());
    }
    
    // ==================== create_policy tests ====================
    
    #[test]
    fn test_create_policy_add_whitelist_to_empty() {
        let config = AcceptanceConfig::default_collateral();
        let mut holders = HashSet::new();
        holders.insert(test_pubkey_1());
        
        let new_config = create_policy(&config, Some(holders), None, None);
        let entries = get_whitelist_entries(&new_config);
        
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, test_pubkey_1());
    }
    
    #[test]
    fn test_create_policy_add_whitelist_to_existing() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: None,
        });
        
        let mut holders = HashSet::new();
        holders.insert(test_pubkey_2());
        
        let new_config = create_policy(&config, Some(holders), None, None);
        let entries = get_whitelist_entries(&new_config);
        
        assert_eq!(entries.len(), 2);
    }
    
    #[test]
    fn test_create_policy_add_blacklist() {
        let config = AcceptanceConfig::default_collateral();
        let mut holders = HashSet::new();
        holders.insert(test_pubkey_1());
        
        let new_config = create_policy(&config, None, Some(holders), None);
        let entries = get_blacklist_entries(&new_config);
        
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], test_pubkey_1());
    }
    
    #[test]
    fn test_create_policy_update_collateral() {
        let config = AcceptanceConfig::default_collateral();
        
        let new_config = create_policy(&config, None, None, Some(250));
        let (collateral, _, _) = get_policy_summary(&new_config);
        
        assert_eq!(collateral, 250);
    }
    
    #[test]
    fn test_create_policy_update_existing_collateral() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Collateralization {
            name: "collateral".to_string(),
            min_ratio: 1.0,
        });
        
        let new_config = create_policy(&config, None, None, Some(150));
        let (collateral, _, _) = get_policy_summary(&new_config);
        
        assert_eq!(collateral, 150);
    }
    
    #[test]
    fn test_create_policy_combined() {
        let config = AcceptanceConfig::default_collateral();
        let mut whitelist = HashSet::new();
        whitelist.insert(test_pubkey_1());
        let mut blacklist = HashSet::new();
        blacklist.insert(test_pubkey_2());
        
        let new_config = create_policy(&config, Some(whitelist), Some(blacklist), Some(200));
        let (collateral, wl_count, bl_count) = get_policy_summary(&new_config);
        
        assert_eq!(collateral, 200);
        assert_eq!(wl_count, 1);
        assert_eq!(bl_count, 1);
    }
    
    #[test]
    fn test_create_policy_no_changes() {
        let config = AcceptanceConfig::default_collateral();
        
        let new_config = create_policy(&config, None, None, None);
        let (collateral, whitelist, blacklist) = get_policy_summary(&new_config);
        
        assert_eq!(collateral, 100);
        assert_eq!(whitelist, 0);
        assert_eq!(blacklist, 0);
    }
    
    #[test]
    fn test_create_policy_whitelist_no_deduplication() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: None,
        });
        
        // Try to add the same pubkey again - Vec.extend doesn't deduplicate
        let mut holders = HashSet::new();
        holders.insert(test_pubkey_1());
        
        let new_config = create_policy(&config, Some(holders), None, None);
        let entries = get_whitelist_entries(&new_config);
        
        // Note: Vec::extend doesn't deduplicate, so we get 2 entries
        // This is current behavior - deduplication would need to be explicit
        assert_eq!(entries.len(), 2);
    }
    
    // ==================== Edge case tests ====================
    
    #[test]
    fn test_empty_config() {
        let config = AcceptanceConfig::empty();
        let (collateral, whitelist, blacklist) = get_policy_summary(&config);
        
        assert_eq!(collateral, 100);
        assert_eq!(whitelist, 0);
        assert_eq!(blacklist, 0);
    }
    
    #[test]
    fn test_multiple_whitelist_predicates() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "group1".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: Some(1000),
        });
        config.predicates.push(PredicateConfig::Whitelist {
            name: "group2".to_string(),
            holders: vec![test_pubkey_2(), test_pubkey_3()],
            max_debt: Some(5000),
        });
        
        let entries = get_whitelist_entries(&config);
        assert_eq!(entries.len(), 3);
        
        let summary = get_policy_summary(&config);
        assert_eq!(summary.1, 3); // 3 whitelist entries
    }
    
    #[test]
    fn test_remove_from_whitelist_multiple_predicates() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "group1".to_string(),
            holders: vec![test_pubkey_1(), test_pubkey_2()],
            max_debt: None,
        });
        config.predicates.push(PredicateConfig::Whitelist {
            name: "group2".to_string(),
            holders: vec![test_pubkey_2(), test_pubkey_3()],
            max_debt: None,
        });
        
        // Remove pubkey_2 - should be removed from both predicates
        let new_config = remove_from_whitelist(&config, &test_pubkey_2());
        let entries = get_whitelist_entries(&new_config);
        
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|(_, pk)| pk == &test_pubkey_1()));
        assert!(entries.iter().any(|(_, pk)| pk == &test_pubkey_3()));
    }
    
    #[test]
    fn test_collateral_zero_percent() {
        let config = AcceptanceConfig::default_collateral();
        let new_config = create_policy(&config, None, None, Some(0));
        let (collateral, _, _) = get_policy_summary(&new_config);
        
        assert_eq!(collateral, 0);
    }
    
    #[test]
    fn test_collateral_high_percent() {
        let config = AcceptanceConfig::default_collateral();
        let new_config = create_policy(&config, None, None, Some(1000));
        let (collateral, _, _) = get_policy_summary(&new_config);
        
        assert_eq!(collateral, 1000);
    }
    
    #[test]
    fn test_roundtrip_policy_manipulation() {
        // Start with default
        let config = AcceptanceConfig::default_collateral();
        
        // Add whitelist
        let mut holders = HashSet::new();
        holders.insert(test_pubkey_1());
        let config = create_policy(&config, Some(holders), None, None);
        
        // Add blacklist
        let mut holders = HashSet::new();
        holders.insert(test_pubkey_2());
        let config = create_policy(&config, None, Some(holders), None);
        
        // Update collateral
        let config = create_policy(&config, None, None, Some(150));
        
        // Verify
        let (collateral, whitelist, blacklist) = get_policy_summary(&config);
        assert_eq!(collateral, 150);
        assert_eq!(whitelist, 1);
        assert_eq!(blacklist, 1);
        
        // Remove from whitelist
        let config = remove_from_whitelist(&config, &test_pubkey_1());
        let (_, whitelist, _) = get_policy_summary(&config);
        assert_eq!(whitelist, 0);
        
        // Remove from blacklist
        let config = remove_from_blacklist(&config, &test_pubkey_2());
        let (_, _, blacklist) = get_policy_summary(&config);
        assert_eq!(blacklist, 0);
    }
    
    #[test]
    fn test_policy_with_max_debt() {
        let mut config = AcceptanceConfig::default_collateral();
        config.predicates.push(PredicateConfig::Whitelist {
            name: "whitelist".to_string(),
            holders: vec![test_pubkey_1()],
            max_debt: Some(10000),
        });
        
        let entries = get_whitelist_entries(&config);
        assert_eq!(entries.len(), 1);
        // max_debt is preserved in the config but not exposed in entries
        // This is expected behavior - entries only return (name, pubkey)
    }
}
