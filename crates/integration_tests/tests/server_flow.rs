use basis_store::{
    ExtendedReserveInfo, ReserveTracker,
};
use hex;

#[tokio::test]
async fn test_server_key_status_logic() {
    // 1. Setup Tracker
    let tracker = ReserveTracker::new();
    
    // 2. Setup Reserve with Tokens
    let box_id = b"server_test_reserve";
    let owner_pubkey = [0x05u8; 33]; // Valid mocked pubkey
    let owner_pubkey_hex = hex::encode(owner_pubkey);
    
    let token_id = b"server_token_1234567890123456789";
    let token_amount = 5000u64;
    let collateral = 1_000_000u64;

    let reserve_info = ExtendedReserveInfo::new(
        box_id,
        &owner_pubkey,
        collateral,
        None,
        1000,
        Some(token_id),
        Some(token_amount),
    );
    
    tracker.update_reserve(reserve_info).unwrap();
    
    // 3. Simulate Logic from get_key_status API
    // (We can't call the API directly easily without spawning the full server, 
    // so we test the logic integration).
    
    let all_reserves = tracker.get_all_reserves();
    let reserve = all_reserves
        .into_iter()
        .find(|r| r.owner_pubkey == owner_pubkey_hex);
        
    assert!(reserve.is_some(), "Reserve should be found");
    let reserve = reserve.unwrap();
    
    // 4. Verify Collateralization Logic Integration
    // Case A: No Debt
    let ratio = reserve.collateralization_ratio();
    assert_eq!(ratio, f64::INFINITY);
    
    // Case B: With Debt (1000)
    // Manually add debt to tracker to verify state update
    tracker.add_debt(&hex::encode(box_id), 1000).unwrap();
    let updated_reserve = tracker.get_reserve(&hex::encode(box_id)).unwrap();
    
    // Ratio = Token Amount / Debt (since token exists) -> 5000 / 1000 = 5.0
    // OR Ratio = Collateral / Debt (if logic fell back) -> 1,000,000 / 1000 = 1000.0
    
    // Our updated logic in ExtendedReserveInfo prefers token amount
    assert_eq!(updated_reserve.collateralization_ratio(), 5.0);
    assert_eq!(updated_reserve.base_info.token_amount, Some(5000));
}
