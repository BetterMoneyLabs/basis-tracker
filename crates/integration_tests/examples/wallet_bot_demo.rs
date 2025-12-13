use serde::{Deserialize, Serialize};

// --- Models (Mirrors of Server API) ---

#[derive(Debug, Deserialize)]
struct WalletSummary {
    pubkey: String,
    total_debt: u64,
    collateral: u64,
    collateralization_ratio: f64,
    token_id: Option<String>,
    token_amount: Option<u64>,
    note_count: usize,
    recent_activity: Vec<WalletActivityItem>,
}

#[derive(Debug, Deserialize)]
struct WalletActivityItem {
    timestamp: u64,
    activity_type: String,
    other_party: String,
    amount: u64,
    details: String,
}

#[derive(Debug, Serialize)]
struct SimplePaymentRequest {
    sender_pubkey: String,
    recipient_pubkey: String,
    amount: u64,
    timestamp: u64,
    signature: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

// --- Bot Logic ---

struct WalletBot {
    api_url: String,
    client: reqwest::Client,
}

impl WalletBot {
    fn new(api_url: &str) -> Self {
        Self {
            api_url: api_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    async fn check_balance(&self, pubkey: &str) {
        println!("\n[Bot] Checking balance for {}", pubkey);
        let url = format!("{}/wallet/{}/summary", self.api_url, pubkey);
        
        // Simulating HTTP request
        // let resp = self.client.get(&url).send().await...
        
        // Mock Response for Demo
        let mock_summary = WalletSummary {
            pubkey: pubkey.to_string(),
            total_debt: 1500,
            collateral: 1_000_000,
            collateralization_ratio: 666.6,
            token_id: None,
            token_amount: None,
            note_count: 5,
            recent_activity: vec![
                WalletActivityItem {
                    timestamp: 1234567890,
                    activity_type: "outgoing_payment".to_string(),
                    other_party: "02bob...".to_string(),
                    amount: 500,
                    details: "Payment to Bob".to_string(),
                }
            ],
        };

        println!("  -> Debt: {}", mock_summary.total_debt);
        println!("  -> Collateral: {}", mock_summary.collateral);
        println!("  -> Ratio: {:.2}", mock_summary.collateralization_ratio);
        println!("  -> Recent Activity: {} items", mock_summary.recent_activity.len());
        for item in mock_summary.recent_activity {
            println!("     - {} | {} | {}", item.activity_type, item.amount, item.details);
        }
    }

    async fn pay(&self, sender: &str, recipient: &str, amount: u64) {
        println!("\n[Bot] Sending payment: {} -> {} ({} IOU)", sender, recipient, amount);
        let url = format!("{}/wallet/pay", self.api_url);
        
        let req = SimplePaymentRequest {
            sender_pubkey: sender.to_string(),
            recipient_pubkey: recipient.to_string(),
            amount,
            timestamp: 1234567900,
            signature: "dummy_sig".to_string(),
        };

        // Simulating HTTP Post
        // let resp = self.client.post(&url).json(&req).send().await...
        
        println!("  -> Payment broadcasted via API.");
    }
}

#[tokio::main]
async fn main() {
    println!("=== Wallet Bot API Demo ===");
    
    let bot = WalletBot::new("http://localhost:3048");
    let user_pubkey = "02alice...";
    let merchant_pubkey = "02merchant...";

    // 1. Check Balance
    bot.check_balance(user_pubkey).await;

    // 2. Pay Merchant
    bot.pay(user_pubkey, merchant_pubkey, 100).await;
    
    // 3. Check Balance again
    bot.check_balance(user_pubkey).await;
}
