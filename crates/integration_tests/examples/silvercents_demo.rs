use basis_server::models::{CreateNoteRequest, SerializableIouNote};
use basis_store::{IouNote, PubKey};
use std::time::{SystemTime, UNIX_EPOCH};

// Mock DexySilver Token ID
const DEXY_SILVER_TOKEN_ID: &str = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

/// SilverCents Console Manager - Simulates CLI interaction
struct SilverConsole {
    role: String,
}

impl SilverConsole {
    fn new(role: &str) -> Self {
        Self { role: role.to_string() }
    }

    fn log(&self, message: &str) {
        println!("[{}] {}", self.role, message);
    }

    fn print_header(&self, title: &str) {
        println!("\n=== {} {} ===", self.role, title);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting SilverCents Demo Environment...\n");

    // 1. Setup Identities
    let vendor_sk = [1u8; 32];
    let vendor_pk = get_pubkey(&vendor_sk);
    let customer_sk = [2u8; 32];
    let customer_pk = get_pubkey(&customer_sk);

    let vendor_console = SilverConsole::new("Vendor (Bob's Farm)");
    let customer_console = SilverConsole::new("Customer (Alice)");

    // 2. Reserve Creation (Simulated)
    vendor_console.print_header("Reserve Initialization");
    vendor_console.log("Checking for On-Chain Reserves...");
    vendor_console.log(&format!("Found Reserve #101 backed by:"));
    vendor_console.log(&format!("  - 10.0 ERG"));
    vendor_console.log(&format!("  - 500 DexySilver Tokens ({})", DEXY_SILVER_TOKEN_ID));
    vendor_console.log("Collateralization Ratio: 250% (Excellent)");

    // 3. Issuance Flow
    vendor_console.print_header("Issuance");
    vendor_console.log("Processing purchase for 'Organic Apples'");
    vendor_console.log("Issuing 10 SilverCents to Alice...");

    let note = create_silver_cent_note(&vendor_sk, &customer_pk, 10);
    vendor_console.log(&format!("Signed Note: {}", hex::encode(&note.signature)[..16]));

    // 4. Customer Receipt
    customer_console.print_header("Wallet");
    customer_console.log("New Note Received!");
    customer_console.log(&format!("Issuer: Bob's Farm ({})", hex::encode(vendor_pk)[..8]));
    customer_console.log("Amount: 10 SilverCents");
    customer_console.log("Backing: DexySilver + ERG");

    // 5. Redemption Flow
    customer_console.print_header("Redemption");
    customer_console.log("Requesting redemption for physical coins...");
    customer_console.log("Redeeming 10 SilverCents for 1 Silver Quarter (approx)");
    
    // Simulate redemption handshake
    vendor_console.log("Redemption request received.");
    vendor_console.log("Verifying note signature... Valid.");
    vendor_console.log("Checking silver inventory... Available.");
    vendor_console.log("ACTION: Dispensing 1 Silver Quarter to Alice.");
    
    customer_console.log("Physical coin received. Transaction Closed.");

    println!("\nDemo Complete.");
    Ok(())
}

fn get_pubkey(secret: &[u8; 32]) -> PubKey {
    use secp256k1::{Secp256k1, SecretKey, PublicKey};
    let secp = Secp256k1::new();
    let sk = SecretKey::from_slice(secret).unwrap();
    let pk = PublicKey::from_secret_key(&secp, &sk);
    pk.serialize()
}

fn create_silver_cent_note(
    issuer_sk: &[u8; 32], 
    recipient_pk: &PubKey, 
    amount: u64
) -> IouNote {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    IouNote::create_and_sign(*recipient_pk, amount, timestamp, issuer_sk).unwrap()
}
