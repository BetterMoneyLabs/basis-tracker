use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeyStatusResponse {
    pub total_debt: u64,
    pub collateral: u64,
    pub collateralization_ratio: f64,
    pub note_count: usize,
    pub last_updated: u64,
    pub issuer_pubkey: String,
}

fn main() {
    // Test what happens when we have null values
    let json_with_null = r#"{
        "success": true,
        "data": {
            "total_debt": 0,
            "collateral": 0,
            "collateralization_ratio": null,
            "note_count": 0,
            "last_updated": 0,
            "issuer_pubkey": "test"
        },
        "error": null
    }"#;

    match serde_json::from_str::<ApiResponse<KeyStatusResponse>>(json_with_null) {
        Ok(response) => {
            println!("Success: {:?}", response);
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}