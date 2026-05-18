use std::path::Path;

fn main() {
    // Update tracker box in database
    let tracker_storage_path = Path::new("data/tracker_boxes");
    let tracker_storage = basis_store::persistence::TrackerStorage::open(tracker_storage_path)
        .expect("Failed to open tracker storage");

    let tracker_box = basis_store::TrackerBoxInfo {
        box_id: "4935e408cd0512498e45e1751da77201c751738b3d053e2656f8e49f3acbd1eb".to_string(),
        tracker_pubkey: "024e564477ff457c601c01ad1cc31903f8b27b7d5e515bd03138891d8152d787b2".to_string(),
        state_commitment: "64000000000000000000000000000000000000000000000000000000000000000001".to_string(),
        last_verified_height: 1785096,
        value: 100000,
        creation_height: 1785096,
        tracker_nft_id: "000b0695159e5f5c32c606385bd5f276d80133149c84c8b1325366381bf6f17f".to_string(),
    };

    tracker_storage.store_tracker_box(&tracker_box)
        .expect("Failed to store tracker box");

    println!("Tracker box updated successfully");
}
