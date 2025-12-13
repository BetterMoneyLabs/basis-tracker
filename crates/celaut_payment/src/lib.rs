use std::collections::HashMap;
use basis_store::IouNote;
use serde::{Deserialize, Serialize};

/// Unique identifier for a peer (hex-encoded public key)
pub type PeerId = String;

/// Currency type for payments
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum Currency {
    BasisIou,
    Token(String), // Token ID
}

/// Credit limit definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditLimit {
    pub limit: u64,
    pub currency: Currency,
}

/// State of a specific peer relation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerState {
    /// Trust score (0-100)
    pub trust_score: u8,
    /// Credit limits extended TO this peer (we trust them up to this amount)
    pub credit_limits: HashMap<Currency, u64>,
    /// Current net balance. Positive means they owe us, negative means we owe them.
    pub balances: HashMap<Currency, i64>,
}

/// Manager for Celaut payments and credit/trust lines
pub struct PaymentManager {
    /// Map of peer states
    peers: HashMap<PeerId, PeerState>,
}

#[derive(thiserror::Error, Debug)]
pub enum PaymentError {
    #[error("Peer not found: {0}")]
    PeerNotFound(String),
    #[error("Credit limit exceeded. Current balance: {balance}, Amount: {amount}, Limit: {limit}")]
    CreditLimitExceeded { balance: i64, amount: u64, limit: u64 },
    #[error("Invalid amount")]
    InvalidAmount,
}

impl PaymentManager {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    /// Register or update a peer
    pub fn add_peer(&mut self, peer_id: PeerId) {
        self.peers.entry(peer_id).or_default();
    }

    /// Set credit limit for a peer (how much we trust them aka how much they can owe us)
    pub fn set_credit_limit(&mut self, peer_id: &str, currency: Currency, limit: u64) {
        let peer = self.peers.entry(peer_id.to_string()).or_default();
        peer.credit_limits.insert(currency, limit);
    }

    /// Get current balance with a peer for a currency
    pub fn get_balance(&self, peer_id: &str, currency: &Currency) -> i64 {
        self.peers
            .get(peer_id)
            .and_then(|p| p.balances.get(currency).copied())
            .unwrap_or(0)
    }

    /// Record a payment FROM us TO a peer (Peer receives payment, so our debt increases / their debt decreases)
    /// This decreases the balance (they owe us less, or we owe them more).
    /// Usually, credit limits apply to *how much they owe us*.
    pub fn pay_peer(&mut self, peer_id: &str, currency: Currency, amount: u64) -> Result<(), PaymentError> {
        // When we pay someone, we are effectively reducing the amount they owe us, or increasing what we owe them.
        // This is generally safe regarding OUR risk limits (unless we have a "debt limit" we want to enforce on ourselves).
        let peer = self.peers.entry(peer_id.to_string()).or_default();
        let balance = peer.balances.entry(currency).or_insert(0);
        *balance -= amount as i64;
        Ok(())
    }

    /// Receive a payment FROM a peer (They pay us).
    /// This is where we might accept an IOU. If they pay with an IOU, they are asking us to hold their debt.
    /// This increases the amount they owe us (positive balance).
    /// We must check if this exceeds the credit limit we set for them.
    pub fn receive_payment_request(
        &mut self, 
        peer_id: &str, 
        currency: Currency, 
        amount: u64
    ) -> Result<(), PaymentError> {
        let peer = self.peers.entry(peer_id.to_string()).or_default();
        
        let current_balance = peer.balances.get(&currency).copied().unwrap_or(0);
        let limit = peer.credit_limits.get(&currency).copied().unwrap_or(0);
        
        // Calculate new projected balance
        // If they pay us with an IOU, they owe us MORE.
        // Note: This semantics depends on if "Receive Payment" means "They sent cash" or "They sent an IOU".
        // In the context of "Basis Offchain Notes", a payment IS an IOU.
        // So receiving a payment = holding more debt from them.
        
        let new_balance = current_balance + amount as i64;
        
        if new_balance > limit as i64 {
            return Err(PaymentError::CreditLimitExceeded { 
                balance: current_balance, 
                amount, 
                limit 
            });
        }
        
        *peer.balances.entry(currency).or_insert(0) = new_balance;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credit_limit_enforcement() {
        let mut manager = PaymentManager::new();
        let peer = "peer_A";
        let currency = Currency::BasisIou;
        
        manager.set_credit_limit(peer, currency.clone(), 1000);
        
        // 1. Receive payment (IOU) of 500. Should succeed. Balance: 500.
        assert!(manager.receive_payment_request(peer, currency.clone(), 500).is_ok());
        assert_eq!(manager.get_balance(peer, &currency), 500);
        
        // 2. Receive another 600. Total 1100 > 1000. Should fail.
        let result = manager.receive_payment_request(peer, currency.clone(), 600);
        assert!(matches!(result, Err(PaymentError::CreditLimitExceeded { .. })));
        assert_eq!(manager.get_balance(peer, &currency), 500); // Balance unchanged
        
        // 3. Receive 500. Total 1000. Should succeed.
        assert!(manager.receive_payment_request(peer, currency.clone(), 500).is_ok());
        assert_eq!(manager.get_balance(peer, &currency), 1000);
        
        // 4. We pay them 200 (reducing their debt to us). Balance: 800.
        assert!(manager.pay_peer(peer, currency.clone(), 200).is_ok());
        assert_eq!(manager.get_balance(peer, &currency), 800);
        
        // 5. Now they can pay 200 more.
        assert!(manager.receive_payment_request(peer, currency.clone(), 200).is_ok());
        assert_eq!(manager.get_balance(peer, &currency), 1000);
    }
}
