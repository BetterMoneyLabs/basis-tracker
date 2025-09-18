//! Reserve tracker for monitoring Basis reserve contracts on-chain

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

use crate::ReserveInfo;

#[derive(Error, Debug)]
pub enum ReserveTrackerError {
    #[error("Serialization error")]
    SerializationError,
    #[error("Reserve box parsing error: {0}")]
    BoxParsingError(String),
    #[error("Reserve not found: {0}")]
    ReserveNotFound(String),
    #[error("Insufficient collateral: {0} < {1}")]
    InsufficientCollateral(u64, u64),
}

/// Extended reserve information with debt tracking
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtendedReserveInfo {
    /// Base reserve information
    pub base_info: ReserveInfo,
    /// Total issued debt against this reserve
    pub total_debt: u64,
    /// Reserve contract box ID (hex encoded)
    pub box_id: String,
    /// Owner's public key (hex encoded)
    pub owner_pubkey: String,
    /// Tracker NFT ID (if any, hex encoded)
    pub tracker_nft_id: Option<String>,
    /// Last update timestamp
    pub last_updated_timestamp: u64,
}

impl ExtendedReserveInfo {
    /// Calculate collateralization ratio (collateral / debt)
    pub fn collateralization_ratio(&self) -> f64 {
        if self.total_debt == 0 {
            f64::INFINITY
        } else {
            self.base_info.collateral_amount as f64 / self.total_debt as f64
        }
    }

    /// Check if reserve is sufficiently collateralized
    pub fn is_sufficiently_collateralized(&self, amount: u64) -> bool {
        let new_debt = self.total_debt + amount;
        new_debt <= self.base_info.collateral_amount
    }

    /// Check if reserve is at warning level (80% utilization)
    pub fn is_warning_level(&self) -> bool {
        self.collateralization_ratio() <= 1.25 // 80% utilization
    }

    /// Check if reserve is at critical level (100% utilization)  
    pub fn is_critical_level(&self) -> bool {
        self.collateralization_ratio() <= 1.0 // 100% utilization
    }
}

/// Reserve tracker that monitors Basis reserve contracts
#[derive(Clone)]
pub struct ReserveTracker {
    reserves: Arc<RwLock<HashMap<String, ExtendedReserveInfo>>>,
}

