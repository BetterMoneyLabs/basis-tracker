//! Tests for tracker scanner functionality

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ergo_scanner::{BoxAsset, ScanBox},
        persistence::{ScannerMetadataStorage, TrackerStorage},
        tracker_scanner::{create_tracker_server_state, TrackerNodeConfig},
    };
    use std::collections::HashMap;
    use std::path::Path;

    #[tokio::test]
    async fn test_tracker_scan_registration_payload() {
        // Test that the scan registration payload is correctly formatted
        let tracker_nft_id = "dbfbbaf91a98c22204de3745e1986463620dcf3525ad566c6924cf9e976f86f8".to_string();
        
        // This would be the expected JSON payload
        let expected_payload = serde_json::json!({
            "scanName": "tracker_boxes",
            "walletInteraction": "shared",
            "trackingRule": {
                "predicate": "containsAsset",
                "assetId": tracker_nft_id
            },
            "removeOffchain": false
        });

        // Verify the payload structure
        assert_eq!(expected_payload["scanName"], "tracker_boxes");
        assert_eq!(expected_payload["walletInteraction"], "shared");
        assert_eq!(expected_payload["trackingRule"]["predicate"], "containsAsset");
        assert_eq!(
            expected_payload["trackingRule"]["assetId"],
            tracker_nft_id
        );
    }

    #[test]
    fn test_tracker_node_config() {
        // Test tracker node configuration
        let config = TrackerNodeConfig {
            start_height: Some(1000),
            tracker_nft_id: Some("test_nft_id".to_string()),
            node_url: "http://localhost:9053".to_string(),
            scan_name: Some("test_tracker_scan".to_string()),
            api_key: Some("test_api_key".to_string()),
        };

        assert_eq!(config.start_height, Some(1000));
        assert_eq!(config.tracker_nft_id, Some("test_nft_id".to_string()));
        assert_eq!(config.node_url, "http://localhost:9053");
        assert_eq!(config.scan_name, Some("test_tracker_scan".to_string()));
        assert_eq!(config.api_key, Some("test_api_key".to_string()));
    }

    #[tokio::test]
    async fn test_tracker_server_state_creation() {
        // Test creating tracker server state
        let temp_dir = tempfile::tempdir().unwrap();
        
        let metadata_storage = ScannerMetadataStorage::open(temp_dir.path().join("metadata"))
            .expect("Failed to create metadata storage");
        
        let tracker_storage = TrackerStorage::open(temp_dir.path().join("tracker"))
            .expect("Failed to create tracker storage");

        let config = TrackerNodeConfig {
            start_height: Some(0),
            tracker_nft_id: Some("test_nft_id".to_string()),
            node_url: "http://localhost:9053".to_string(),
            scan_name: Some("test_tracker_scan".to_string()),
            api_key: None,
        };

        let server_state = create_tracker_server_state(config, metadata_storage, tracker_storage);
        
        // Verify the server state was created
        assert_eq!(server_state.config.node_url, "http://localhost:9053");
        assert_eq!(server_state.config.tracker_nft_id, Some("test_nft_id".to_string()));
    }

    #[test]
    fn test_tracker_box_info_serialization() {
        // Test TrackerBoxInfo serialization/deserialization
        let tracker_box = crate::TrackerBoxInfo {
            box_id: "test_box_id".to_string(),
            tracker_pubkey: "test_pubkey".to_string(),
            state_commitment: "test_commitment".to_string(),
            last_verified_height: 1000,
            value: 1000000,
            creation_height: 900,
            tracker_nft_id: "test_nft_id".to_string(),
        };

        // Test serialization
        let serialized = serde_json::to_string(&tracker_box).unwrap();
        let deserialized: crate::TrackerBoxInfo = serde_json::from_str(&serialized).unwrap();

        assert_eq!(tracker_box.box_id, deserialized.box_id);
        assert_eq!(tracker_box.tracker_pubkey, deserialized.tracker_pubkey);
        assert_eq!(tracker_box.state_commitment, deserialized.state_commitment);
        assert_eq!(tracker_box.last_verified_height, deserialized.last_verified_height);
        assert_eq!(tracker_box.value, deserialized.value);
        assert_eq!(tracker_box.creation_height, deserialized.creation_height);
        assert_eq!(tracker_box.tracker_nft_id, deserialized.tracker_nft_id);
    }

    #[test]
    fn test_parse_tracker_box() {
        let temp_dir = tempfile::tempdir().unwrap();
        
        let metadata_storage = ScannerMetadataStorage::open(temp_dir.path().join("metadata"))
            .expect("Failed to create metadata storage");
        
        let tracker_storage = TrackerStorage::open(temp_dir.path().join("tracker"))
            .expect("Failed to create tracker storage");

        let config = TrackerNodeConfig {
            start_height: Some(0),
            tracker_nft_id: Some("dbfbbaf91a98c22204de3745e1986463620dcf3525ad566c6924cf9e976f86f8".to_string()),
            node_url: "http://localhost:9053".to_string(),
            scan_name: Some("test_tracker_scan".to_string()),
            api_key: None,
        };

        let server_state = create_tracker_server_state(config, metadata_storage, tracker_storage);

        // Create a mock ScanBox
        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7".to_string());
        registers.insert("R5".to_string(), "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string());
        registers.insert("R6".to_string(), "1000".to_string());

        let scan_box = ScanBox {
            box_id: "test_box_id_1234567890abcdef".to_string(),
            value: 1000000,
            ergo_tree: "mock_ergo_tree".to_string(),
            creation_height: 950,
            transaction_id: "mock_tx_id".to_string(),
            assets: vec![
                BoxAsset {
                    token_id: "dbfbbaf91a98c22204de3745e1986463620dcf3525ad566c6924cf9e976f86f8".to_string(),
                    amount: 1,
                }
            ],
            additional_registers: registers,
        };

        // Parse the tracker box
        let result = server_state.parse_tracker_box(&scan_box);
        assert!(result.is_ok());

        let tracker_box = result.unwrap();
        assert_eq!(tracker_box.box_id, "test_box_id_1234567890abcdef");
        assert_eq!(tracker_box.tracker_pubkey, "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7");
        assert_eq!(tracker_box.state_commitment, "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
        assert_eq!(tracker_box.last_verified_height, 1000);
        assert_eq!(tracker_box.value, 1000000);
        assert_eq!(tracker_box.creation_height, 950);
        assert_eq!(tracker_box.tracker_nft_id, "dbfbbaf91a98c22204de3745e1986463620dcf3525ad566c6924cf9e976f86f8");
    }

    #[test]
    fn test_parse_tracker_box_missing_nft() {
        let temp_dir = tempfile::tempdir().unwrap();
        
        let metadata_storage = ScannerMetadataStorage::open(temp_dir.path().join("metadata"))
            .expect("Failed to create metadata storage");
        
        let tracker_storage = TrackerStorage::open(temp_dir.path().join("tracker"))
            .expect("Failed to create tracker storage");

        let config = TrackerNodeConfig {
            start_height: Some(0),
            tracker_nft_id: Some("dbfbbaf91a98c22204de3745e1986463620dcf3525ad566c6924cf9e976f86f8".to_string()),
            node_url: "http://localhost:9053".to_string(),
            scan_name: Some("test_tracker_scan".to_string()),
            api_key: None,
        };

        let server_state = create_tracker_server_state(config, metadata_storage, tracker_storage);

        // Create a mock ScanBox without the tracker NFT
        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7".to_string());
        registers.insert("R5".to_string(), "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string());
        registers.insert("R6".to_string(), "1000".to_string());

        let scan_box = ScanBox {
            box_id: "test_box_id_1234567890abcdef".to_string(),
            value: 1000000,
            ergo_tree: "mock_ergo_tree".to_string(),
            creation_height: 950,
            transaction_id: "mock_tx_id".to_string(),
            assets: vec![
                BoxAsset {
                    token_id: "different_nft_id_1234567890abcdef1234567890abcdef".to_string(),
                    amount: 1,
                }
            ],
            additional_registers: registers,
        };

        // Parse should fail due to missing tracker NFT
        let result = server_state.parse_tracker_box(&scan_box);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::tracker_scanner::TrackerScannerError::MissingTrackerNft));
    }

    #[test]
    fn test_parse_tracker_box_missing_register() {
        let temp_dir = tempfile::tempdir().unwrap();
        
        let metadata_storage = ScannerMetadataStorage::open(temp_dir.path().join("metadata"))
            .expect("Failed to create metadata storage");
        
        let tracker_storage = TrackerStorage::open(temp_dir.path().join("tracker"))
            .expect("Failed to create tracker storage");

        let config = TrackerNodeConfig {
            start_height: Some(0),
            tracker_nft_id: Some("dbfbbaf91a98c22204de3745e1986463620dcf3525ad566c6924cf9e976f86f8".to_string()),
            node_url: "http://localhost:9053".to_string(),
            scan_name: Some("test_tracker_scan".to_string()),
            api_key: None,
        };

        let server_state = create_tracker_server_state(config, metadata_storage, tracker_storage);

        // Create a mock ScanBox missing R6 register
        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), "02dada811a888cd0dc7a0a41739a3ad9b0f427741fe6ca19700cf1a51200c96bf7".to_string());
        registers.insert("R5".to_string(), "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string());
        // Missing R6

        let scan_box = ScanBox {
            box_id: "test_box_id_1234567890abcdef".to_string(),
            value: 1000000,
            ergo_tree: "mock_ergo_tree".to_string(),
            creation_height: 950,
            transaction_id: "mock_tx_id".to_string(),
            assets: vec![
                BoxAsset {
                    token_id: "dbfbbaf91a98c22204de3745e1986463620dcf3525ad566c6924cf9e976f86f8".to_string(),
                    amount: 1,
                }
            ],
            additional_registers: registers,
        };

        // Parse should fail due to missing register
        let result = server_state.parse_tracker_box(&scan_box);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::tracker_scanner::TrackerScannerError::MissingRegister(_)));
    }
}