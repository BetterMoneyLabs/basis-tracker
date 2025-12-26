use celaut_payment::{Currency, PaymentManager, PeerId};
use basis_store::{IouNote, PubKey, Signature};

fn main() {
    println!("=== Agent Payment Showcase ===");

    // 1. Setup Agents
    println!("\n[Setup] Initializing agents...");
    let mut alice = Agent::new("Alice", "02alice..."); // Mock pubkey
    let mut bob = Agent::new("Bob", "02bob...");     // Mock pubkey
    let mut carol = Agent::new("Carol", "02carol..."); // Mock pubkey

    // 2. Establish Trust (Credit Limits)
    // Alice trusts Bob up to 1000 (Bob can owe Alice 1000)
    println!("\n[Trust] Alice extends 1000 credit limit to Bob");
    alice.pm.set_credit_limit(&bob.id, Currency::BasisIou, 1000);
    
    // Bob trusts Carol up to 500
    println!("[Trust] Bob extends 500 credit limit to Carol");
    bob.pm.set_credit_limit(&carol.id, Currency::BasisIou, 500);

    // 3. Execute Payments
    
    // Scenario A: Bob pays Alice 500
    println!("\n[Payment A] Bob pays Alice 500 (BasisIOU)");
    // In reality, Bob signs a note.
    let note_to_alice = create_mock_note(&bob.id, &alice.id, 500);
    
    // Alice receives the payment (IOU)
    match alice.pm.receive_payment_request(&bob.id, Currency::BasisIou, 500) {
        Ok(_) => println!("-> Alice accepted payment. Bob now owes Alice 500."),
        Err(e) => println!("-> Alice rejected payment: {}", e),
    }

    // Scenario B: Carol pays Bob 600 (Exceeds limit of 500)
    println!("\n[Payment B] Carol tries to pay Bob 600 (BasisIOU)");
    match bob.pm.receive_payment_request(&carol.id, Currency::BasisIou, 600) {
        Ok(_) => println!("-> Bob accepted payment."),
        Err(e) => println!("-> Bob rejected payment from Carol: {}", e),
    }
    
    // Scenario C: Carol pays Bob 300 (Within limit)
    println!("\n[Payment C] Carol tries to pay Bob 300 (BasisIOU)");
    match bob.pm.receive_payment_request(&carol.id, Currency::BasisIou, 300) {
        Ok(_) => println!("-> Bob accepted payment. Carol now owes Bob 300."),
        Err(e) => println!("-> Bob rejected payment from Carol: {}", e),
    }

    // 4. Report Status
    println!("\n=== Final Status ===");
    alice.report_status();
    bob.report_status();
    carol.report_status();
}

struct Agent {
    name: String,
    id: PeerId,
    pm: PaymentManager,
}

impl Agent {
    fn new(name: &str, id: &str) -> Self {
        Self {
            name: name.to_string(),
            id: id.to_string(),
            pm: PaymentManager::new(),
        }
    }

    fn report_status(&self) {
        println!("Agent {}:", self.name);
        println!("  Balances (Positive = Others owe me):");
        // Using a public method or just printing for demo. 
        // Need to expose iterating peers or specific check.
        // For demo simplicity, we'll check known peers.
        if self.name == "Alice" {
             println!("    vs Bob: {}", self.pm.get_balance("02bob...", &Currency::BasisIou));
        }
        if self.name == "Bob" {
             println!("    vs Alice: {}", self.pm.get_balance("02alice...", &Currency::BasisIou));
             println!("    vs Carol: {}", self.pm.get_balance("02carol...", &Currency::BasisIou));
        }
    }
}

fn create_mock_note(_issuer: &str, _recipient: &str, amount: u64) -> IouNote {
    // Return a dummy note for the demo
    IouNote::new(
        [0u8; 33], // Recipient Pubkey
        amount,
        0,
        0,
        [0u8; 65], // Signature
    )
}