impl ReserveTracker {
    /// Create a new reserve tracker
    pub fn new() -> Self {
        Self {
            reserves: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add or update a reserve
    pub fn update_reserve(&self, info: ExtendedReserveInfo) -> Result<(), ReserveTrackerError> {
        let mut reserves = self.reserves.write().unwrap();
        reserves.insert(info.box_id.clone(), info);
        Ok(())
    }

    /// Get reserve information by box ID
    pub fn get_reserve(&self, box_id: &str) -> Result<ExtendedReserveInfo, ReserveTrackerError> {
        let reserves = self.reserves.read().unwrap();
        reserves
            .get(box_id)
            .cloned()
            .ok_or_else(|| ReserveTrackerError::ReserveNotFound(box_id.to_string()))
    }

    /// Get reserve information by owner public key
    pub fn get_reserve_by_owner(
        &self,
        owner_pubkey: &str,
    ) -> Result<ExtendedReserveInfo, ReserveTrackerError> {
        let reserves = self.reserves.read().unwrap();
        reserves
            .values()
            .find(|reserve| reserve.owner_pubkey == owner_pubkey)
            .cloned()
            .ok_or_else(|| ReserveTrackerError::ReserveNotFound(owner_pubkey.to_string()))
    }

    /// Get all reserves
    pub fn get_all_reserves(&self) -> Vec<ExtendedReserveInfo> {
        let reserves = self.reserves.read().unwrap();
        reserves.values().cloned().collect()
    }

    /// Remove a reserve
    pub fn remove_reserve(&self, box_id: &str) -> Result<(), ReserveTrackerError> {
        let mut reserves = self.reserves.write().unwrap();
        reserves
            .remove(box_id)
            .map(|_| ())
            .ok_or_else(|| ReserveTrackerError::ReserveNotFound(box_id.to_string()))
    }

    /// Add debt to a reserve
    pub fn add_debt(&self, box_id: &str, amount: u64) -> Result<(), ReserveTrackerError> {
        let mut reserves = self.reserves.write().unwrap();
        let reserve = reserves
            .get_mut(box_id)
            .ok_or_else(|| ReserveTrackerError::ReserveNotFound(box_id.to_string()))?;

        if !reserve.is_sufficiently_collateralized(amount) {
            return Err(ReserveTrackerError::InsufficientCollateral(
                reserve.base_info.collateral_amount,
                reserve.total_debt + amount,
            ));
        }

        reserve.total_debt += amount;
        Ok(())
    }

    /// Remove debt from a reserve (when notes are redeemed)
    pub fn remove_debt(&self, box_id: &str, amount: u64) -> Result<(), ReserveTrackerError> {
        let mut reserves = self.reserves.write().unwrap();
        let reserve = reserves
            .get_mut(box_id)
            .ok_or_else(|| ReserveTrackerError::ReserveNotFound(box_id.to_string()))?;

        if amount > reserve.total_debt {
            reserve.total_debt = 0;
        } else {
            reserve.total_debt -= amount;
        }

        Ok(())
    }

    /// Update collateral amount for a reserve
    pub fn update_collateral(
        &self,
        box_id: &str,
        new_collateral: u64,
    ) -> Result<(), ReserveTrackerError> {
        let mut reserves = self.reserves.write().unwrap();
        let reserve = reserves
            .get_mut(box_id)
            .ok_or_else(|| ReserveTrackerError::ReserveNotFound(box_id.to_string()))?;

        reserve.base_info.collateral_amount = new_collateral;
        Ok(())
    }

    /// Check if a reserve can support additional debt
    pub fn can_support_debt(&self, box_id: &str, amount: u64) -> Result<bool, ReserveTrackerError> {
        let reserves = self.reserves.read().unwrap();
        let reserve = reserves
            .get(box_id)
            .ok_or_else(|| ReserveTrackerError::ReserveNotFound(box_id.to_string()))?;

        Ok(reserve.is_sufficiently_collateralized(amount))
    }

    /// Get reserves at warning level (<= 125% collateralization)
    pub fn get_warning_reserves(&self) -> Vec<ExtendedReserveInfo> {
        let reserves = self.reserves.read().unwrap();
        reserves
            .values()
            .filter(|reserve| reserve.is_warning_level())
            .cloned()
            .collect()
    }

    /// Get reserves at critical level (<= 100% collateralization)
    pub fn get_critical_reserves(&self) -> Vec<ExtendedReserveInfo> {
        let reserves = self.reserves.read().unwrap();
        reserves
            .values()
            .filter(|reserve| reserve.is_critical_level())
            .cloned()
            .collect()
    }

    /// Get total system collateral and debt
    pub fn get_system_totals(&self) -> (u64, u64) {
        let reserves = self.reserves.read().unwrap();
        let total_collateral = reserves
            .values()
            .map(|r| r.base_info.collateral_amount)
            .sum();
        let total_debt = reserves.values().map(|r| r.total_debt).sum();
        (total_collateral, total_debt)
    }
}

// Manual implementation for tests and examples
impl ExtendedReserveInfo {
    /// Create a new extended reserve info from raw components
    pub fn new(
        box_id: &[u8],
        owner_pubkey: &[u8],
        collateral_amount: u64,
        tracker_nft_id: Option<&[u8]>,
        last_updated_height: u64,
    ) -> Self {
        Self {
            base_info: ReserveInfo {
                collateral_amount,
                last_updated_height,
                contract_address: "".to_string(), // Placeholder
            },
            total_debt: 0,
            box_id: hex::encode(box_id),
            owner_pubkey: hex::encode(owner_pubkey),
            tracker_nft_id: tracker_nft_id.map(hex::encode),
            last_updated_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reserve_operations() {
        let tracker = ReserveTracker::new();

        // Create a test reserve
        let reserve_info = ExtendedReserveInfo::new(
            b"test_box_id_1234567890",
            b"test_owner_pubkey_1234567890123456",
            1000000000, // 1 ERG
            Some(b"test_tracker_nft_1234567890"),
            1000,
        );

        // Add reserve
        tracker.update_reserve(reserve_info.clone()).unwrap();

        // Get reserve
        let retrieved = tracker.get_reserve(&reserve_info.box_id).unwrap();
        assert_eq!(retrieved.base_info.collateral_amount, 1000000000);
        assert_eq!(retrieved.total_debt, 0);

        // Add debt
        tracker.add_debt(&reserve_info.box_id, 500000000).unwrap(); // 0.5 ERG debt
        let after_debt = tracker.get_reserve(&reserve_info.box_id).unwrap();
        assert_eq!(after_debt.total_debt, 500000000);
        assert!(after_debt.is_sufficiently_collateralized(500000000)); // Can add another 0.5 ERG

        // Try to add too much debt
        let result = tracker.add_debt(&reserve_info.box_id, 600000000); // 0.6 ERG more
        assert!(result.is_err());

        // Remove debt
        tracker
            .remove_debt(&reserve_info.box_id, 300000000)
            .unwrap(); // Remove 0.3 ERG
        let after_removal = tracker.get_reserve(&reserve_info.box_id).unwrap();
        assert_eq!(after_removal.total_debt, 200000000);

        // Update collateral
        tracker
            .update_collateral(&reserve_info.box_id, 2000000000)
            .unwrap(); // Increase to 2 ERG
        let after_collateral = tracker.get_reserve(&reserve_info.box_id).unwrap();
        assert_eq!(after_collateral.base_info.collateral_amount, 2000000000);

        // Get all reserves
        let all_reserves = tracker.get_all_reserves();
        assert_eq!(all_reserves.len(), 1);

        // Remove reserve
        tracker.remove_reserve(&reserve_info.box_id).unwrap();
        let result = tracker.get_reserve(&reserve_info.box_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_collateralization_ratios() {
        let reserve = ExtendedReserveInfo {
            base_info: ReserveInfo {
                collateral_amount: 1000,
                last_updated_height: 0,
                contract_address: "test".to_string(),
            },
            total_debt: 0,
            box_id: "test".to_string(),
            owner_pubkey: "test".to_string(),
            tracker_nft_id: None,
            last_updated_timestamp: 0,
        };

        // Infinite ratio when no debt
        assert_eq!(reserve.collateralization_ratio(), f64::INFINITY);

        let reserve_with_debt = ExtendedReserveInfo {
            base_info: ReserveInfo {
                collateral_amount: 1000,
                last_updated_height: 0,
                contract_address: "test".to_string(),
            },
            total_debt: 800,
            ..reserve.clone()
        };

        // 125% collateralization (80% utilization)
        assert_eq!(reserve_with_debt.collateralization_ratio(), 1.25);
        assert!(reserve_with_debt.is_warning_level());
        assert!(!reserve_with_debt.is_critical_level());

        let critical_reserve = ExtendedReserveInfo {
            base_info: ReserveInfo {
                collateral_amount: 1000,
                last_updated_height: 0,
                contract_address: "test".to_string(),
            },
            total_debt: 1000,
            ..reserve
        };

        // 100% collateralization (100% utilization)
        assert_eq!(critical_reserve.collateralization_ratio(), 1.0);
        assert!(critical_reserve.is_warning_level());
        assert!(critical_reserve.is_critical_level());
    }
}
