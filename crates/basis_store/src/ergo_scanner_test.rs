//! Test utilities for Ergo scanner transaction processing

use super::ergo_scanner::{ErgoScanner, NodeConfig, ReserveEvent};
use serde_json::json;

/// Create a test transaction with Basis reserve boxes
pub fn create_test_reserve_creation_tx() -> serde_json::Value {
    json!({
        "id": "test_tx_1",
        "inputs": [],
        "outputs": [
            {
                "boxId": "reserve_box_1",
                "value": 1000000000, // 1 ERG
                "ergoTree": "0008cd0101010101010101010101010101010101010101",
                "creationHeight": 1000,
                "transactionId": "test_tx_1",
                "additionalRegisters": {
                    "R4": {
                        "serializedValue": "owner_pubkey_hex"
                    },
                    "R6": {
                        "serializedValue": "tracker_nft_id"
                    }
                }
            }
        ]
    })
}

/// Create a test transaction with reserve top-up
pub fn create_test_reserve_topup_tx() -> serde_json::Value {
    json!({
        "id": "test_tx_2",
        "inputs": [
            {
                "box": {
                    "boxId": "reserve_box_1",
                    "value": 1000000000,
                    "ergoTree": "0008cd0101010101010101010101010101010101010101",
                    "creationHeight": 1000,
                    "transactionId": "test_tx_1",
                    "additionalRegisters": {
                        "R4": {
                            "serializedValue": "owner_pubkey_hex"
                        },
                        "R6": {
                            "serializedValue": "tracker_nft_id"
                        }
                    }
                }
            }
        ],
        "outputs": [
            {
                "boxId": "reserve_box_1",
                "value": 1500000000, // Increased by 0.5 ERG
                "ergoTree": "0008cd0101010101010101010101010101010101010101",
                "creationHeight": 1001,
                "transactionId": "test_tx_2",
                "additionalRegisters": {
                    "R4": {
                        "serializedValue": "owner_pubkey_hex"
                    },
                    "R6": {
                        "serializedValue": "tracker_nft_id"
                    }
                }
            }
        ]
    })
}

/// Create a test transaction with reserve redemption
pub fn create_test_reserve_redemption_tx() -> serde_json::Value {
    json!({
        "id": "test_tx_3",
        "inputs": [
            {
                "box": {
                    "boxId": "reserve_box_1",
                    "value": 1500000000,
                    "ergoTree": "0008cd0101010101010101010101010101010101010101",
                    "creationHeight": 1001,
                    "transactionId": "test_tx_2",
                    "additionalRegisters": {
                        "R4": {
                            "serializedValue": "owner_pubkey_hex"
                        },
                        "R6": {
                            "serializedValue": "tracker_nft_id"
                        }
                    }
                }
            }
        ],
        "outputs": [
            {
                "boxId": "reserve_box_1",
                "value": 1200000000, // Reduced by 0.3 ERG
                "ergoTree": "0008cd0101010101010101010101010101010101010101",
                "creationHeight": 1002,
                "transactionId": "test_tx_3",
                "additionalRegisters": {
                    "R4": {
                        "serializedValue": "owner_pubkey_hex"
                    },
                    "R6": {
                        "serializedValue": "tracker_nft_id"
                    }
                }
            }
        ],
        "dataInputs": [
            {
                "boxId": "tracker_box_1"
            }
        ]
    })
}

/// Create a test transaction with reserve spending
pub fn create_test_reserve_spending_tx() -> serde_json::Value {
    json!({
        "id": "test_tx_4",
        "inputs": [
            {
                "box": {
                    "boxId": "reserve_box_1",
                    "value": 1200000000,
                    "ergoTree": "0008cd0101010101010101010101010101010101010101",
                    "creationHeight": 1002,
                    "transactionId": "test_tx_3",
                    "additionalRegisters": {
                        "R4": {
                            "serializedValue": "owner_pubkey_hex"
                        },
                        "R6": {
                            "serializedValue": "tracker_nft_id"
                        }
                    }
                }
            }
        ],
        "outputs": [
            {
                "boxId": "regular_box_1",
                "value": 1200000000,
                "ergoTree": "regular_script",
                "creationHeight": 1003,
                "transactionId": "test_tx_4"
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_reserve_creation_detection() {
        let config = NodeConfig::default();
        let scanner = ErgoScanner::new(config);
        
        let tx = create_test_reserve_creation_tx();
        let events = scanner.process_transaction(&tx, 1000).unwrap();
        
        assert!(events.is_some());
        let events = events.unwrap();
        assert_eq!(events.len(), 1);
        
        if let ReserveEvent::ReserveCreated { box_id, .. } = &events[0] {
            assert_eq!(box_id, "reserve_box_1");
        } else {
            panic!("Expected ReserveCreated event");
        }
    }
    
    #[tokio::test]
    async fn test_reserve_topup_detection() {
        let config = NodeConfig::default();
        let scanner = ErgoScanner::new(config);
        
        let tx = create_test_reserve_topup_tx();
        let events = scanner.process_transaction(&tx, 1001).unwrap();
        
        assert!(events.is_some());
        let events = events.unwrap();
        assert_eq!(events.len(), 1);
        
        if let ReserveEvent::ReserveToppedUp { box_id, additional_collateral, .. } = &events[0] {
            assert_eq!(box_id, "reserve_box_1");
            assert_eq!(additional_collateral, &500000000); // 0.5 ERG
        } else {
            panic!("Expected ReserveToppedUp event");
        }
    }
    
    #[tokio::test]
    async fn test_reserve_redemption_detection() {
        let config = NodeConfig::default();
        let scanner = ErgoScanner::new(config);
        
        let tx = create_test_reserve_redemption_tx();
        let events = scanner.process_transaction(&tx, 1002).unwrap();
        
        assert!(events.is_some());
        let events = events.unwrap();
        assert_eq!(events.len(), 1);
        
        if let ReserveEvent::ReserveRedeemed { box_id, redeemed_amount, .. } = &events[0] {
            assert_eq!(box_id, "reserve_box_1");
            assert_eq!(redeemed_amount, &300000000); // 0.3 ERG
        } else {
            panic!("Expected ReserveRedeemed event");
        }
    }
    
    #[tokio::test]
    async fn test_reserve_spending_detection() {
        let config = NodeConfig::default();
        let scanner = ErgoScanner::new(config);
        
        let tx = create_test_reserve_spending_tx();
        let events = scanner.process_transaction(&tx, 1003).unwrap();
        
        assert!(events.is_some());
        let events = events.unwrap();
        assert_eq!(events.len(), 1);
        
        if let ReserveEvent::ReserveSpent { box_id, .. } = &events[0] {
            assert_eq!(box_id, "reserve_box_1");
        } else {
            panic!("Expected ReserveSpent event");
        }
    }
}